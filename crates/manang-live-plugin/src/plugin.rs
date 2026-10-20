//! The `live` plugin: commands over one session at a time.

use std::path::PathBuf;
use std::sync::Arc;

use manang_editor::{DocId, Editor};
use manang_kernel::{Context, EventBus, Payload, Plugin, PluginError};
use manang_live::LoroEngine;
use manang_live::bridge::LiveDoc;
use manang_live::envelope::{Entropy, Invite, OsEntropy, RoomSecret, seed_fingerprint};
use manang_live::seed::{JoinPlan, plan_join};
use manang_live::session::SessionConfig;
use manang_live::vcs::coauthor_trailers;
use parking_lot::Mutex;
use ropey::Rope;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::runtime::{Builder, Handle, Runtime};

use crate::driver::{Change, DriverHandle, DriverParams, PushOutcome, random_peer};
use crate::error::LiveHostError;
use crate::invite::{InviteLink, normalize_relay, room_url};
use crate::store::{Snapshots, Trash};

/// The relay run by Fundaments; the default of `live.relay-url`. It only ever
/// sees ciphertext, and the setting can point anywhere or be emptied.
const DEFAULT_RELAY: &str = "wss://relay.fundaments.work";

/// Values of the plugin's settings.
#[derive(Debug, Clone)]
struct Settings {
    relay_url: String,
    display_name: String,
    /// Shared with collaborators for `Co-authored-by:` credit; empty shares none.
    email: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            relay_url: DEFAULT_RELAY.to_owned(),
            display_name: String::new(),
            email: String::new(),
        }
    }
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
    /// Where disposable document snapshots live (the app data directory).
    data_dir: PathBuf,
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
                data_dir: config_dir.clone(),
                config_dir,
                settings: Mutex::new(Settings::default()),
                runtime: Mutex::new(None),
                active: Mutex::new(None),
            }),
        }
    }
}

impl LivePlugin {
    /// Keep document snapshots in `dir` (the app data directory) instead of
    /// the config directory. Call before the plugin is registered.
    #[must_use]
    pub fn with_data_dir(mut self, dir: PathBuf) -> Self {
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.data_dir = dir;
        }
        self
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

    fn session_config(&self, peer: u64, await_host: bool) -> SessionConfig {
        let email = self.settings.lock().email.trim().to_owned();
        SessionConfig {
            await_host,
            email: (!email.is_empty()).then_some(email),
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
            config: self.session_config(peer, false),
            replace: None,
            trash: None,
            snapshots: Some((self.snapshots(), room_hex)),
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
        let room_hex = keys.room_id_hex();
        let snapshots = self.snapshots();
        let trash = Trash {
            dir: self.config_dir.join("live-trash"),
            name: name.to_owned(),
        };
        // A snapshot of this very room, equal to the file as it is now, lets a
        // document outlive everyone leaving; it is never trusted over the disk.
        let restored = snapshots.restore::<LoroEngine>(&room_hex, peer, &local);
        let (live_doc, rope, replace, await_host) = if let Some(doc) = restored {
            (doc, Rope::from_str(&local), None, false)
        } else {
            match plan_join(invite.seed, &local, None) {
                JoinPlan::SeedLocally => (
                    LiveDoc::seeded(peer, &local)?,
                    Rope::from_str(&local),
                    None,
                    true,
                ),
                plan => {
                    // An empty copy has nothing to lose; anything else must be
                    // confirmed, and is saved before it can be replaced. It is
                    // replaced only once a host has answered.
                    if !local.is_empty() && plan == JoinPlan::ChooseResolution && !take_host {
                        return Err(LiveHostError::Differs);
                    }
                    if !local.is_empty() {
                        trash.save(&local, "join")?;
                    }
                    (LiveDoc::empty(peer)?, Rope::new(), Some(local), true)
                }
            }
        };

        let session = random_hex(6)?;
        let handle = DriverHandle::spawn(DriverParams {
            session_id: session.clone(),
            doc,
            editor: Arc::clone(&self.editor),
            events: self.events.clone(),
            handle: self.handle()?,
            room_url: room_url(&link.relay, &room_hex),
            live_doc,
            rope,
            keys,
            config: self.session_config(peer, await_host),
            replace,
            trash: Some(trash),
            snapshots: Some((snapshots, room_hex)),
        })?;
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
            "waiting": await_host,
            "reset": Value::Null,
        }))
    }

    fn snapshots(&self) -> Snapshots {
        Snapshots::new(self.data_dir.join("live-snapshots"))
    }

    /// Rebuild the document from the others in the room.
    fn resync(&self) -> Result<Value, LiveHostError> {
        let handle = self.active_handle(None)?;
        handle.resync(random_peer()?)?;
        Ok(json!({ "resyncing": true }))
    }

    /// Who edited since the last call, with ready-to-append commit trailers.
    fn contributors(&self, args: &Value) -> Result<Value, LiveHostError> {
        let handle = self.active_handle(None)?;
        let list = handle.contributors()?;
        let text = |key: &str| args.get(key).and_then(Value::as_str);
        let plan = coauthor_trailers(&list, text("committer_email"), text("committer_name"));
        Ok(json!({
            "contributors": list.iter().map(|c| json!({
                "name": c.name, "email": c.email,
            })).collect::<Vec<_>>(),
            "trailers": plan.trailers,
            "without_email": plan.without_email,
        }))
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
        if let Some(v) = values.get("live.email").and_then(Value::as_str) {
            v.clone_into(&mut next.email);
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
            "live.contributors" => inner.contributors(&args)?,
            "live.resync" => inner.resync()?,
            "live.configure" => inner.configure(&args)?,
            other => return Err(PluginError::UnknownCommand(other.to_owned())),
        };
        Ok(Payload::Json(out))
    }
}
