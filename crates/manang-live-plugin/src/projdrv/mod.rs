//! The project session thread.
//!
//! A project session shares a whole folder: one project tree plus one text
//! document per file, loaded lazily (see `manang_live::session`). It has its
//! own thread like a single-file session (`crate::driver`), which stays
//! untouched; this one owns the [`Session`] in project mode, the per-file
//! state, the [`DiskBridge`] (the only writer of files that are not open in a
//! tab), the [`FsBridge`] (disk changes become tree operations and text edits)
//! and the sidecar store (documents evicted from memory). Nothing else
//! touches them, so no lock is held around CRDT work and nothing CPU-bound
//! runs on a Tokio worker.
//!
//! # Who writes which file
//!
//! * A file **open in a tab** (`live.attach`) is shown by the web view, which
//!   keeps the file on disk (its autosave is the single writer). The thread
//!   keeps a rope for it, applies remote edits to the rope and tells the view
//!   (`live.remote` with the file's `path`, the same version protocol as a
//!   single-file session, one counter per file).
//! * Every other file is written by this thread through the [`DiskBridge`]
//!   (debounced, atomic, echo suppressed, replaced or deleted copies go to
//!   `.manang/trash`). A remote path is validated by the policy before any
//!   write: an invite holder writes on other people's disks.
//!
//! # Join
//!
//! A guest connects, fetches the manifest and classifies its folder against it
//! (`live.plan`). Nothing on disk changes until `live.resolve`. Differing
//! files are replaced with the host's version only when the guest chose so,
//! and the guest's copy goes to the trash first; a join that replaces more
//! than one file needs the single all-files confirmation, enforced here too.
//! A file the guest keeps stays out of the session: it is never offered to
//! the room and never overwritten.
//!
//! # Fields the shell can read
//!
//! `live.status` and the `live.peers` event carry `unsynced_files`: how many
//! files of the project no other present peer has shown that it holds. A host
//! about to leave can warn "N files not yet synced to others"; leaving is
//! never blocked.

mod files;
mod join;
mod local;
mod remote;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use manang_editor::{DocId, Editor};
use manang_kernel::{EventBus, Payload, Subscription};
use manang_live::LoroEngine;
use manang_live::blob::BlobHash;
use manang_live::envelope::{OsEntropy, RoomKeys};
use manang_live::project::{FileId, ProjectDoc, ProjectView};
use manang_live::session::{Input, Output, Session, SessionConfig};
use manang_live::transport::{SyncTransport, TransportEvent};
use manang_live::vcs::Contributor;
use manang_relay::RelayClient;
use parking_lot::Mutex;
use ropey::Rope;
use serde_json::{Value, json};

use crate::disk::{Clock, DiskBridge, SystemClock};
use crate::driver::{Change, PushOutcome, state_label};
use crate::error::LiveHostError;
use crate::fsbridge::FsBridge;
use crate::pathsec::PathPolicy;
use crate::sidecar::{Residency, SidecarStore};

pub(crate) use join::{Action, Choice, ResolveOutcome};

/// How often the thread wakes with nothing to do.
const TICK: Duration = Duration::from_millis(10);
/// How long a command waits for the thread.
const REPLY_TIMEOUT: Duration = Duration::from_secs(12);
/// How long an attach waits for a file that is still being fetched.
const ATTACH_WAIT_MS: u64 = 8_000;
/// Quiet time after a local edit before the node's size and hash are updated.
const META_QUIET_MS: u64 = 1_500;
/// Minimum gap between presence events.
const PRESENCE_GAP_MS: u64 = 40;
/// Gap between retries of deferred evictions and the statistics refresh.
const SLOW_GAP_MS: u64 = 1_000;
/// Most skipped paths remembered for the status.
const MAX_SKIPPED: usize = 32;

/// Which side of the room this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Host,
    Guest,
}

/// A remote participant and the file in front of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PeerAt {
    pub peer: u64,
    pub name: String,
    pub slot: u8,
    pub path: Option<String>,
}

impl PeerAt {
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "peer": self.peer.to_string(),
            "name": self.name,
            "slot": self.slot,
            "path": self.path,
        })
    }
}

/// What the shell shows about a project session.
#[derive(Debug, Clone)]
pub(crate) struct ProjectStatus {
    pub state: &'static str,
    pub attempt: u32,
    pub error: Option<String>,
    pub rejected: u64,
    pub peers: Vec<PeerAt>,
    /// `manifest` (a guest waiting for the host), `plan` (waiting for
    /// `live.resolve`) or `live`.
    pub phase: &'static str,
    /// Files (not directories) of the session, not counting excluded ones.
    pub files: usize,
    pub progress: Option<(usize, usize)>,
    /// Files no other present peer has shown that it holds.
    pub unsynced_files: usize,
    /// A large change on disk waits for an answer (`live.paused`).
    pub paused: Option<Value>,
    /// Paths left out or refused, most recent last.
    pub skipped: Vec<(String, String)>,
}

impl ProjectStatus {
    fn new(phase: &'static str) -> Self {
        Self {
            state: "connecting",
            attempt: 0,
            error: None,
            rejected: 0,
            peers: Vec::new(),
            phase,
            files: 0,
            progress: None,
            unsynced_files: 0,
            paused: None,
            skipped: Vec::new(),
        }
    }
}

/// A file of the session for `live.files`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileNode {
    pub path: String,
    pub binary: bool,
    pub peers: Vec<u64>,
}

type Reply<T> = Sender<Result<T, LiveHostError>>;

enum Cmd {
    Push {
        path: String,
        version: u64,
        changes: Vec<Change>,
        reply: Reply<PushOutcome>,
    },
    Selection {
        path: String,
        version: u64,
        selection: Option<(usize, usize)>,
    },
    Snapshot {
        path: String,
        reply: Reply<(u64, String)>,
    },
    Attach {
        path: String,
        doc: Option<DocId>,
        reply: Reply<(u64, String)>,
    },
    Detach {
        path: String,
        reply: Reply<bool>,
    },
    Resolve {
        choices: Vec<Choice>,
        reply: Reply<ResolveOutcome>,
    },
    Files {
        reply: Reply<Vec<FileNode>>,
    },
    Exclude {
        path: String,
        reply: Reply<bool>,
    },
    Include {
        path: String,
        reply: Reply<bool>,
    },
    Restore {
        path: String,
        reply: Reply<bool>,
    },
    Contributors {
        reply: Sender<Vec<Contributor>>,
    },
    Resync {
        paused: Option<bool>,
        reply: Reply<()>,
    },
    Leave {
        reply: Sender<()>,
    },
}

/// Everything a project session needs to start.
pub(crate) struct ProjectParams {
    pub session_id: String,
    pub role: Role,
    pub policy: PathPolicy,
    pub editor: Arc<Editor>,
    pub events: EventBus,
    pub fs_events: Subscription,
    pub handle: tokio::runtime::Handle,
    pub room_url: String,
    pub room_hex: String,
    pub keys: RoomKeys,
    pub config: SessionConfig,
    /// The host's tree built from the folder; a guest's is empty.
    pub project: ProjectDoc,
    pub sidecar: SidecarStore,
    /// Most documents held in memory at once.
    pub loaded_docs: usize,
}

/// The plugin's handle on a running project session thread.
pub(crate) struct ProjectHandle {
    tx: Sender<Cmd>,
    status: Arc<Mutex<ProjectStatus>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl ProjectHandle {
    /// Start the thread and connect.
    pub(crate) fn spawn(params: ProjectParams) -> Result<Self, LiveHostError> {
        let (tx, rx) = mpsc::channel();
        let phase = if params.role == Role::Guest {
            "manifest"
        } else {
            "live"
        };
        let status = Arc::new(Mutex::new(ProjectStatus::new(phase)));
        let mut transport = RelayClient::new(params.room_url, params.handle);
        let mut session =
            Session::new_project(params.project, params.keys, OsEntropy, params.config)?;
        // The host's tree was built before the session existed: nothing of it
        // is news to the room, peers read it from the exchange on connect.
        let _ = session.flush_project();
        let mut first = Vec::new();
        for output in session.start() {
            match output {
                Output::Connect => transport.connect()?,
                other => first.push(other),
            }
        }
        let clock = SystemClock::new();
        let peer = session.peer();
        let mut driver = Driver {
            sid: params.session_id,
            role: params.role,
            peer,
            room_hex: params.room_hex,
            session,
            transport,
            rx,
            status: Arc::clone(&status),
            events: params.events,
            editor: params.editor,
            clock,
            now: 0,
            disk: DiskBridge::new(params.policy, clock),
            fs: FsBridge::new(clock),
            fs_events: params.fs_events,
            sidecar: params.sidecar,
            residency: Residency::new(params.loaded_docs.max(1)),
            files: BTreeMap::new(),
            focus: None,
            ropes: BTreeMap::new(),
            phase: if params.role == Role::Guest {
                Phase::AwaitingManifest
            } else {
                Phase::Live
            },
            excluded_paths: BTreeSet::new(),
            dirty_docs: BTreeSet::new(),
            arriving: BTreeSet::new(),
            arriving_blobs: BTreeMap::new(),
            meta_due: BTreeMap::new(),
            pending_text: BTreeMap::new(),
            pending_attach: Vec::new(),
            deferred_evict: BTreeSet::new(),
            deleted_trash: BTreeMap::new(),
            blob_served: BTreeMap::new(),
            known_peers: BTreeMap::new(),
            last_peers: Vec::new(),
            last_unsynced: None,
            peers_dirty: true,
            presence_dirty: true,
            last_presence_ms: 0,
            last_presence: BTreeMap::new(),
            last_slow_ms: 0,
            closed: false,
            hard_error: false,
        };
        for output in first {
            driver.absorb_into(output);
        }
        let thread = std::thread::Builder::new()
            .name("manang-live-project".to_owned())
            .spawn(move || driver.run())
            .map_err(|e| LiveHostError::Io(e.to_string()))?;
        Ok(Self {
            tx,
            status,
            thread: Mutex::new(Some(thread)),
        })
    }

    fn ask<T>(&self, make: impl FnOnce(Sender<T>) -> Cmd) -> Result<T, LiveHostError> {
        let (reply, answer) = mpsc::channel();
        self.tx
            .send(make(reply))
            .map_err(|_| LiveHostError::Stopped)?;
        answer
            .recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| LiveHostError::Stopped)
    }

    /// Offer local edits to the file at `path`, made against `version`.
    pub(crate) fn push(
        &self,
        path: &str,
        version: u64,
        changes: Vec<Change>,
    ) -> Result<PushOutcome, LiveHostError> {
        self.ask(|reply| Cmd::Push {
            path: path.to_owned(),
            version,
            changes,
            reply,
        })?
    }

    /// Announce the local selection in `path`.
    pub(crate) fn selection(&self, path: &str, version: u64, selection: Option<(usize, usize)>) {
        let _ = self.tx.send(Cmd::Selection {
            path: path.to_owned(),
            version,
            selection,
        });
    }

    pub(crate) fn snapshot(&self, path: &str) -> Result<(u64, String), LiveHostError> {
        self.ask(|reply| Cmd::Snapshot {
            path: path.to_owned(),
            reply,
        })?
    }

    pub(crate) fn attach(
        &self,
        path: &str,
        doc: Option<DocId>,
    ) -> Result<(u64, String), LiveHostError> {
        self.ask(|reply| Cmd::Attach {
            path: path.to_owned(),
            doc,
            reply,
        })?
    }

    pub(crate) fn detach(&self, path: &str) -> Result<bool, LiveHostError> {
        self.ask(|reply| Cmd::Detach {
            path: path.to_owned(),
            reply,
        })?
    }

    pub(crate) fn resolve(&self, choices: Vec<Choice>) -> Result<ResolveOutcome, LiveHostError> {
        self.ask(|reply| Cmd::Resolve { choices, reply })?
    }

    pub(crate) fn files(&self) -> Result<Vec<FileNode>, LiveHostError> {
        self.ask(|reply| Cmd::Files { reply })?
    }

    pub(crate) fn exclude(&self, path: &str) -> Result<bool, LiveHostError> {
        self.ask(|reply| Cmd::Exclude {
            path: path.to_owned(),
            reply,
        })?
    }

    pub(crate) fn include(&self, path: &str) -> Result<bool, LiveHostError> {
        self.ask(|reply| Cmd::Include {
            path: path.to_owned(),
            reply,
        })?
    }

    pub(crate) fn restore(&self, path: &str) -> Result<bool, LiveHostError> {
        self.ask(|reply| Cmd::Restore {
            path: path.to_owned(),
            reply,
        })?
    }

    pub(crate) fn contributors(&self) -> Result<Vec<Contributor>, LiveHostError> {
        self.ask(|reply| Cmd::Contributors { reply })
    }

    /// Answer a held large change (`Some(true)` shares it, `Some(false)`
    /// discards it and rewrites the disk from the session) or, with `None`,
    /// ask the room for everything again.
    pub(crate) fn resync(&self, paused: Option<bool>) -> Result<(), LiveHostError> {
        self.ask(|reply| Cmd::Resync { paused, reply })?
    }

    #[must_use]
    pub(crate) fn status(&self) -> ProjectStatus {
        self.status.lock().clone()
    }

    /// Leave the room and stop the thread. Idempotent.
    pub(crate) fn leave(&self) {
        let _ = self.ask(|reply| Cmd::Leave { reply });
        if let Some(thread) = self.thread.lock().take() {
            let _ = thread.join();
        }
    }
}

/// Where a guest is in joining.
enum Phase {
    /// Connected, no manifest yet; nothing is on disk or in memory.
    AwaitingManifest,
    /// The plan is out; `live.resolve` decides. Nothing on disk changed.
    AwaitingResolve(Box<join::PendingPlan>),
    /// In the session.
    Live,
}

/// Per-file state of the thread.
#[derive(Default)]
struct FileState {
    /// Version counter of the protocol with the web view (one per file).
    version: u64,
    /// The kernel editor document that mirrors the file while it is open.
    editor_doc: Option<DocId>,
    attached: bool,
    /// The document came from this machine (a sidecar or the disk), so it is
    /// usable before any peer answers.
    local_origin: bool,
    /// SHA-256 of the text this side last made equal to the disk (the
    /// sidecar's expectation); set when the document is evicted.
    synced_sha: Option<[u8; 32]>,
    /// The first arrival replaces the guest's differing copy (trash first).
    replace_on_arrival: bool,
    /// Kept out of the session on this side.
    excluded: bool,
    /// The path the tab shows (kept while the file is open, even after a
    /// remote delete removed it from the tree).
    open_path: Option<String>,
    /// The path the manifest or the last move gave the file: the fallback
    /// while the project tree has not reached a guest yet.
    known_path: Option<String>,
    /// The join (or a creation) has already taken this file.
    joined: bool,
}

struct PendingAttach {
    id: FileId,
    path: String,
    doc: Option<DocId>,
    deadline_ms: u64,
    reply: Reply<(u64, String)>,
}

struct Driver {
    sid: String,
    role: Role,
    peer: u64,
    room_hex: String,
    session: Session<LoroEngine, OsEntropy>,
    transport: RelayClient,
    rx: Receiver<Cmd>,
    status: Arc<Mutex<ProjectStatus>>,
    events: EventBus,
    editor: Arc<Editor>,
    clock: SystemClock,
    now: u64,
    disk: DiskBridge<SystemClock>,
    fs: FsBridge<SystemClock>,
    fs_events: Subscription,
    sidecar: SidecarStore,
    residency: Residency<FileId>,
    files: BTreeMap<FileId, FileState>,
    /// The file this side is editing (its caret is announced).
    focus: Option<FileId>,
    /// Editor buffers of the files open in a tab: remote edits land here.
    ropes: BTreeMap<FileId, Rope>,
    phase: Phase,
    /// Paths kept out of the session on this side (kept copies and private
    /// guest-only files), so a remote change never touches them.
    excluded_paths: BTreeSet<String>,
    /// Documents changed by a remote update, to be written to disk.
    dirty_docs: BTreeSet<FileId>,
    /// Text files whose first content is still on its way.
    arriving: BTreeSet<FileId>,
    /// Blobs being fetched: the files waiting for each.
    arriving_blobs: BTreeMap<BlobHash, BTreeSet<FileId>>,
    /// Local edits whose node metadata is due at this time.
    meta_due: BTreeMap<FileId, u64>,
    /// New disk text of a file whose document is still being fetched.
    pending_text: BTreeMap<FileId, String>,
    pending_attach: Vec<PendingAttach>,
    /// Evictions put off until the disk caught up.
    deferred_evict: BTreeSet<FileId>,
    /// Where a remote delete put the file (path -> trash path).
    deleted_trash: BTreeMap<String, String>,
    /// Peers each blob was served to (blobs have no document to confirm them).
    blob_served: BTreeMap<BlobHash, BTreeSet<u64>>,
    known_peers: BTreeMap<u64, String>,
    last_peers: Vec<PeerAt>,
    last_unsynced: Option<usize>,
    peers_dirty: bool,
    presence_dirty: bool,
    last_presence_ms: u64,
    last_presence: BTreeMap<FileId, Value>,
    last_slow_ms: u64,
    closed: bool,
    hard_error: bool,
}

impl Driver {
    fn run(mut self) {
        while !self.closed {
            match self.rx.recv_timeout(TICK) {
                Ok(cmd) => {
                    self.now = self.clock.now_ms();
                    self.command(cmd);
                    while !self.closed
                        && let Ok(more) = self.rx.try_recv()
                    {
                        self.command(more);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.now = self.clock.now_ms();
                    self.leave();
                    break;
                }
            }
            if self.closed {
                break;
            }
            self.now = self.clock.now_ms();
            self.pump();
            self.fs_poll();
            self.housekeeping();
        }
    }

    fn command(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Push {
                path,
                version,
                changes,
                reply,
            } => {
                let _ = reply.send(self.push(&path, version, &changes));
            }
            Cmd::Selection {
                path,
                version,
                selection,
            } => self.selection(&path, version, selection),
            Cmd::Snapshot { path, reply } => {
                let _ = reply.send(self.snapshot(&path));
            }
            Cmd::Attach { path, doc, reply } => self.attach(path, doc, reply),
            Cmd::Detach { path, reply } => {
                let _ = reply.send(self.detach(&path));
            }
            Cmd::Resolve { choices, reply } => {
                let _ = reply.send(self.resolve(&choices));
            }
            Cmd::Files { reply } => {
                let _ = reply.send(Ok(self.file_list()));
            }
            Cmd::Exclude { path, reply } => {
                let _ = reply.send(self.exclude(&path));
            }
            Cmd::Include { path, reply } => {
                let _ = reply.send(self.include(&path));
            }
            Cmd::Restore { path, reply } => {
                let _ = reply.send(self.restore(&path));
            }
            Cmd::Contributors { reply } => {
                let _ = reply.send(self.session.take_contributors());
            }
            Cmd::Resync { paused, reply } => {
                let _ = reply.send(self.resync(paused));
            }
            Cmd::Leave { reply } => {
                self.leave();
                let _ = reply.send(());
            }
        }
    }

    /// Move what the transport delivered into the session.
    fn pump(&mut self) {
        let mut events: Vec<TransportEvent> = Vec::new();
        while let Some(event) = self.transport.poll() {
            events.push(event);
        }
        for event in events {
            let now = self.now;
            let outputs = match &event {
                TransportEvent::Up => self.session.handle_with(now, Input::Up, &mut self.ropes),
                TransportEvent::Down => self.session.handle_with(now, Input::Down, &mut self.ropes),
                TransportEvent::Frame(frame) => {
                    self.session
                        .handle_with(now, Input::Frame(frame), &mut self.ropes)
                }
            };
            self.perform(outputs);
        }
        let outputs = self
            .session
            .handle_with(self.now, Input::Tick, &mut self.ropes);
        self.perform(outputs);
    }

    /// Execute transport outputs and handle the rest, including whatever the
    /// handling produces in turn.
    fn perform(&mut self, outputs: Vec<Output>) {
        let mut queue: VecDeque<Output> = outputs.into();
        while let Some(output) = queue.pop_front() {
            match output {
                Output::Send(frame) => {
                    if let Err(e) = self.transport.send(frame) {
                        self.fail(&e.to_string());
                    }
                }
                Output::Connect => {
                    if let Err(e) = self.transport.connect() {
                        self.fail(&e.to_string());
                    }
                }
                Output::Close => self.transport.close(),
                other => queue.extend(self.absorb(other)),
            }
        }
    }

    /// For the outputs of the first `start`: transport work is done by the
    /// caller, the rest is handled here.
    fn absorb_into(&mut self, output: Output) {
        let more = self.absorb(output);
        self.perform(more);
    }

    fn housekeeping(&mut self) {
        let now = self.now;
        let report = self.disk.tick();
        for (path, error) in report.failed {
            self.skip(&path, &error.to_string());
        }
        self.flush_dirty_docs();
        self.settle_arrivals();
        self.apply_pending_text();
        self.update_metas();
        self.finish_attaches();
        let fs_out = self.fs_tick();
        self.perform(fs_out);
        self.emit_presence();
        if self.peers_dirty {
            self.peers_changed();
        }
        if now.saturating_sub(self.last_slow_ms) >= SLOW_GAP_MS {
            self.last_slow_ms = now;
            self.retry_evictions();
            self.refresh_stats();
        }
    }

    fn leave(&mut self) {
        if self.closed {
            return;
        }
        // Whatever the room sent is on disk before we go; the files stay.
        let report = self.disk.flush_all();
        for (path, error) in report.failed {
            self.skip(&path, &error.to_string());
        }
        self.flush_dirty_docs();
        let _ = self.disk.flush_all();
        for pending in std::mem::take(&mut self.pending_attach) {
            let _ = pending.reply.send(Err(LiveHostError::NotLive));
        }
        let outputs = self
            .session
            .handle_with(self.now, Input::Leave, &mut self.ropes);
        self.perform(outputs);
        self.closed = true;
        {
            let mut status = self.status.lock();
            status.state = "left";
        }
        self.emit_state();
    }

    // -- small helpers ---------------------------------------------------------

    fn view(&mut self) -> Arc<ProjectView> {
        self.session
            .project_view()
            .unwrap_or_else(|| Arc::new(ProjectView::default()))
    }

    fn path_of(&mut self, id: FileId) -> Option<String> {
        self.view().path_of(id).map(str::to_owned)
    }

    fn id_of(&mut self, path: &str) -> Result<FileId, LiveHostError> {
        self.view()
            .id_at_path(path)
            .ok_or_else(|| LiveHostError::NotInSession(path.to_owned()))
    }

    fn file(&mut self, id: FileId) -> &mut FileState {
        self.files.entry(id).or_default()
    }

    fn emit(&self, topic: &str, mut payload: Value) {
        if let Some(object) = payload.as_object_mut() {
            object.insert("session".to_owned(), json!(self.sid));
        }
        self.events.emit(topic, Payload::Json(payload));
    }

    /// Remember a path that was left out or refused, with the reason.
    fn skip(&mut self, path: &str, reason: &str) {
        let mut status = self.status.lock();
        if status.skipped.len() >= MAX_SKIPPED {
            status.skipped.remove(0);
        }
        status.skipped.push((path.to_owned(), reason.to_owned()));
    }

    /// A problem that may pass: the next state change clears it.
    fn fail(&mut self, message: &str) {
        if self.hard_error {
            return;
        }
        {
            let mut status = self.status.lock();
            status.state = "error";
            status.error = Some(message.to_owned());
        }
        self.emit_state();
    }

    fn emit_state(&self) {
        let status = self.status.lock().clone();
        self.emit(
            "live.state",
            json!({
                "state": status.state,
                "attempt": status.attempt,
                "error": status.error,
                "peers": status.peers.iter().map(PeerAt::to_json).collect::<Vec<_>>(),
            }),
        );
    }

    /// The session's state changed.
    fn on_state(&mut self, state: manang_live::session::SessionState) {
        let (label, attempt) = state_label(state);
        {
            let mut status = self.status.lock();
            if !self.hard_error {
                status.state = label;
                status.error = None;
            }
            status.attempt = attempt;
        }
        self.peers_dirty = true;
        self.emit_state();
    }

    /// Recount files and the files nobody else holds.
    fn refresh_stats(&mut self) {
        let view = self.view();
        let present: BTreeSet<u64> = self
            .session
            .peers()
            .into_iter()
            .map(|(p, _, _)| p)
            .collect();
        let mut files = 0usize;
        let mut unsynced = 0usize;
        for entry in view.entries() {
            if entry.meta.kind == manang_live::project::NodeKind::Dir {
                continue;
            }
            if self.files.get(&entry.id).is_some_and(|f| f.excluded)
                || self.excluded_paths.contains(&entry.path)
            {
                continue;
            }
            files += 1;
            let held = match entry.meta.kind {
                manang_live::project::NodeKind::Text => self
                    .session
                    .file_holders(entry.id)
                    .iter()
                    .any(|p| present.contains(p)),
                _ => self
                    .blob_served
                    .get(&entry.meta.sha256)
                    .is_some_and(|set| set.iter().any(|p| present.contains(p))),
            };
            if !held {
                unsynced += 1;
            }
        }
        let changed = {
            let mut status = self.status.lock();
            let changed = status.unsynced_files != unsynced;
            status.files = files;
            status.unsynced_files = unsynced;
            changed
        };
        if changed {
            self.peers_dirty = true;
        }
    }
}
