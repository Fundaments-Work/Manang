//! The `live` plugin: commands over one session at a time.

use std::path::PathBuf;
use std::sync::Arc;

use manang_editor::{DocId, Editor};
use manang_kernel::{Context, EventBus, Payload, Plugin, PluginError};
use manang_live::bridge::LiveDoc;
use manang_live::envelope::{Entropy, Invite, OsEntropy, RoomSecret, seed_fingerprint};
use manang_live::seed::{JoinPlan, plan_join};
use manang_live::session::SessionConfig;
use parking_lot::Mutex;
use ropey::Rope;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::runtime::{Builder, Handle, Runtime};

use crate::driver::{Change, DriverHandle, DriverParams, PushOutcome};
use crate::error::LiveHostError;
use crate::invite::{InviteLink, normalize_relay, room_url};

/// Values of the plugin's settings.
#[derive(Debug, Clone, Default)]
struct Settings {
    relay_url: String,
    display_name: String,
}

struct Active {
    handle: Arc<DriverHandle>,
    session: String,
    role: &'static str,
    invite: String,
    file: Option<String>,
    doc: DocId,
    relay: String,
}

struct Inner {
    editor: Arc<Editor>,
    events: EventBus,
    /// Where a replaced copy of a file is saved before a join overwrites it.
    config_dir: PathBuf,
    settings: Mutex<Settings>,
    runtime: Mutex<Option<Runtime>>,
    active: Mutex<Option<Active>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        // Dropping a runtime from async code panics; this never waits.
        if let Some(runtime) = self.runtime.lock().take() {
            runtime.shutdown_background();
        }
    }
}

/// The plugin. Cheap to clone-share behind an `Arc`.
pub struct LivePlugin {
    inner: Arc<Inner>,
}

impl LivePlugin {
    /// `editor` is the kernel's editor plugin state (the shared copy of every
    /// open file); `config_dir` is the app's config directory.
    #[must_use]
    pub fn new(editor: Arc<Editor>, events: EventBus, config_dir: PathBuf) -> Self {
        Self {
            inner: Arc::new(Inner {
                editor,
                events,
                config_dir,
                settings: Mutex::new(Settings::default()),
                runtime: Mutex::new(None),
                active: Mutex::new(None),
            }),
        }
    }
}

#[derive(Deserialize)]
struct ChangeArg {
    start: usize,
    #[serde(default)]
    delete: usize,
    #[serde(default)]
    insert: String,
}

#[derive(Deserialize)]
struct PushArgs {
    session: Option<String>,
    version: u64,
    changes: Vec<ChangeArg>,
}

fn json_args(input: Payload) -> Result<Value, LiveHostError> {
    match input {
        Payload::Json(v) => Ok(v),
        Payload::Empty => Ok(Value::Null),
        Payload::Binary(_) => Err(LiveHostError::Invalid("expected a JSON payload".to_owned())),
    }
}

fn parse<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, LiveHostError> {
    serde_json::from_value(value).map_err(|e| LiveHostError::Invalid(e.to_string()))
}

fn doc_arg(args: &Value) -> Result<DocId, LiveHostError> {
    args.get("doc")
        .and_then(Value::as_u64)
        .ok_or_else(|| LiveHostError::Invalid("`doc` must be an unsigned integer".to_owned()))
}

fn random_hex(bytes: usize) -> Result<String, LiveHostError> {
    let mut buf = vec![0u8; bytes];
    OsEntropy.fill(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// A fresh peer id: unique per replica, below 2^53 (yrs compatible) and not
/// the seed author's.
fn random_peer() -> Result<u64, LiveHostError> {
    let mut buf = [0u8; 8];
    OsEntropy.fill(&mut buf)?;
    let id = u64::from_le_bytes(buf) & ((1 << 53) - 1);
    Ok(if id <= manang_live::SEED_PEER {
        id + 2
    } else {
        id
    })
}

fn clean_name(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect::<String>()
        .trim()
        .to_owned()
}

impl Inner {
    fn handle(&self) -> Result<Handle, LiveHostError> {
        let mut slot = self.runtime.lock();
        if slot.is_none() {
            let runtime = Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("manang-live-io")
                .enable_all()
                .build()
                .map_err(|e| LiveHostError::Io(e.to_string()))?;
            *slot = Some(runtime);
        }
        slot.as_ref()
            .map(|r| r.handle().clone())
            .ok_or(LiveHostError::Stopped)
    }

    fn display_name(&self) -> String {
        let chosen = clean_name(&self.settings.lock().display_name);
        if !chosen.is_empty() {
            return chosen;
        }
        for var in ["USER", "USERNAME"] {
            if let Ok(name) = std::env::var(var) {
                let name = clean_name(&name);
                if !name.is_empty() {
                    return name;
                }
            }
        }
        "Guest".to_owned()
    }

    fn session_config(&self, peer: u64) -> SessionConfig {
        SessionConfig {
            name: self.display_name(),
            slot: u8::try_from(peer % u64::from(manang_live::presence::PALETTE_SLOTS)).unwrap_or(0),
            ..SessionConfig::default()
        }
    }

    fn ensure_idle(&self) -> Result<(), LiveHostError> {
        if self.active.lock().is_some() {
            return Err(LiveHostError::AlreadyLive);
        }
        Ok(())
    }

    fn start(&self, args: &Value) -> Result<Value, LiveHostError> {
        self.ensure_idle()?;
        let relay = normalize_relay(&self.settings.lock().relay_url)?;
        let doc = doc_arg(args)?;
        let file = args.get("file").and_then(Value::as_str).map(str::to_owned);
        let text = self
            .editor
            .text(Some(doc))
            .map_err(|e| LiveHostError::Editor(e.to_string()))?;

        let secret = RoomSecret::generate(&mut OsEntropy)?;
        let keys = secret.derive()?;
        let room_hex = keys.room_id_hex();
        let fragment = Invite {
            secret,
            seed: seed_fingerprint(&text),
        }
        .to_fragment();
        let invite = InviteLink {
            relay: relay.clone(),
            file: file.clone(),
            fragment,
        }
        .render()?;

        let peer = random_peer()?;
        let session = random_hex(6)?;
        let handle = DriverHandle::spawn(DriverParams {
            session_id: session.clone(),
            doc,
            editor: Arc::clone(&self.editor),
            events: self.events.clone(),
            handle: self.handle()?,
            room_url: room_url(&relay, &room_hex),
            live_doc: LiveDoc::seeded(peer, &text)?,
            rope: Rope::from_str(&text),
            keys,
            config: self.session_config(peer),
        })?;
        *self.active.lock() = Some(Active {
            handle: Arc::new(handle),
            session: session.clone(),
            role: "host",
            invite: invite.clone(),
            file,
            doc,
            relay,
        });
        Ok(json!({ "session": session, "version": 0, "role": "host", "invite": invite }))
    }

    fn join(&self, args: &Value) -> Result<Value, LiveHostError> {
        self.ensure_idle()?;
        let doc = doc_arg(args)?;
        let link = InviteLink::parse(args.get("invite").and_then(Value::as_str).unwrap_or(""))?;
        let invite = Invite::from_fragment(&link.fragment)
            .map_err(|e| LiveHostError::BadInvite(e.to_string()))?;
        let keys = invite.secret.derive()?;
        let take_host = args.get("resolution").and_then(Value::as_str) == Some("take-host");
        let name = args.get("name").and_then(Value::as_str).unwrap_or("file");

        let local = self
            .editor
            .text(Some(doc))
            .map_err(|e| LiveHostError::Editor(e.to_string()))?;
        let peer = random_peer()?;
        let (live_doc, rope, reset) = match plan_join(invite.seed, &local, None) {
            JoinPlan::SeedLocally => (LiveDoc::seeded(peer, &local)?, Rope::from_str(&local), None),
            plan => {
                // An empty copy has nothing to lose; anything else must be
                // confirmed, and is saved before it is replaced.
                if !local.is_empty() && plan == JoinPlan::ChooseResolution && !take_host {
                    return Err(LiveHostError::Differs);
                }
                if !local.is_empty() {
                    self.save_replaced_copy(name, &local)?;
                }
                (LiveDoc::empty(peer)?, Rope::new(), Some(local.clone()))
            }
        };
        let mut change = None;
        if let Some(old) = &reset
            && !old.is_empty()
        {
            let chars = old.chars().count();
            self.editor
                .replace_silent(Some(doc), 0, chars, "")
                .map_err(|e| LiveHostError::Editor(e.to_string()))?;
            change = Some(json!({
                "start": 0,
                "delete": old.encode_utf16().count(),
                "insert": "",
            }));
        }

        let session = random_hex(6)?;
        let room_hex = keys.room_id_hex();
        let spawned = self.handle().and_then(|runtime| {
            DriverHandle::spawn(DriverParams {
                session_id: session.clone(),
                doc,
                editor: Arc::clone(&self.editor),
                events: self.events.clone(),
                handle: runtime,
                room_url: room_url(&link.relay, &room_hex),
                live_doc,
                rope,
                keys,
                config: self.session_config(peer),
            })
        });
        let handle = match spawned {
            Ok(handle) => handle,
            Err(e) => {
                if change.is_some() {
                    // Put back what was blanked: nothing changed after all.
                    let _ = self.editor.replace_silent(Some(doc), 0, 0, &local);
                }
                return Err(e);
            }
        };
        let invite_text = link.render()?;
        *self.active.lock() = Some(Active {
            handle: Arc::new(handle),
            session: session.clone(),
            role: "guest",
            invite: invite_text,
            file: link.file.clone(),
            doc,
            relay: link.relay,
        });
        Ok(json!({
            "session": session,
            "version": 0,
            "role": "guest",
            "file": link.file,
            "reset": change,
        }))
    }

    /// Save the copy a join is about to replace, outside the project.
    fn save_replaced_copy(&self, name: &str, text: &str) -> Result<(), LiveHostError> {
        let dir = self.config_dir.join("live-trash");
        std::fs::create_dir_all(&dir).map_err(|e| LiveHostError::Io(e.to_string()))?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .take(80)
            .collect();
        std::fs::write(dir.join(format!("{stamp}-{safe}")), text)
            .map_err(|e| LiveHostError::Io(e.to_string()))
    }

    fn active_handle(&self, session: Option<&str>) -> Result<Arc<DriverHandle>, LiveHostError> {
        let guard = self.active.lock();
        let active = guard.as_ref().ok_or(LiveHostError::NotLive)?;
        if let Some(wanted) = session
            && wanted != active.session
        {
            return Err(LiveHostError::NotLive);
        }
        Ok(Arc::clone(&active.handle))
    }

    fn status(&self) -> Value {
        let relay_set = !self.settings.lock().relay_url.trim().is_empty();
        let guard = self.active.lock();
        let Some(active) = guard.as_ref() else {
            return json!({ "active": false, "relay_set": relay_set });
        };
        let status = active.handle.status();
        json!({
            "active": true,
            "relay_set": relay_set,
            "session": active.session,
            "role": active.role,
            "doc": active.doc,
            "file": active.file,
            "relay": active.relay,
            "state": status.state,
            "attempt": status.attempt,
            "error": status.error,
            "version": status.version,
            "rejected": status.rejected,
            "peers": status.peers.iter().map(|p| json!({
                "peer": p.peer.to_string(), "name": p.name, "slot": p.slot,
            })).collect::<Vec<_>>(),
        })
    }

    fn leave(&self) -> Value {
        let active = self.active.lock().take();
        match active {
            Some(active) => {
                active.handle.leave();
                json!({ "left": true })
            }
            None => json!({ "left": false }),
        }
    }

    fn push(&self, args: Value) -> Result<Value, LiveHostError> {
        let a: PushArgs = parse(args)?;
        let handle = self.active_handle(a.session.as_deref())?;
        let changes = a
            .changes
            .into_iter()
            .map(|c| Change {
                start: c.start,
                delete: c.delete,
                insert: c.insert,
            })
            .collect();
        Ok(match handle.push(a.version, changes)? {
            PushOutcome::Accepted(version) => json!({ "accepted": true, "version": version }),
            PushOutcome::Stale(version) => json!({ "accepted": false, "version": version }),
        })
    }

    fn selection(&self, args: &Value) -> Result<Value, LiveHostError> {
        let handle = self.active_handle(args.get("session").and_then(Value::as_str))?;
        let version = args
            .get("version")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX);
        let part = |key: &str| {
            args.get(key)
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
        };
        let selection = part("anchor").zip(part("head"));
        handle.selection(version, selection);
        Ok(json!({}))
    }

    fn configure(&self, args: &Value) -> Result<Value, LiveHostError> {
        let values = args
            .get("values")
            .and_then(Value::as_object)
            .ok_or_else(|| LiveHostError::Invalid("`values` must be an object".to_owned()))?;
        let mut next = self.settings.lock().clone();
        if let Some(v) = values.get("live.relay-url").and_then(Value::as_str) {
            v.clone_into(&mut next.relay_url);
        }
        if let Some(v) = values.get("live.display-name").and_then(Value::as_str) {
            v.clone_into(&mut next.display_name);
        }
        *self.settings.lock() = next;
        Ok(json!({ "applied": values.len() }))
    }
}

impl Plugin for LivePlugin {
    fn invoke(
        &self,
        _ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        let args = json_args(input)?;
        let inner = &self.inner;
        let out = match command {
            "live.start" => inner.start(&args)?,
            "live.join" => inner.join(&args)?,
            "live.leave" => inner.leave(),
            "live.status" => inner.status(),
            "live.invite" => {
                let guard = inner.active.lock();
                let active = guard.as_ref().ok_or(LiveHostError::NotLive)?;
                json!({ "invite": active.invite, "file": active.file })
            }
            "live.inspect" => {
                let link =
                    InviteLink::parse(args.get("invite").and_then(Value::as_str).unwrap_or(""))?;
                json!({ "relay": link.relay, "file": link.file })
            }
            "live.push" => inner.push(args)?,
            "live.selection" => inner.selection(&args)?,
            "live.snapshot" => {
                let handle = inner.active_handle(args.get("session").and_then(Value::as_str))?;
                let (version, text) = handle.snapshot()?;
                json!({ "version": version, "text": text })
            }
            "live.contributors" => {
                let handle = inner.active_handle(None)?;
                let list = handle.contributors()?;
                json!({ "contributors": list.iter().map(|c| json!({
                    "name": c.name, "email": c.email,
                })).collect::<Vec<_>>() })
            }
            "live.configure" => inner.configure(&args)?,
            other => return Err(PluginError::UnknownCommand(other.to_owned())),
        };
        Ok(Payload::Json(out))
    }
}
