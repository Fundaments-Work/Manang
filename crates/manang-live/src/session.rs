//! The live session state machine.
//!
//! Sans-IO: the session consumes [`Input`]s (transport events, frames, ticks,
//! local selection changes) and returns [`Output`]s (frames to send, remote
//! changes already applied to the rope, state changes). It never touches a
//! socket, a clock or a thread, which makes it deterministic to test and
//! identical on desktop and in a browser. Time is passed in as milliseconds.
//!
//! ```text
//!   Idle --start--> Connecting --Up--> Syncing --first Reply / timeout--> Live
//!                       ^                                                   |
//!                       |                                                 Down
//!                       +--- retry timer fires --- Reconnecting <-----------+
//!   any --leave--> Closed
//! ```
//!
//! Local edits flow `rope -> LiveDoc -> flush_local() -> sealed Update frame`.
//! While disconnected, edits accumulate in the CRDT; the Request/Reply/Final
//! exchange on reconnect (and a periodic anti-entropy request) delivers them,
//! so no outbound queue is needed and frames lost in flight self-heal.

use crate::bridge::{LiveDoc, RemoteApplied, TextTarget};
use crate::engine::TextCrdt;
use crate::envelope::{Entropy, Kind, RoomKeys};
use crate::error::LiveError;
use crate::presence::{Presence, PresenceBook, PresenceMessage, Selection};
use crate::protocol::{Message, SyncStage};

/// Tunables, in milliseconds.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Name shown next to this peer's caret.
    pub name: String,
    /// Colour slot for this peer (`0..PALETTE_SLOTS`).
    pub slot: u8,
    /// Anti-entropy request period while live.
    pub resync_interval_ms: u64,
    /// Presence heartbeat period.
    pub presence_interval_ms: u64,
    /// Silence after which a peer's presence is dropped.
    pub presence_ttl_ms: u64,
    /// How long to wait for a first Reply before declaring ourselves live
    /// (we may be alone in the room).
    pub sync_timeout_ms: u64,
    /// First reconnect delay; doubles per failed attempt.
    pub backoff_base_ms: u64,
    /// Reconnect delay ceiling.
    pub backoff_max_ms: u64,
    /// Minimum gap between pending-triggered resync requests.
    pub pending_request_gap_ms: u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            slot: 0,
            resync_interval_ms: 10_000,
            presence_interval_ms: 5_000,
            presence_ttl_ms: 20_000,
            sync_timeout_ms: 3_000,
            backoff_base_ms: 500,
            backoff_max_ms: 15_000,
            pending_request_gap_ms: 1_000,
        }
    }
}

/// Where the session is in its life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Constructed, not started.
    Idle,
    /// Waiting for the transport to open.
    Connecting,
    /// Connected; exchanging state with the room.
    Syncing,
    /// Connected and caught up (eventually: updates keep flowing).
    Live,
    /// Connection lost; a retry is scheduled.
    Reconnecting {
        /// Failed attempts so far.
        attempt: u32,
        /// Time at which to reconnect.
        retry_at_ms: u64,
    },
    /// Left the room for good.
    Closed,
}

/// Things the session reacts to.
#[derive(Debug, Clone, Copy)]
pub enum Input<'a> {
    /// The transport opened.
    Up,
    /// The transport dropped.
    Down,
    /// A frame arrived.
    Frame(&'a [u8]),
    /// Time passed.
    Tick,
    /// Leave the room.
    Leave,
}

/// Things the session asks its driver to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Output {
    /// Open the transport.
    Connect,
    /// Close the transport.
    Close,
    /// Send this frame.
    Send(Vec<u8>),
    /// Remote edits were applied to the rope; mirror them in the editor view.
    Remote(RemoteApplied),
    /// The set of remote peers or their carets changed; redraw them.
    PresenceChanged,
    /// The state changed.
    State(SessionState),
    /// A frame was dropped (wrong key, tampered, malformed). Informational:
    /// never fatal, because the relay and other peers are not trusted to send
    /// well-formed data.
    Rejected(String),
}

/// A remote peer's caret, resolved to the current text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSelection {
    /// The peer.
    pub peer: u64,
    /// Display name.
    pub name: String,
    /// Colour slot.
    pub slot: u8,
    /// Anchor, UTF-16 offset.
    pub anchor: usize,
    /// Head (caret), UTF-16 offset.
    pub head: usize,
}

/// A live editing session over one document.
pub struct Session<E: TextCrdt, N: Entropy> {
    doc: LiveDoc<E>,
    keys: RoomKeys,
    entropy: N,
    config: SessionConfig,
    state: SessionState,
    presence: PresenceBook,
    presence_seq: u64,
    local_selection: Option<Selection>,
    next_resync: u64,
    next_presence: u64,
    sync_deadline: u64,
    last_pending_request: Option<u64>,
    failed_attempts: u32,
}

impl<E: TextCrdt, N: Entropy> Session<E, N> {
    /// A session over `doc` in the room identified by `keys`.
    pub fn new(doc: LiveDoc<E>, keys: RoomKeys, entropy: N, config: SessionConfig) -> Self {
        let ttl = config.presence_ttl_ms;
        Self {
            doc,
            keys,
            entropy,
            config,
            state: SessionState::Idle,
            presence: PresenceBook::new(ttl),
            presence_seq: 0,
            local_selection: None,
            next_resync: 0,
            next_presence: 0,
            sync_deadline: 0,
            last_pending_request: None,
            failed_attempts: 0,
        }
    }

    /// This replica's peer id.
    #[must_use]
    pub fn peer(&self) -> u64 {
        self.doc.engine().peer()
    }

    /// Current state.
    #[must_use]
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// The bridged document (read access).
    #[must_use]
    pub fn doc(&self) -> &LiveDoc<E> {
        &self.doc
    }

    /// The bridged document, for applying local edits. Call
    /// [`Session::flush_local`] afterwards.
    pub fn doc_mut(&mut self) -> &mut LiveDoc<E> {
        &mut self.doc
    }

    /// Begin: ask the driver to open the transport.
    pub fn start(&mut self) -> Vec<Output> {
        let mut out = Vec::new();
        self.set_state(SessionState::Connecting, &mut out);
        out.push(Output::Connect);
        out
    }

    /// Turn local edits made through [`Session::doc_mut`] into frames.
    pub fn flush_local(&mut self) -> Vec<Output> {
        let mut out = Vec::new();
        let updates = match self.doc.take_local_updates() {
            Ok(u) => u,
            Err(e) => {
                out.push(Output::Rejected(e.to_string()));
                return out;
            }
        };
        if !matches!(self.state, SessionState::Syncing | SessionState::Live) {
            return out; // the reconnect exchange will carry them
        }
        let from = self.peer();
        for update in updates {
            self.push_data(Message::Update { from, update }, &mut out);
        }
        out
    }

    /// Set (or clear) this peer's selection, as UTF-16 offsets, and announce it.
    pub fn set_selection<T: TextTarget>(
        &mut self,
        target: &T,
        selection: Option<(usize, usize)>,
    ) -> Vec<Output> {
        self.local_selection = selection.and_then(|(anchor, head)| {
            Some(Selection {
                anchor: self.doc.cursor_at_utf16(target, anchor)?,
                head: self.doc.cursor_at_utf16(target, head)?,
            })
        });
        let mut out = Vec::new();
        if matches!(self.state, SessionState::Syncing | SessionState::Live) {
            self.push_presence(&mut out);
        }
        out
    }

    /// Remote carets resolved against the current text.
    #[must_use]
    pub fn remote_selections<T: TextTarget>(&self, target: &T) -> Vec<RemoteSelection> {
        self.presence
            .iter()
            .filter_map(|p| {
                let sel = p.selection.as_ref()?;
                Some(RemoteSelection {
                    peer: p.peer,
                    name: p.name.clone(),
                    slot: p.slot,
                    anchor: self.doc.cursor_pos_utf16(target, &sel.anchor)?,
                    head: self.doc.cursor_pos_utf16(target, &sel.head)?,
                })
            })
            .collect()
    }

    /// Remote peers currently present (with or without a caret).
    #[must_use]
    pub fn peers(&self) -> Vec<(u64, String, u8)> {
        self.presence
            .iter()
            .map(|p| (p.peer, p.name.clone(), p.slot))
            .collect()
    }

    /// Feed one input at time `now_ms`; remote edits are applied to `target`.
    pub fn handle<T: TextTarget>(
        &mut self,
        now_ms: u64,
        input: Input<'_>,
        target: &mut T,
    ) -> Vec<Output> {
        let mut out = Vec::new();
        if self.state == SessionState::Closed {
            return out;
        }
        match input {
            Input::Up => self.on_up(now_ms, &mut out),
            Input::Down => self.on_down(now_ms, &mut out),
            Input::Frame(bytes) => self.on_frame(now_ms, bytes, target, &mut out),
            Input::Tick => self.on_tick(now_ms, &mut out),
            Input::Leave => self.on_leave(&mut out),
        }
        out
    }

    fn set_state(&mut self, state: SessionState, out: &mut Vec<Output>) {
        if self.state != state {
            self.state = state;
            out.push(Output::State(state));
        }
    }

    fn on_up(&mut self, now: u64, out: &mut Vec<Output>) {
        self.failed_attempts = 0;
        self.sync_deadline = now + self.config.sync_timeout_ms;
        self.next_resync = now + self.config.resync_interval_ms;
        self.next_presence = now + self.config.presence_interval_ms;
        self.set_state(SessionState::Syncing, out);
        self.push_request(out);
        self.push_presence(out);
    }

    fn on_down(&mut self, now: u64, out: &mut Vec<Output>) {
        if !self.presence.is_empty() {
            self.presence.clear();
            out.push(Output::PresenceChanged);
        }
        let attempt = self.failed_attempts;
        self.failed_attempts = self.failed_attempts.saturating_add(1);
        let shift = attempt.min(16);
        let delay = (self.config.backoff_base_ms << shift).min(self.config.backoff_max_ms);
        self.set_state(
            SessionState::Reconnecting {
                attempt: attempt + 1,
                retry_at_ms: now + delay,
            },
            out,
        );
    }

    fn on_tick(&mut self, now: u64, out: &mut Vec<Output>) {
        match self.state {
            SessionState::Reconnecting { retry_at_ms, .. } if now >= retry_at_ms => {
                self.set_state(SessionState::Connecting, out);
                out.push(Output::Connect);
            }
            SessionState::Syncing if now >= self.sync_deadline => {
                self.set_state(SessionState::Live, out);
            }
            SessionState::Live => {
                if now >= self.next_resync {
                    self.next_resync = now + self.config.resync_interval_ms;
                    self.push_request(out);
                }
                if now >= self.next_presence {
                    self.next_presence = now + self.config.presence_interval_ms;
                    self.push_presence(out);
                }
            }
            _ => {}
        }
        if self.presence.expire(now) > 0 {
            out.push(Output::PresenceChanged);
        }
    }

    fn on_leave(&mut self, out: &mut Vec<Output>) {
        if matches!(self.state, SessionState::Syncing | SessionState::Live) {
            let msg = PresenceMessage::Leave { peer: self.peer() };
            if let Ok(plain) = msg.encode() {
                self.push_sealed(Kind::Presence, &plain, out);
            }
        }
        self.presence.clear();
        self.set_state(SessionState::Closed, out);
        out.push(Output::Close);
    }

    fn on_frame<T: TextTarget>(
        &mut self,
        now: u64,
        bytes: &[u8],
        target: &mut T,
        out: &mut Vec<Output>,
    ) {
        let (kind, plain) = match self.keys.open(bytes) {
            Ok(v) => v,
            Err(e) => {
                out.push(Output::Rejected(e.to_string()));
                return;
            }
        };
        match kind {
            Kind::Data => match Message::decode(&plain) {
                Ok(msg) => self.on_message(now, msg, target, out),
                Err(e) => out.push(Output::Rejected(e.to_string())),
            },
            Kind::Presence => match PresenceMessage::decode(&plain) {
                Ok(PresenceMessage::State(p)) if p.peer != self.peer() => {
                    if self.presence.update(p, now) {
                        out.push(Output::PresenceChanged);
                    }
                }
                Ok(PresenceMessage::Leave { peer }) if peer != self.peer() => {
                    if self.presence.remove(peer) {
                        out.push(Output::PresenceChanged);
                    }
                }
                Ok(_) => {}
                Err(e) => out.push(Output::Rejected(e.to_string())),
            },
        }
    }

    fn on_message<T: TextTarget>(
        &mut self,
        now: u64,
        msg: Message,
        target: &mut T,
        out: &mut Vec<Output>,
    ) {
        let me = self.peer();
        match msg {
            Message::Update { from, update } if from != me => {
                self.apply(now, &update, target, out);
            }
            Message::Sync {
                from,
                to,
                stage,
                version,
                update,
            } if from != me && to.is_none_or(|t| t == me) => match stage {
                SyncStage::Request => {
                    let known = self.presence.iter().any(|p| p.peer == from);
                    if version != self.doc.engine().version() {
                        self.push_reply(from, &version, out);
                    }
                    if !known {
                        self.push_presence(out);
                    }
                }
                SyncStage::Reply => {
                    self.apply(now, &update, target, out);
                    if self.state == SessionState::Syncing {
                        self.set_state(SessionState::Live, out);
                    }
                    if version != self.doc.engine().version() {
                        self.push_final(from, &version, out);
                    }
                }
                SyncStage::Final => {
                    self.apply(now, &update, target, out);
                }
            },
            _ => {}
        }
    }

    fn apply<T: TextTarget>(
        &mut self,
        now: u64,
        update: &[u8],
        target: &mut T,
        out: &mut Vec<Output>,
    ) {
        if update.is_empty() {
            return;
        }
        match self.doc.apply_remote(target, update) {
            Ok(applied) => {
                let pending = applied.pending;
                if !applied.changes.is_empty() {
                    out.push(Output::Remote(applied));
                }
                if pending {
                    let due = self
                        .last_pending_request
                        .is_none_or(|t| now >= t + self.config.pending_request_gap_ms);
                    if due {
                        self.last_pending_request = Some(now);
                        self.push_request(out);
                    }
                }
            }
            Err(e) => out.push(Output::Rejected(e.to_string())),
        }
    }

    fn push_request(&mut self, out: &mut Vec<Output>) {
        let msg = Message::Sync {
            from: self.peer(),
            to: None,
            stage: SyncStage::Request,
            version: self.doc.engine().version(),
            update: Vec::new(),
        };
        self.push_data(msg, out);
    }

    fn push_reply(&mut self, to: u64, their_version: &[u8], out: &mut Vec<Output>) {
        match self.doc.engine().export_since(their_version) {
            Ok(update) => {
                let msg = Message::Sync {
                    from: self.peer(),
                    to: Some(to),
                    stage: SyncStage::Reply,
                    version: self.doc.engine().version(),
                    update,
                };
                self.push_data(msg, out);
            }
            Err(e) => out.push(Output::Rejected(e.to_string())),
        }
    }

    fn push_final(&mut self, to: u64, their_version: &[u8], out: &mut Vec<Output>) {
        match self.doc.engine().export_since(their_version) {
            Ok(update) => {
                let msg = Message::Sync {
                    from: self.peer(),
                    to: Some(to),
                    stage: SyncStage::Final,
                    version: self.doc.engine().version(),
                    update,
                };
                self.push_data(msg, out);
            }
            Err(e) => out.push(Output::Rejected(e.to_string())),
        }
    }

    fn push_data(&mut self, msg: Message, out: &mut Vec<Output>) {
        match msg.encode() {
            Ok(plain) => self.push_sealed(Kind::Data, &plain, out),
            Err(e) => out.push(Output::Rejected(e.to_string())),
        }
    }

    fn push_presence(&mut self, out: &mut Vec<Output>) {
        self.presence_seq += 1;
        let msg = PresenceMessage::State(Presence {
            peer: self.peer(),
            seq: self.presence_seq,
            name: self.config.name.clone(),
            slot: self.config.slot,
            selection: self.local_selection.clone(),
        });
        match msg.encode() {
            Ok(plain) => self.push_sealed(Kind::Presence, &plain, out),
            Err(e) => out.push(Output::Rejected(e.to_string())),
        }
    }

    fn push_sealed(&mut self, kind: Kind, plain: &[u8], out: &mut Vec<Output>) {
        match self.keys.seal(kind, plain, &mut self.entropy) {
            Ok(frame) => out.push(Output::Send(frame)),
            Err(e) => out.push(Output::Rejected(e.to_string())),
        }
    }
}

/// Convenience for drivers: pump a transport into a session and the session's
/// outputs back out. Returns outputs the caller still has to act on (remote
/// changes for the view, presence and state changes); `Send`, `Connect` and
/// `Close` are performed here.
pub fn pump<E, N, T, X>(
    session: &mut Session<E, N>,
    transport: &mut X,
    target: &mut T,
    now_ms: u64,
) -> Result<Vec<Output>, LiveError>
where
    E: TextCrdt,
    N: Entropy,
    T: TextTarget,
    X: crate::transport::SyncTransport,
{
    use crate::transport::TransportEvent;
    let mut pending: Vec<Output> = Vec::new();
    let mut inputs: Vec<TransportEvent> = Vec::new();
    while let Some(ev) = transport.poll() {
        inputs.push(ev);
    }
    let mut batches = Vec::new();
    for ev in &inputs {
        batches.push(match ev {
            TransportEvent::Up => session.handle(now_ms, Input::Up, target),
            TransportEvent::Down => session.handle(now_ms, Input::Down, target),
            TransportEvent::Frame(f) => session.handle(now_ms, Input::Frame(f), target),
        });
    }
    batches.push(session.handle(now_ms, Input::Tick, target));
    batches.push(session.flush_local());
    for batch in batches {
        for output in batch {
            match output {
                Output::Send(frame) => transport.send(frame)?,
                Output::Connect => transport.connect()?,
                Output::Close => transport.close(),
                other => pending.push(other),
            }
        }
    }
    Ok(pending)
}
