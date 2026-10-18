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

/// How often the thread wakes with nothing to do (polls the socket, ticks timers).
const TICK: Duration = Duration::from_millis(10);
/// Minimum gap between presence events.
const PRESENCE_GAP_MS: u64 = 40;
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
    /// `connecting`, `connected`, `reconnecting`, `left` or `error`.
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
}

fn state_label(state: SessionState) -> (&'static str, u32) {
    match state {
        SessionState::Idle | SessionState::Connecting | SessionState::Syncing => ("connecting", 0),
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
            match pump(&mut self.session, &mut self.transport, &mut self.rope, now) {
                Ok(outputs) => {
                    for output in outputs {
                        self.absorb(output);
                    }
                }
                Err(e) => self.fail(&e.to_string()),
            }
            self.emit_presence(now);
        }
    }

    fn command(&mut self, cmd: Cmd, now: u64) {
        match cmd {
            Cmd::Push {
                version,
                changes,
                reply,
            } => {
                let _ = reply.send(self.push(version, &changes));
            }
            Cmd::Selection { version, selection } => {
                if version == self.version {
                    let outputs = self.session.set_selection(&self.rope, selection);
                    self.perform(outputs);
                }
            }
            Cmd::Snapshot { reply } => {
                let _ = reply.send((self.version, self.rope.to_string()));
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
        let outputs = self.session.handle(now, Input::Leave, &mut self.rope);
        self.perform(outputs);
        self.closed = true;
    }

    /// Apply local edits made against `version`.
    fn push(&mut self, version: u64, changes: &[Change]) -> Result<PushOutcome, LiveHostError> {
        if self.closed {
            return Err(LiveHostError::NotLive);
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
                {
                    let mut status = self.status.lock();
                    if status.state != "error" {
                        status.state = label;
                    }
                    status.attempt = attempt;
                    status.peers = self.peers();
                }
                self.emit_state();
            }
            Output::Diverged { .. } => {
                self.fail(
                    "The copies of this file no longer match. Leave and join again from the host's invite.",
                );
            }
            Output::Rejected(_) => {
                self.status.lock().rejected += 1;
            }
            Output::Send(_) | Output::Connect | Output::Close => {}
        }
    }

    fn remote(&mut self, applied: &RemoteApplied) {
        if applied.changes.is_empty() {
            return;
        }
        for c in &applied.changes {
            if let Err(e) = self.editor.replace_silent(
                Some(self.doc),
                c.char_start,
                c.char_start + c.char_delete,
                &c.insert,
            ) {
                self.fail(&format!("The shared copy of the file is gone: {e}"));
                return;
            }
        }
        self.version += 1;
        self.status.lock().version = self.version;
        let changes: Vec<Value> = applied
            .changes
            .iter()
            .map(|c| {
                Change {
                    start: c.utf16_start,
                    delete: c.utf16_delete,
                    insert: c.insert.clone(),
                }
                .to_json()
            })
            .collect();
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

    fn fail(&mut self, message: &str) {
        {
            let mut status = self.status.lock();
            status.state = "error";
            status.error = Some(message.to_owned());
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
