//! The `live` plugin: commands over one session at a time.

use std::path::PathBuf;
use std::sync::Arc;

use manang_editor::{DocId, Editor};
use manang_kernel::{Context, EventBus, Payload, Plugin, PluginError};
use manang_live::LoroEngine;
use manang_live::bridge::LiveDoc;
use manang_live::envelope::{Entropy, Invite, OsEntropy, RoomSecret, seed_fingerprint};
use manang_live::manifest::manifest_root;
use manang_live::project::ProjectDoc;
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
use crate::invite::{InviteLink, InviteScope, normalize_relay, room_url};
use crate::pathsec::PathPolicy;
use crate::projdrv::{Action, Choice, ProjectHandle, ProjectParams, Role};
use crate::scan::{build_project, scan_project};
use crate::sidecar::{DEFAULT_LOADED_DOCS, SidecarStore};
use crate::store::{Snapshots, Trash};

/// The relay run by Fundaments; the default of `live.relay-url`. It only ever
/// sees ciphertext, and the setting can point anywhere or be emptied.
const DEFAULT_RELAY: &str = "wss://relay.fundaments.work";

/// Events of the project watcher a session may have queued before the thread
/// reads them (a checkout can report thousands of paths in a few batches).
const FS_EVENT_QUEUE: usize = 4096;

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

/// The running session: one file, or a whole project.
#[derive(Clone)]
enum Running {
    File(Arc<DriverHandle>),
    Project(Arc<ProjectHandle>),
}

struct Active {
    running: Running,
    session: String,
    role: &'static str,
    invite: String,
    file: Option<String>,
    doc: DocId,
    relay: String,
    /// Which peer this window follows (local state, echoed by `live.follow`).
    following: Option<String>,
}

/// Where the shell says the open project folder is.
type RootSource = Arc<dyn Fn() -> Option<PathBuf> + Send + Sync>;

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
    /// Most documents a project session keeps in memory.
    loaded_docs: usize,
    /// Fallback for the project folder when a command names none.
    root_source: Option<RootSource>,
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
                loaded_docs: DEFAULT_LOADED_DOCS,
                root_source: None,
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

impl LivePlugin {
    /// Keep at most `n` documents of a project session in memory (the rest
    /// are parked in the sidecar). Call before the plugin is registered.
    #[must_use]
    pub fn with_loaded_docs(mut self, n: usize) -> Self {
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.loaded_docs = n.max(1);
        }
        self
    }

    /// Tell the plugin where the open project folder is, for project
    /// sessions started or joined without an explicit `root` argument. Call
    /// before the plugin is registered.
    #[must_use]
    pub fn with_project_root(
        mut self,
        source: impl Fn() -> Option<PathBuf> + Send + Sync + 'static,
    ) -> Self {
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.root_source = Some(Arc::new(source));
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
    #[serde(default)]
    path: Option<String>,
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

fn str_arg(args: &Value, key: &str) -> Result<String, LiveHostError> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| LiveHostError::Invalid(format!("`{key}` must be a string")))
}

fn path_required() -> LiveHostError {
    LiveHostError::Invalid("`path` is required in a project session".to_owned())
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
        if args.get("scope").and_then(Value::as_str) == Some("project") {
            return self.start_project(args);
        }
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
            scope: InviteScope::File,
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
            running: Running::File(Arc::new(handle)),
            session: session.clone(),
            role: "host",
            invite: invite.clone(),
            file,
            doc,
            relay,
            following: None,
        });
        Ok(json!({ "session": session, "version": 0, "role": "host", "invite": invite }))
    }

    fn join(&self, args: &Value) -> Result<Value, LiveHostError> {
        self.ensure_idle()?;
        let link = InviteLink::parse(args.get("invite").and_then(Value::as_str).unwrap_or(""))?;
        if link.scope == InviteScope::Project {
            return self.join_project(args, &link);
        }
        let doc = doc_arg(args)?;
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
            running: Running::File(Arc::new(handle)),
            session: session.clone(),
            role: "guest",
            invite: invite_text,
            file: link.file.clone(),
            doc,
            relay: link.relay,
            following: None,
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

    /// The project folder: the command's `root`, else the shell's source.
    fn root_of(&self, args: &Value) -> Result<PathBuf, LiveHostError> {
        if let Some(root) = args.get("root").and_then(Value::as_str)
            && !root.trim().is_empty()
        {
            return Ok(PathBuf::from(root));
        }
        self.root_source
            .as_ref()
            .and_then(|source| source())
            .ok_or(LiveHostError::NoProject)
    }

    fn policy_for(args_root: &std::path::Path) -> Result<PathPolicy, LiveHostError> {
        PathPolicy::new(args_root).map_err(|e| LiveHostError::BadPath(e.to_string()))
    }

    /// Share the whole folder: build the tree from disk and open the room.
    fn start_project(&self, args: &Value) -> Result<Value, LiveHostError> {
        self.ensure_idle()?;
        let relay = normalize_relay(&self.settings.lock().relay_url)?;
        let root = self.root_of(args)?;
        let policy = Self::policy_for(&root)?;
        let scan = scan_project(&policy).map_err(|e| LiveHostError::Io(e.to_string()))?;
        let peer = random_peer()?;
        let (project, _ids) = build_project(peer, &scan)?;
        let root_hash = manifest_root(&project.view().manifest());
        let files = scan
            .items
            .iter()
            .filter(|i| i.kind != manang_live::project::NodeKind::Dir)
            .count();

        let secret = RoomSecret::generate(&mut OsEntropy)?;
        let keys = secret.derive_project()?;
        let room_hex = keys.room_id_hex();
        let fragment = Invite::for_project(secret, root_hash).to_fragment();
        let invite = InviteLink {
            scope: InviteScope::Project,
            relay: relay.clone(),
            file: None,
            fragment,
        }
        .render()?;

        let session = random_hex(6)?;
        let handle = ProjectHandle::spawn(ProjectParams {
            session_id: session.clone(),
            role: Role::Host,
            policy,
            editor: Arc::clone(&self.editor),
            events: self.events.clone(),
            fs_events: self
                .events
                .subscribe_with_capacity("project.changed", FS_EVENT_QUEUE),
            handle: self.handle()?,
            room_url: room_url(&relay, &room_hex),
            room_hex,
            keys,
            config: self.session_config(peer, false),
            project,
            sidecar: SidecarStore::new(&self.data_dir),
            loaded_docs: self.loaded_docs,
        })?;
        *self.active.lock() = Some(Active {
            running: Running::Project(Arc::new(handle)),
            session: session.clone(),
            role: "host",
            invite: invite.clone(),
            file: None,
            doc: 0,
            relay,
            following: None,
        });
        Ok(json!({
            "session": session,
            "version": 0,
            "role": "host",
            "scope": "project",
            "invite": invite,
            "files": files,
            "skipped": scan.skipped,
        }))
    }

    /// Phase 1 of joining a project: connect and fetch the manifest. Nothing
    /// on disk changes until `live.resolve`.
    fn join_project(&self, args: &Value, link: &InviteLink) -> Result<Value, LiveHostError> {
        let invite = Invite::from_fragment(&link.fragment)
            .map_err(|e| LiveHostError::BadInvite(e.to_string()))?;
        let keys = invite.secret.derive_project()?;
        let root = self.root_of(args)?;
        let policy = Self::policy_for(&root)?;
        let peer = random_peer()?;
        let room_hex = keys.room_id_hex();
        let session = random_hex(6)?;
        let handle = ProjectHandle::spawn(ProjectParams {
            session_id: session.clone(),
            role: Role::Guest,
            policy,
            editor: Arc::clone(&self.editor),
            events: self.events.clone(),
            fs_events: self
                .events
                .subscribe_with_capacity("project.changed", FS_EVENT_QUEUE),
            handle: self.handle()?,
            room_url: room_url(&link.relay, &room_hex),
            room_hex,
            keys,
            config: self.session_config(peer, true),
            project: ProjectDoc::new(peer)?,
            sidecar: SidecarStore::new(&self.data_dir),
            loaded_docs: self.loaded_docs,
        })?;
        let invite_text = link.render()?;
        *self.active.lock() = Some(Active {
            running: Running::Project(Arc::new(handle)),
            session: session.clone(),
            role: "guest",
            invite: invite_text,
            file: None,
            doc: 0,
            relay: link.relay.clone(),
            following: None,
        });
        Ok(json!({
            "session": session,
            "version": 0,
            "role": "guest",
            "scope": "project",
            "file": Value::Null,
            "waiting": true,
            "reset": Value::Null,
        }))
    }

    fn snapshots(&self) -> Snapshots {
        Snapshots::new(self.data_dir.join("live-snapshots"))
    }

    /// Rebuild the document from the others in the room. In a project
    /// session `paused` answers a held large change (`"share"` or
    /// `"discard"`); without it, a guest still joining asks for the manifest
    /// again.
    fn resync(&self, args: &Value) -> Result<Value, LiveHostError> {
        match self.running(None)? {
            Running::File(handle) => handle.resync(random_peer()?)?,
            Running::Project(handle) => {
                let paused = match args.get("paused").and_then(Value::as_str) {
                    None => None,
                    Some("share") => Some(true),
                    Some("discard") => Some(false),
                    Some(other) => {
                        return Err(LiveHostError::Invalid(format!(
                            "`paused` must be \"share\" or \"discard\", not {other:?}"
                        )));
                    }
                };
                handle.resync(paused)?;
            }
        }
        Ok(json!({ "resyncing": true }))
    }

    /// Who edited since the last call, with ready-to-append commit trailers.
    fn contributors(&self, args: &Value) -> Result<Value, LiveHostError> {
        let list = match self.running(None)? {
            Running::File(handle) => handle.contributors()?,
            Running::Project(handle) => handle.contributors()?,
        };
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

    /// The running session (of either kind), if `session` names it.
    fn running(&self, session: Option<&str>) -> Result<Running, LiveHostError> {
        let guard = self.active.lock();
        let active = guard.as_ref().ok_or(LiveHostError::NotLive)?;
        if let Some(wanted) = session
            && wanted != active.session
        {
            return Err(LiveHostError::NotLive);
        }
        Ok(active.running.clone())
    }

    /// The project session, for the commands that only mean one.
    fn project(&self) -> Result<Arc<ProjectHandle>, LiveHostError> {
        match self.running(None)? {
            Running::Project(handle) => Ok(handle),
            Running::File(_) => Err(LiveHostError::NotProject),
        }
    }

    fn status(&self) -> Value {
        let relay_set = !self.settings.lock().relay_url.trim().is_empty();
        let guard = self.active.lock();
        let Some(active) = guard.as_ref() else {
            return json!({ "active": false, "relay_set": relay_set });
        };
        let mut out = json!({
            "active": true,
            "relay_set": relay_set,
            "session": active.session,
            "role": active.role,
            "file": active.file,
            "relay": active.relay,
        });
        let Some(object) = out.as_object_mut() else {
            return out;
        };
        match &active.running {
            Running::File(handle) => {
                let status = handle.status();
                object.insert("scope".into(), json!("file"));
                object.insert("doc".into(), json!(active.doc));
                object.insert("state".into(), json!(status.state));
                object.insert("attempt".into(), json!(status.attempt));
                object.insert("error".into(), json!(status.error));
                object.insert("version".into(), json!(status.version));
                object.insert("rejected".into(), json!(status.rejected));
                object.insert(
                    "peers".into(),
                    json!(
                        status
                            .peers
                            .iter()
                            .map(|p| json!({
                                "peer": p.peer.to_string(), "name": p.name, "slot": p.slot,
                            }))
                            .collect::<Vec<_>>()
                    ),
                );
            }
            Running::Project(handle) => {
                let status = handle.status();
                object.insert("scope".into(), json!("project"));
                object.insert("state".into(), json!(status.state));
                object.insert("attempt".into(), json!(status.attempt));
                object.insert("error".into(), json!(status.error));
                object.insert("version".into(), json!(0));
                object.insert("rejected".into(), json!(status.rejected));
                object.insert("phase".into(), json!(status.phase));
                object.insert("files".into(), json!(status.files));
                object.insert(
                    "progress".into(),
                    status.progress.map_or(
                        Value::Null,
                        |(done, total)| json!({ "done": done, "total": total }),
                    ),
                );
                // Files no other present peer has shown that it holds: what
                // a host loses by leaving now (a warning, never a block).
                object.insert("unsynced_files".into(), json!(status.unsynced_files));
                object.insert(
                    "paused".into(),
                    status.paused.clone().unwrap_or(Value::Null),
                );
                object.insert(
                    "skipped".into(),
                    json!(
                        status
                            .skipped
                            .iter()
                            .map(|(path, reason)| json!({
                                "path": path, "reason": reason,
                            }))
                            .collect::<Vec<_>>()
                    ),
                );
                object.insert("following".into(), json!(active.following));
                object.insert(
                    "peers".into(),
                    json!(
                        status
                            .peers
                            .iter()
                            .map(crate::projdrv::PeerAt::to_json)
                            .collect::<Vec<_>>()
                    ),
                );
            }
        }
        out
    }

    fn leave(&self) -> Value {
        let active = self.active.lock().take();
        match active {
            Some(active) => {
                match &active.running {
                    Running::File(handle) => handle.leave(),
                    Running::Project(handle) => handle.leave(),
                }
                json!({ "left": true })
            }
            None => json!({ "left": false }),
        }
    }

    fn push(&self, args: Value) -> Result<Value, LiveHostError> {
        let a: PushArgs = parse(args)?;
        let running = self.running(a.session.as_deref())?;
        let changes = a
            .changes
            .into_iter()
            .map(|c| Change {
                start: c.start,
                delete: c.delete,
                insert: c.insert,
            })
            .collect();
        let outcome = match running {
            Running::File(handle) => handle.push(a.version, changes)?,
            Running::Project(handle) => {
                let path = a.path.ok_or_else(path_required)?;
                handle.push(&path, a.version, changes)?
            }
        };
        Ok(match outcome {
            PushOutcome::Accepted(version) => json!({ "accepted": true, "version": version }),
            PushOutcome::Stale(version) => json!({ "accepted": false, "version": version }),
        })
    }

    fn selection(&self, args: &Value) -> Result<Value, LiveHostError> {
        let running = self.running(args.get("session").and_then(Value::as_str))?;
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
        match running {
            Running::File(handle) => handle.selection(version, selection),
            Running::Project(handle) => {
                let path = str_arg(args, "path")?;
                handle.selection(&path, version, selection);
            }
        }
        Ok(json!({}))
    }

    fn snapshot(&self, args: &Value) -> Result<Value, LiveHostError> {
        let (version, text) = match self.running(args.get("session").and_then(Value::as_str))? {
            Running::File(handle) => handle.snapshot()?,
            Running::Project(handle) => handle.snapshot(&str_arg(args, "path")?)?,
        };
        Ok(json!({ "version": version, "text": text }))
    }

    /// `live.resolve {choices}`: phase 2 of a project join.
    fn resolve(&self, args: &Value) -> Result<Value, LiveHostError> {
        let handle = self.project()?;
        let list = args
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| LiveHostError::Invalid("`choices` must be a list".to_owned()))?;
        let mut choices = Vec::with_capacity(list.len());
        for item in list {
            let path = str_arg(item, "path")?;
            let action = item
                .get("action")
                .and_then(Value::as_str)
                .and_then(Action::parse)
                .ok_or_else(|| LiveHostError::Invalid(format!("`{path}` has an unknown action")))?;
            choices.push(Choice { path, action });
        }
        let outcome = handle.resolve(choices)?;
        Ok(json!({ "applied": outcome.applied, "trashed": outcome.trashed }))
    }

    fn files(&self) -> Result<Value, LiveHostError> {
        let files = self.project()?.files()?;
        Ok(json!({
            "files": files.iter().map(|f| json!({
                "path": f.path,
                "kind": if f.binary { "binary" } else { "text" },
                "peers": f.peers.iter().map(u64::to_string).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }))
    }

    fn follow(&self, args: &Value) -> Result<Value, LiveHostError> {
        let peer = args.get("peer").and_then(Value::as_str).map(str::to_owned);
        let mut guard = self.active.lock();
        let active = guard.as_mut().ok_or(LiveHostError::NotLive)?;
        if !matches!(active.running, Running::Project(_)) {
            return Err(LiveHostError::NotProject);
        }
        active.following.clone_from(&peer);
        Ok(json!({ "following": peer }))
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
                json!({ "relay": link.relay, "file": link.file, "scope": link.scope.as_str() })
            }
            "live.push" => inner.push(args)?,
            "live.selection" => inner.selection(&args)?,
            "live.snapshot" => inner.snapshot(&args)?,
            "live.contributors" => inner.contributors(&args)?,
            "live.resync" => inner.resync(&args)?,
            "live.resolve" => inner.resolve(&args)?,
            "live.attach" => {
                let path = str_arg(&args, "path")?;
                let doc = args.get("doc").and_then(Value::as_u64);
                let (version, text) = inner.project()?.attach(&path, doc)?;
                json!({ "version": version, "text": text })
            }
            "live.detach" => {
                let path = str_arg(&args, "path")?;
                json!({ "detached": inner.project()?.detach(&path)? })
            }
            "live.follow" => inner.follow(&args)?,
            "live.files" => inner.files()?,
            "live.exclude" => {
                let path = str_arg(&args, "path")?;
                json!({ "excluded": inner.project()?.exclude(&path)? })
            }
            "live.include" => {
                let path = str_arg(&args, "path")?;
                json!({ "included": inner.project()?.include(&path)? })
            }
            "live.restore" => {
                let path = str_arg(&args, "path")?;
                json!({ "restored": inner.project()?.restore(&path)? })
            }
            "live.configure" => inner.configure(&args)?,
            other => return Err(PluginError::UnknownCommand(other.to_owned())),
        };
        Ok(Payload::Json(out))
    }
}
