//! The session thread.
//!
//! One live session owns one OS thread. The thread owns the CRDT session, the
//! rope mirror of the document and the relay transport, and nothing else
//! touches them, so no lock is held around CRDT work and nothing CPU-bound
//! runs on a Tokio worker (the runtime only moves bytes). The plugin's
//! commands reach it through a channel and wait for a reply.
//!
//! # The edit protocol with the web view
//!
//! The document the web view shows and the one this thread holds can differ
//! for a few milliseconds: a remote edit may be on its way to the view while
//! the person types. The two sides agree on order with a version counter (the
//! scheme of CodeMirror's collab module, with this thread as the authority):
//!
//! * `version` counts accepted steps: every applied remote batch and every
//!   accepted local batch is one step.
//! * The view pushes local edits with the version it has seen. If that is the
//!   current version the edits are applied and acknowledged (`live.remote`
//!   with `ack: true`); otherwise they are refused (`Stale`) and the view
//!   catches up on the events it has not processed yet, rebases its pending
//!   edits over them and pushes again. This thread never transforms anything.
//! * Remote batches and acknowledgements travel on one ordered event stream,
//!   so the view sees every step exactly once and in order.
//!
//! The shared copy in the editor plugin (the kernel document) is updated here,
//! step by step, so compiling, search and the agent read the live text.

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use manang_editor::{DocId, Editor};
use manang_kernel::{EventBus, Payload};
use manang_live::LoroEngine;
use manang_live::TextCrdt;
use manang_live::bridge::{LiveDoc, RemoteApplied, TextTarget};
use manang_live::envelope::{OsEntropy, RoomKeys};
use manang_live::session::{Input, Output, Session, SessionConfig, SessionState, pump};
use manang_live::transport::SyncTransport;
use manang_live::vcs::Contributor;
use manang_relay::RelayClient;
use parking_lot::Mutex;
use ropey::Rope;
use serde_json::{Value, json};

use crate::error::LiveHostError;
use crate::store::{Snapshots, Trash};

/// How often the thread wakes with nothing to do (polls the socket, ticks timers).
const TICK: Duration = Duration::from_millis(10);
/// Minimum gap between presence events.
const PRESENCE_GAP_MS: u64 = 40;
/// Minimum gap between writes of the document snapshot.
const SNAPSHOT_GAP_MS: u64 = 2_000;
/// Two automatic resyncs are never closer than this.
const RESYNC_GAP_MS: u64 = 10_000;
/// How long a command waits for the thread.
const REPLY_TIMEOUT: Duration = Duration::from_secs(10);

/// One replacement in UTF-16 units, sequential: each change addresses the
/// document as it stands after the earlier ones in the same batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub start: usize,
    pub delete: usize,
    pub insert: String,
}

impl Change {
    fn to_json(&self) -> Value {
        json!({ "start": self.start, "delete": self.delete, "insert": self.insert })
    }
}

/// A remote participant, for the session bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    pub peer: u64,
    pub name: String,
    pub slot: u8,
}

impl PeerInfo {
    fn to_json(&self) -> Value {
        json!({ "peer": self.peer.to_string(), "name": self.name, "slot": self.slot })
    }
}

/// What the shell shows about the session.
#[derive(Debug, Clone)]
pub struct Status {
    /// `connecting`, `waiting`, `connected`, `reconnecting`, `left` or `error`.
    pub state: &'static str,
    /// Failed reconnect attempts so far (while `reconnecting`).
    pub attempt: u32,
    pub peers: Vec<PeerInfo>,
    pub version: u64,
    pub error: Option<String>,
    /// Frames dropped as unreadable (wrong key, tampered, malformed).
    pub rejected: u64,
}

impl Status {
    fn new() -> Self {
        Self {
            state: "connecting",
            attempt: 0,
            peers: Vec::new(),
            version: 0,
            error: None,
            rejected: 0,
        }
    }
}

/// The answer to a push.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    /// Applied; this is the new version (also announced as an ack event).
    Accepted(u64),
    /// Refused: the view is behind. Catch up to this version, then push again.
    Stale(u64),
}

enum Cmd {
    Push {
        version: u64,
        changes: Vec<Change>,
        reply: Sender<Result<PushOutcome, LiveHostError>>,
    },
    Selection {
        version: u64,
        selection: Option<(usize, usize)>,
    },
    Snapshot {
        reply: Sender<(u64, String)>,
    },
    Contributors {
        reply: Sender<Vec<Contributor>>,
    },
    Resync {
        peer: u64,
        reply: Sender<Result<(), LiveHostError>>,
    },
    Leave {
        reply: Sender<()>,
    },
}

/// Everything a session needs to start.
pub struct DriverParams {
    pub session_id: String,
    pub doc: DocId,
    pub editor: Arc<Editor>,
    pub events: EventBus,
    pub handle: tokio::runtime::Handle,
    pub room_url: String,
    pub live_doc: LiveDoc<LoroEngine>,
    /// The text the CRDT and the shared copy both hold at the start.
    pub rope: Rope,
    pub keys: RoomKeys,
    pub config: SessionConfig,
    /// The file's text before this join, when the join will replace it with
    /// the room's: it is replaced only once somebody answers.
    pub replace: Option<String>,
    /// Where a copy about to be replaced is saved first.
    pub trash: Option<Trash>,
    /// Where the document snapshot is kept, and the room's id for its name.
    pub snapshots: Option<(Snapshots, String)>,
}

/// The snapshot sidecar of this session.
struct SnapshotSink {
    store: Snapshots,
    room: String,
}

/// The plugin's handle on a running session thread.
pub struct DriverHandle {
    tx: Sender<Cmd>,
    status: Arc<Mutex<Status>>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl DriverHandle {
    /// Start the thread and connect.
    pub fn spawn(params: DriverParams) -> Result<Self, LiveHostError> {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::new()));
        let mut transport = RelayClient::new(params.room_url, params.handle);
        let mut session = Session::new(params.live_doc, params.keys, OsEntropy, params.config);
        let mut first = Vec::new();
        for output in session.start() {
            match output {
                Output::Connect => transport.connect()?,
                other => first.push(other),
            }
        }
        let mut driver = Driver {
            session,
            transport,
            rope: params.rope,
            editor: params.editor,
            doc: params.doc,
            events: params.events,
            sid: params.session_id,
            version: 0,
            status: Arc::clone(&status),
            rx,
            presence_dirty: true,
            last_presence_ms: 0,
            last_presence: Value::Null,
            closed: false,
            replace: params.replace,
            trash: params.trash,
            snap: params
                .snapshots
                .map(|(store, room)| SnapshotSink { store, room }),
            snap_version: Vec::new(),
            last_snap_ms: None,
            hard_error: false,
            clock: 0,
            last_resync_ms: None,
        };
        for output in first {
            driver.absorb(output);
        }
        let thread = std::thread::Builder::new()
            .name("manang-live-session".to_owned())
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

    /// Offer local edits made against `version`.
    pub fn push(&self, version: u64, changes: Vec<Change>) -> Result<PushOutcome, LiveHostError> {
        self.ask(|reply| Cmd::Push {
            version,
            changes,
            reply,
        })?
    }

    /// Announce the local selection (UTF-16 offsets at `version`); `None` clears it.
    pub fn selection(&self, version: u64, selection: Option<(usize, usize)>) {
        // Fire and forget: a lost selection is replaced by the next one.
        let _ = self.tx.send(Cmd::Selection { version, selection });
    }

    /// The document as this side has it, with its version.
    pub fn snapshot(&self) -> Result<(u64, String), LiveHostError> {
        self.ask(|reply| Cmd::Snapshot { reply })
    }

    /// Throw the local document away and rebuild it from the room (the other
    /// copies are the authority). The file keeps its text until someone answers.
    pub fn resync(&self, peer: u64) -> Result<(), LiveHostError> {
        self.ask(|reply| Cmd::Resync { peer, reply })?
    }

    /// Everyone who edited since the last call (for commit trailers).
    pub fn contributors(&self) -> Result<Vec<Contributor>, LiveHostError> {
        self.ask(|reply| Cmd::Contributors { reply })
    }

    /// Current status, without waiting for the thread.
    #[must_use]
    pub fn status(&self) -> Status {
        self.status.lock().clone()
    }

    /// Leave the room and stop the thread. Idempotent.
    pub fn leave(&self) {
        let _ = self.ask(|reply| Cmd::Leave { reply });
        if let Some(thread) = self.thread.lock().take() {
            let _ = thread.join();
        }
    }
}

struct Driver {
    session: Session<LoroEngine, OsEntropy>,
    transport: RelayClient,
    rope: Rope,
    editor: Arc<Editor>,
    doc: DocId,
    events: EventBus,
    sid: String,
    version: u64,
    status: Arc<Mutex<Status>>,
    rx: Receiver<Cmd>,
    presence_dirty: bool,
    last_presence_ms: u64,
    last_presence: Value,
    closed: bool,
    /// The text this join will replace once a host answers (see
    /// [`DriverParams::replace`]); also set by a resync.
    replace: Option<String>,
    trash: Option<Trash>,
    snap: Option<SnapshotSink>,
    snap_version: Vec<u8>,
    last_snap_ms: Option<u64>,
    /// The error cannot go away by itself (the shared copy is gone).
    hard_error: bool,
    /// The session thread's clock, ms since it started.
    clock: u64,
    last_resync_ms: Option<u64>,
}

/// A fresh peer id: unique per replica, below 2^53 (yrs compatible) and not
/// the seed author's.
pub(crate) fn random_peer() -> Result<u64, LiveHostError> {
    let mut buf = [0u8; 8];
    manang_live::envelope::Entropy::fill(&mut OsEntropy, &mut buf)?;
    let id = u64::from_le_bytes(buf) & ((1 << 53) - 1);
    Ok(if id <= manang_live::SEED_PEER {
        id + 2
    } else {
        id
    })
}

fn state_label(state: SessionState) -> (&'static str, u32) {
    match state {
        SessionState::Idle | SessionState::Connecting | SessionState::Syncing => ("connecting", 0),
        SessionState::Waiting => ("waiting", 0),
        SessionState::Live => ("connected", 0),
        SessionState::Reconnecting { attempt, .. } => ("reconnecting", attempt),
        SessionState::Closed => ("left", 0),
    }
}

impl Driver {
    fn run(mut self) {
        let t0 = Instant::now();
        let ms = |t: Instant| u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX);
        while !self.closed {
            match self.rx.recv_timeout(TICK) {
                Ok(cmd) => {
                    self.command(cmd, ms(t0));
                    while !self.closed
                        && let Ok(more) = self.rx.try_recv()
                    {
                        self.command(more, ms(t0));
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.leave(ms(t0));
                    break;
                }
            }
            if self.closed {
                break;
            }
            let now = ms(t0);
            self.clock = now;
            match pump(&mut self.session, &mut self.transport, &mut self.rope, now) {
                Ok(outputs) => {
                    for output in outputs {
                        self.absorb(output);
                    }
                }
                Err(e) => self.fail(&e.to_string()),
            }
            self.emit_presence(now);
            self.save_snapshot(now, false);
        }
    }

    fn command(&mut self, cmd: Cmd, now: u64) {
        self.clock = now;
        match cmd {
            Cmd::Push {
                version,
                changes,
                reply,
            } => {
                let _ = reply.send(self.push(version, &changes));
            }
            Cmd::Selection { version, selection } => {
                if version == self.version && !self.session.awaiting_host() {
                    let outputs = self.session.set_selection(&self.rope, selection);
                    self.perform(outputs);
                }
            }
            Cmd::Snapshot { reply } => {
                // While waiting, the view still shows the file's own text and
                // the session's document is empty: the file is what to re-read.
                let text = if self.replace.is_some() || self.session.awaiting_host() {
                    self.editor.text(Some(self.doc)).unwrap_or_default()
                } else {
                    self.rope.to_string()
                };
                let _ = reply.send((self.version, text));
            }
            Cmd::Resync { peer, reply } => {
                let _ = reply.send(self.resync(peer, now));
            }
            Cmd::Contributors { reply } => {
                let _ = reply.send(self.session.take_contributors());
            }
            Cmd::Leave { reply } => {
                self.leave(now);
                let _ = reply.send(());
            }
        }
    }

    fn leave(&mut self, now: u64) {
        self.save_snapshot(now, true);
        let outputs = self.session.handle(now, Input::Leave, &mut self.rope);
        self.perform(outputs);
        self.closed = true;
    }

    /// Apply local edits made against `version`.
    fn push(&mut self, version: u64, changes: &[Change]) -> Result<PushOutcome, LiveHostError> {
        if self.closed {
            return Err(LiveHostError::NotLive);
        }
        if self.session.awaiting_host() {
            return Err(LiveHostError::Waiting);
        }
        if version != self.version {
            return Ok(PushOutcome::Stale(self.version));
        }
        let mut result = Ok(());
        for change in changes {
            if let Err(e) = self.apply_local(change) {
                result = Err(e);
                break;
            }
        }
        // Even a half-applied batch is a step: the view re-reads the document
        // after an error, and the version must then name what it reads.
        self.version += 1;
        self.status.lock().version = self.version;
        result?;
        self.events.emit(
            "live.remote",
            Payload::Json(json!({
                "session": self.sid,
                "version": self.version,
                "ack": true,
                "changes": [],
            })),
        );
        let flushed = self.session.flush_local();
        self.perform(flushed);
        self.presence_dirty = true;
        Ok(PushOutcome::Accepted(self.version))
    }

    fn apply_local(&mut self, change: &Change) -> Result<(), LiveHostError> {
        let end = change
            .start
            .checked_add(change.delete)
            .ok_or_else(|| LiveHostError::Invalid("change is out of range".to_owned()))?;
        let len = self.rope.len_utf16();
        if end > len {
            return Err(LiveHostError::Invalid(format!(
                "change {}..{end} is outside the document ({len} units)",
                change.start
            )));
        }
        let from = self.rope.utf16_cu_to_char(change.start);
        let to = self.rope.utf16_cu_to_char(end);
        self.session.doc_mut().local_replace_utf16(
            &mut self.rope,
            change.start,
            end,
            &change.insert,
        )?;
        self.editor
            .replace_silent(Some(self.doc), from, to, &change.insert)
            .map_err(|e| LiveHostError::Editor(e.to_string()))?;
        Ok(())
    }

    /// Execute transport outputs; keep the rest for [`Driver::absorb`].
    fn perform(&mut self, outputs: Vec<Output>) {
        for output in outputs {
            let sent = match output {
                Output::Send(frame) => self.transport.send(frame),
                Output::Connect => self.transport.connect(),
                Output::Close => {
                    self.transport.close();
                    Ok(())
                }
                other => {
                    self.absorb(other);
                    Ok(())
                }
            };
            if let Err(e) = sent {
                self.fail(&e.to_string());
            }
        }
    }

    fn absorb(&mut self, output: Output) {
        match output {
            Output::Remote(applied) => self.remote(&applied),
            Output::PresenceChanged => {
                self.presence_dirty = true;
                self.status.lock().peers = self.peers();
            }
            Output::State(state) => {
                let (label, attempt) = state_label(state);
                if state == SessionState::Live {
                    self.replace_with_room();
                }
                {
                    let mut status = self.status.lock();
                    if !self.hard_error {
                        // A later state report means the session moved on: a
                        // soft error is over.
                        status.state = label;
                        status.error = None;
                    }
                    status.attempt = attempt;
                    status.peers = self.peers();
                }
                self.emit_state();
            }
            Output::HostTimeout => self.fail(
                "Nobody has answered yet. The host may have left or be offline. Your file has not been changed; Manang keeps trying.",
            ),
            Output::Diverged { peer } => self.on_diverged(peer),
            Output::Rejected(_) => {
                self.status.lock().rejected += 1;
            }
            Output::Send(_) | Output::Connect | Output::Close => {}
            // Project-session outputs (several files, manifests, blobs). A
            // single-file session never produces them; the project driver
            // handles them in work package P4.
            Output::DocChanged(_)
            | Output::Tree(_)
            | Output::DocWanted(_)
            | Output::DocDiverged { .. }
            | Output::Progress { .. }
            | Output::Manifest { .. }
            | Output::BlobRequested { .. }
            | Output::BlobReceived { .. }
            | Output::BlobFailed { .. } => {}
        }
    }

    fn remote(&mut self, applied: &RemoteApplied) {
        if applied.changes.is_empty() {
            return;
        }
        let mut changes: Vec<Value> = Vec::new();
        match self.blank_for_replace() {
            Ok(Some(blank)) => changes.push(blank.to_json()),
            Ok(None) => {}
            Err(e) => {
                self.fail_hard(&e.to_string());
                return;
            }
        }
        for c in &applied.changes {
            if let Err(e) = self.editor.replace_silent(
                Some(self.doc),
                c.char_start,
                c.char_start + c.char_delete,
                &c.insert,
            ) {
                self.fail_hard(&format!("The shared copy of the file is gone: {e}"));
                return;
            }
        }
        changes.extend(applied.changes.iter().map(|c| {
            Change {
                start: c.utf16_start,
                delete: c.utf16_delete,
                insert: c.insert.clone(),
            }
            .to_json()
        }));
        self.step(changes);
        self.recover();
    }

    /// One accepted remote step: bump the version and tell the view.
    fn step(&mut self, changes: Vec<Value>) {
        self.version += 1;
        self.status.lock().version = self.version;
        self.events.emit(
            "live.remote",
            Payload::Json(json!({
                "session": self.sid,
                "version": self.version,
                "ack": false,
                "changes": changes,
            })),
        );
        self.presence_dirty = true;
    }

    /// The room has answered: the file's old text goes (a copy was saved at
    /// join, and the editor's text is saved again if it was edited since).
    /// Returns the view change that removes it, `None` if nothing to remove.
    fn blank_for_replace(&mut self) -> Result<Option<Change>, LiveHostError> {
        let Some(original) = self.replace.take() else {
            return Ok(None);
        };
        let current = self
            .editor
            .text(Some(self.doc))
            .map_err(|e| LiveHostError::Editor(e.to_string()))?;
        if current.is_empty() {
            return Ok(None);
        }
        if current != original
            && let Some(trash) = &self.trash
        {
            trash.save(&current, "edited")?;
        }
        self.editor
            .replace_silent(Some(self.doc), 0, current.chars().count(), "")
            .map_err(|e| LiveHostError::Editor(e.to_string()))?;
        Ok(Some(Change {
            start: 0,
            delete: current.encode_utf16().count(),
            insert: String::new(),
        }))
    }

    /// The session went live with nothing to apply (the room's text is empty):
    /// the old text still has to go.
    fn replace_with_room(&mut self) {
        if self.replace.is_none() {
            return;
        }
        match self.blank_for_replace() {
            Ok(Some(blank)) => self.step(vec![blank.to_json()]),
            Ok(None) => {}
            Err(e) => self.fail_hard(&e.to_string()),
        }
    }

    /// Rebuild the document from the room: save the file's text, start from an
    /// empty document under a new peer id and ask again. The editor keeps its
    /// text until somebody answers.
    fn resync(&mut self, peer: u64, now: u64) -> Result<(), LiveHostError> {
        if self.closed {
            return Err(LiveHostError::NotLive);
        }
        let current = self
            .editor
            .text(Some(self.doc))
            .map_err(|e| LiveHostError::Editor(e.to_string()))?;
        if !current.is_empty()
            && let Some(trash) = &self.trash
        {
            trash.save(&current, "resync")?;
        }
        let doc = LiveDoc::empty(peer)?;
        self.rope = Rope::new();
        self.replace = Some(current);
        self.last_resync_ms = Some(now);
        let outputs = self.session.resync(now, doc);
        self.perform(outputs);
        Ok(())
    }

    /// The copies disagree at the same version. The peer with the higher id
    /// rebuilds from the other (so two peers never both wait for each other);
    /// the other keeps going and answers.
    fn on_diverged(&mut self, peer: u64) {
        if self.session.peer() <= peer {
            return;
        }
        if self
            .last_resync_ms
            .is_some_and(|t| self.clock.saturating_sub(t) < RESYNC_GAP_MS)
        {
            return;
        }
        let result = random_peer().and_then(|p| self.resync(p, self.clock));
        if let Err(e) = result {
            self.fail(&format!("The copies of this file no longer match: {e}"));
        }
    }

    /// Write the document snapshot (debounced) when the document changed.
    /// A failed write is ignored on purpose: the snapshot is a cache, and the
    /// file on disk and the room still hold everything.
    fn save_snapshot(&mut self, now: u64, force: bool) {
        let Some(sink) = &self.snap else {
            return;
        };
        if self.session.awaiting_host() {
            return; // an empty joiner's document must never shadow the room's
        }
        if !force
            && self
                .last_snap_ms
                .is_some_and(|t| now.saturating_sub(t) < SNAPSHOT_GAP_MS)
        {
            return;
        }
        let engine = self.session.doc().engine();
        let version = engine.version();
        if version == self.snap_version {
            return;
        }
        self.last_snap_ms = Some(now);
        if let Ok(bytes) = engine.export_snapshot()
            && sink.store.save(&sink.room, &engine.text(), &bytes).is_ok()
        {
            self.snap_version = version;
        }
    }

    /// A problem that may pass: the next state change or the next remote
    /// step clears it.
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

    /// A problem that cannot pass (the shared copy of the file is gone).
    fn fail_hard(&mut self, message: &str) {
        self.hard_error = true;
        {
            let mut status = self.status.lock();
            status.state = "error";
            status.error = Some(message.to_owned());
        }
        self.emit_state();
    }

    /// Data is flowing again: a soft error is over.
    fn recover(&mut self) {
        if self.hard_error {
            return;
        }
        let (label, attempt) = state_label(self.session.state());
        {
            let mut status = self.status.lock();
            if status.state != "error" {
                return;
            }
            status.state = label;
            status.attempt = attempt;
            status.error = None;
        }
        self.emit_state();
    }

    fn peers(&self) -> Vec<PeerInfo> {
        self.session
            .peers()
            .into_iter()
            .map(|(peer, name, slot)| PeerInfo { peer, name, slot })
            .collect()
    }

    fn emit_state(&self) {
        let status = self.status.lock().clone();
        self.events.emit(
            "live.state",
            Payload::Json(json!({
                "session": self.sid,
                "state": status.state,
                "attempt": status.attempt,
                "error": status.error,
                "peers": status.peers.iter().map(PeerInfo::to_json).collect::<Vec<_>>(),
            })),
        );
    }

    /// Resolved remote carets, at most every [`PRESENCE_GAP_MS`] and only when
    /// something moved.
    fn emit_presence(&mut self, now: u64) {
        if !self.presence_dirty || now.saturating_sub(self.last_presence_ms) < PRESENCE_GAP_MS {
            return;
        }
        self.presence_dirty = false;
        self.last_presence_ms = now;
        let selections: Vec<Value> = self
            .session
            .remote_selections(&self.rope)
            .into_iter()
            .map(|s| {
                json!({
                    "peer": s.peer.to_string(),
                    "name": s.name,
                    "slot": s.slot,
                    "anchor": s.anchor,
                    "head": s.head,
                })
            })
            .collect();
        let payload = json!({
            "session": self.sid,
            "version": self.version,
            "selections": selections,
        });
        if payload == self.last_presence {
            return;
        }
        self.last_presence = payload.clone();
        self.events.emit("live.presence", Payload::Json(payload));
    }
}
