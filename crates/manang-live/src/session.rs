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
//! A session that joins somebody else's room (`SessionConfig::await_host`)
//! has nothing to show until a peer answers, so it enters [`SessionState::Waiting`]
//! instead of `Syncing` and leaves it only on a real answer, never on a timer:
//! a joiner alone in an empty room must not look "live". It keeps asking, and
//! reports [`Output::HostTimeout`] once if nobody answers within
//! `host_timeout_ms`. Answering later still works. While waiting it does not
//! answer other peers' requests (its document is empty and would mislead them).
//!
//! Local edits flow `rope -> LiveDoc -> flush_local() -> sealed Update frame`.
//! While disconnected, edits accumulate in the CRDT; the Request/Reply/Final
//! exchange on reconnect (and a periodic anti-entropy request) delivers them,
//! so no outbound queue is needed and frames lost in flight self-heal.
//!
//! # Project sessions
//!
//! [`Session::new_project`] turns the same machine into a router over a whole
//! project: the project tree ([`ProjectDoc`]) plus one [`LiveDoc`] per text
//! file, created lazily on first touch, and content-addressed blobs for the
//! rest. Frames demultiplex by the wire v2 messages (`DocSync`, `DocUpdate`,
//! `Manifest`, `ManifestGet`, `BlobGet`, `BlobChunk`). Single-document
//! sessions ([`Session::new`]) keep the original wire format and behaviour.
//!
//! * **Targets.** [`Session::handle_with`] takes a [`TextTargets`] set that
//!   maps a file to the editor buffer showing it. A file without a target is
//!   applied to its CRDT only (the session reports [`Output::DocChanged`] so
//!   the disk bridge can write it).
//! * **Lazy documents.** A peer asking for a file we have not loaded makes
//!   the session report [`Output::DocWanted`] and remember the request; the
//!   caller answers with [`Session::provide_doc`] (seeded from disk or a
//!   sidecar snapshot) or [`Session::want_doc`] (start empty and fetch).
//! * **Join.** A guest asks for the manifest (`ManifestGet`), the session
//!   reassembles and verifies it and reports [`Output::Manifest`]; the caller
//!   classifies local files and then loads documents and blobs.
//! * **Anti-entropy.** The project tree every resync tick, loaded documents
//!   round-robin a few at a time; each document has its own digest check, and
//!   a guest whose copy diverged re-fetches that one file
//!   ([`Output::DocDiverged`]) instead of failing the session.
//! * **Pacing.** Every outbound frame passes a [`Pacer`] so a bulk first sync
//!   stays under the relay limits; open files and live edits go first.
//! * **Blobs.** One blob in flight per direction and peer, verified by
//!   SHA-256, bounded by [`MAX_BLOB_BYTES`]; bad chunks are counted and
//!   dropped, never fatal.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ops::Bound;
use std::sync::Arc;

use crate::blob::{BlobAssembler, BlobHash, MAX_BLOB_BYTES, blob_hash, chunk_count, chunk_range};
use crate::bridge::{LiveDoc, RemoteApplied, TextTarget};
use crate::engine::{SEED_PEER, TextCrdt};
use crate::envelope::{Entropy, Kind, RoomKeys};
use crate::error::LiveError;
use crate::manifest::{DocId, ManifestAssembler, ManifestEntry, manifest_root, paginate};
use crate::pacer::{Class, PaceConfig, Pacer};
use crate::presence::{FilePeer, Presence, PresenceBook, PresenceMessage, PresenceV2, Selection};
use crate::project::{FileId, NodeKind, ProjectDoc, ProjectView, TreeChange};
use crate::protocol::{Message, SyncStage, text_digest};
use crate::vcs::{ContributionLog, Contributor};

/// Flags carried in the (otherwise empty) `update` of a Request, as one byte.
/// Bit 0, the waiting mark, is set by a session that is still waiting for its
/// first answer: a peer that sees it always replies, even when the versions
/// match and the sender is already known, so a joiner with an identical copy
/// learns that somebody is there (older peers ignore the field of a Request;
/// the mark alone is the byte `[1]`, as in wave 0). Bit 1 asks the replier to
/// treat the answer as interactive (the requester has the file open).
/// Unknown bits are ignored.
const FLAG_UNSYNCED: u8 = 1;
const FLAG_PRIORITY: u8 = 2;

fn request_flags(update: &[u8]) -> u8 {
    match update {
        [b] => *b & (FLAG_UNSYNCED | FLAG_PRIORITY),
        _ => 0,
    }
}

/// Most documents with unanswered requests remembered while the caller loads
/// them, and most requesters remembered per document.
const MAX_DEFERRED_DOCS: usize = 1024;
const MAX_DEFERRED_PEERS: usize = 8;
/// Most senders whose manifest pages are assembled at once.
const MAX_MANIFEST_SENDERS: usize = 8;
/// Most blob requests remembered while the caller reads the blob.
const MAX_DEFERRED_GETS: usize = 64;
/// Smallest gap between a refetch of the same document.
const REFETCH_GAP_MS: u64 = 30_000;
/// A peer fetching one blob may switch to another after this much idleness.
const BLOB_SWITCH_IDLE_MS: u64 = 10_000;
/// Attempts at one chunk (or whole blob, after a hash mismatch) before giving up.
const BLOB_MAX_TRIES: u32 = 6;

/// Tunables, in milliseconds.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Name shown next to this peer's caret.
    pub name: String,
    /// Colour slot for this peer (`0..PALETTE_SLOTS`).
    pub slot: u8,
    /// Commit identity email to share with the room (opt-in), for trailers.
    pub email: Option<String>,
    /// Anti-entropy request period while live.
    pub resync_interval_ms: u64,
    /// Presence heartbeat period.
    pub presence_interval_ms: u64,
    /// Silence after which a peer's presence is dropped.
    pub presence_ttl_ms: u64,
    /// How long to wait for a first Reply before declaring ourselves live
    /// (we may be alone in the room). Not used when `await_host` is set.
    pub sync_timeout_ms: u64,
    /// This session joins a room somebody else started: stay in
    /// [`SessionState::Waiting`] until a peer answers (never go live alone).
    pub await_host: bool,
    /// While waiting, how long before [`Output::HostTimeout`] is reported.
    pub host_timeout_ms: u64,
    /// While waiting, how often the request is repeated.
    pub waiting_request_ms: u64,
    /// First reconnect delay; doubles per failed attempt.
    pub backoff_base_ms: u64,
    /// Reconnect delay ceiling.
    pub backoff_max_ms: u64,
    /// Minimum gap between pending-triggered resync requests.
    pub pending_request_gap_ms: u64,
    /// Project sessions: outbound pacing (see [`Pacer`]). A
    /// `bytes_per_sec` of zero disables it.
    pub pace: PaceConfig,
    /// Project sessions: loaded documents asked per anti-entropy tick.
    pub doc_resync_batch: usize,
    /// Project sessions: wait before asking again for a missing blob chunk.
    pub blob_retry_ms: u64,
    /// Project sessions: smallest gap between manifests served to one peer.
    pub manifest_serve_gap_ms: u64,
    /// Project sessions: how long a blob offered for serving stays in memory
    /// after its last use.
    pub offered_blob_ttl_ms: u64,
    /// Project sessions: most bytes of offered blobs kept in memory.
    pub offered_blob_max_bytes: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            slot: 0,
            email: None,
            resync_interval_ms: 10_000,
            presence_interval_ms: 5_000,
            presence_ttl_ms: 20_000,
            sync_timeout_ms: 3_000,
            await_host: false,
            host_timeout_ms: 20_000,
            waiting_request_ms: 2_000,
            backoff_base_ms: 500,
            backoff_max_ms: 15_000,
            pending_request_gap_ms: 1_000,
            pace: PaceConfig::default(),
            doc_resync_batch: 8,
            blob_retry_ms: 3_000,
            manifest_serve_gap_ms: 1_000,
            offered_blob_ttl_ms: 120_000,
            offered_blob_max_bytes: 128 * 1024 * 1024,
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
    /// Connected to the room but no peer has answered yet, and the local
    /// document is not the room's (an empty joiner). Nothing is replaced and
    /// nothing is shown as live until someone answers.
    Waiting,
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
    /// In a project session `RemoteApplied::doc` names the file.
    Remote(RemoteApplied),
    /// Project sessions: a remote change was applied to the CRDT of a file
    /// that has no text target (its disk copy should be rewritten).
    DocChanged(FileId),
    /// Project sessions: the project tree changed through a remote update.
    Tree(Vec<TreeChange>),
    /// Project sessions: a peer asked for a file whose document is not
    /// loaded. Answer with [`Session::provide_doc`] or [`Session::want_doc`];
    /// the peer's request is remembered and answered then.
    DocWanted(FileId),
    /// Project sessions: one file's replica diverged from a peer's (same
    /// version, different text). A guest re-fetches that file
    /// (`refetching`); a host only reports it.
    DocDiverged {
        /// The file.
        doc: FileId,
        /// The peer whose digest disagrees.
        peer: u64,
        /// Whether this session dropped its copy and asked again.
        refetching: bool,
    },
    /// Project sessions: fetch progress of the files and blobs requested with
    /// [`Session::want_doc`] and [`Session::want_blob`]. `done == total` ends
    /// a batch.
    Progress {
        /// Completed items.
        done: usize,
        /// Items requested in this batch.
        total: usize,
    },
    /// Project sessions: the host's manifest, verified against its root hash.
    /// Classify local files against it, then load documents and blobs.
    Manifest {
        /// The peer that sent it.
        from: u64,
        /// [`manifest_root`] of the entries.
        root: [u8; 16],
        /// Every node of the project.
        entries: Vec<ManifestEntry>,
    },
    /// Project sessions: a peer asked for a blob that is not offered. Answer
    /// with [`Session::offer_blob`] or [`Session::decline_blob`].
    BlobRequested {
        /// The asking peer.
        peer: u64,
        /// Content address.
        hash: BlobHash,
    },
    /// Project sessions: a wanted blob arrived and its SHA-256 matches.
    BlobReceived {
        /// Content address.
        hash: BlobHash,
        /// The content.
        data: Vec<u8>,
    },
    /// Project sessions: a wanted blob could not be fetched (nobody served
    /// it, or the content never matched its hash).
    BlobFailed {
        /// Content address.
        hash: BlobHash,
    },
    /// The set of remote peers or their carets changed; redraw them.
    PresenceChanged,
    /// The state changed.
    State(SessionState),
    /// A peer reports the same version as ours but different text: the
    /// replicas diverged and no further exchange can repair it. The driver
    /// should rejoin from a snapshot of the host. Never expected in a healthy
    /// room.
    Diverged {
        /// The peer whose digest disagrees.
        peer: u64,
    },
    /// Nobody answered within `host_timeout_ms` while waiting. Reported once
    /// per connection; the session keeps asking and recovers if someone answers.
    HostTimeout,
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

/// A remote peer present in a file of a project session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePresence {
    /// The peer.
    pub peer: u64,
    /// Display name.
    pub name: String,
    /// Colour slot.
    pub slot: u8,
    /// Whether the peer is editing this file (not merely holding it open).
    pub active: bool,
}

/// Counters of hostile or unexpected input, for diagnostics and tests. None
/// of these is fatal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionStats {
    /// Frames dropped before or while being understood (wrong key, tampered,
    /// malformed, rejected by a bound).
    pub rejected_frames: u64,
    /// Blob chunks refused (bad index, length, count, or hash mismatch).
    pub blob_rejected: u64,
    /// Blob chunks for a blob nobody here asked for.
    pub blob_unsolicited: u64,
    /// Blob requests refused because the peer already had another blob in
    /// flight.
    pub blob_busy: u64,
    /// Messages about a document that is neither loaded nor a text file of
    /// the project tree.
    pub unknown_doc: u64,
    /// Same-version, different-text exchanges seen.
    pub divergences: u64,
}

/// Where remote edits of a project session are applied: the editor buffer
/// showing each file.
///
/// A file for which [`TextTargets::target`] returns `None` is applied to its
/// CRDT only. `BTreeMap<FileId, T>` implements this for any [`TextTarget`].
pub trait TextTargets {
    /// The buffer showing `doc`, if one is open.
    fn target(&mut self, doc: FileId) -> Option<&mut dyn TextTarget>;
    /// The buffer of a single-document session.
    fn single(&mut self) -> Option<&mut dyn TextTarget> {
        None
    }
}

/// A target set with nothing open: every file is applied to its CRDT only.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoTargets;

impl TextTargets for NoTargets {
    fn target(&mut self, _doc: FileId) -> Option<&mut dyn TextTarget> {
        None
    }
}

impl<T: TextTarget> TextTargets for BTreeMap<FileId, T> {
    fn target(&mut self, doc: FileId) -> Option<&mut dyn TextTarget> {
        self.get_mut(&doc).map(|t| t as &mut dyn TextTarget)
    }
}

/// Adapter for the single-document [`Session::handle`].
struct OneTarget<'a, T: TextTarget>(&'a mut T);

impl<T: TextTarget> TextTargets for OneTarget<'_, T> {
    fn target(&mut self, _doc: FileId) -> Option<&mut dyn TextTarget> {
        None
    }
    fn single(&mut self) -> Option<&mut dyn TextTarget> {
        Some(&mut *self.0)
    }
}

/// Which document a sync message is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    /// The one document of a single-document session.
    Single,
    /// The project tree.
    Project,
    /// One file of a project session.
    File(FileId),
}

impl Key {
    fn of(doc: DocId) -> Self {
        match doc {
            DocId::Project => Self::Project,
            DocId::File(id) => Self::File(id),
        }
    }
}

/// A text document with its sync bookkeeping.
struct FileSlot<E: TextCrdt> {
    doc: LiveDoc<E>,
    digest_cache: Option<(Vec<u8>, u64)>,
    last_pending_request: Option<u64>,
    /// A peer has answered for this document (or it is the room's by origin).
    synced: bool,
    /// Counted in [`Output::Progress`] until it is synced.
    counted: bool,
    last_reset: Option<u64>,
}

impl<E: TextCrdt> FileSlot<E> {
    fn new(doc: LiveDoc<E>, synced: bool, counted: bool) -> Self {
        Self {
            doc,
            digest_cache: None,
            last_pending_request: None,
            synced,
            counted,
            last_reset: None,
        }
    }

    /// Digest of the text, cached per version so anti-entropy stays cheap.
    fn digest(&mut self) -> u64 {
        let version = self.doc.engine().version();
        if let Some((v, d)) = &self.digest_cache
            && *v == version
        {
            return *d;
        }
        let d = text_digest(&self.doc.engine().text());
        self.digest_cache = Some((version, d));
        d
    }
}

/// The project tree with its sync bookkeeping.
struct ProjectSlot {
    doc: ProjectDoc,
    last_pending_request: Option<u64>,
    view_cache: Option<(Vec<u8>, Arc<ProjectView>)>,
}

/// A request for a document that was not loaded yet.
struct Deferred {
    version: Vec<u8>,
    digest: u64,
    flags: u8,
}

/// What applying a remote text update did.
enum TextOutcome {
    Applied(RemoteApplied),
    Detached {
        changed: bool,
        pending: bool,
        units: usize,
    },
}

fn apply_to_slot<E: TextCrdt>(
    slot: &mut FileSlot<E>,
    update: &[u8],
    target: Option<&mut dyn TextTarget>,
) -> Result<TextOutcome, LiveError> {
    match target {
        Some(t) => slot.doc.apply_remote(t, update).map(TextOutcome::Applied),
        None => {
            let before = slot.doc.engine().version();
            let len_before = slot.doc.engine().len_bytes();
            let pending = slot.doc.apply_remote_detached(update)?;
            let changed = slot.doc.engine().version() != before;
            let delta = slot.doc.engine().len_bytes().abs_diff(len_before);
            Ok(TextOutcome::Detached {
                changed,
                pending,
                units: if changed { delta.max(1) } else { 0 },
            })
        }
    }
}

/// A blob being fetched.
struct ActiveBlob {
    asm: BlobAssembler,
    hash: BlobHash,
    requested_at: u64,
    tries: u32,
    mismatches: u32,
}

/// An offered blob kept for serving.
struct Offered {
    data: Arc<Vec<u8>>,
    last_used: u64,
}

#[derive(Default)]
struct BlobState {
    queue: VecDeque<(BlobHash, u64)>,
    active: Option<ActiveBlob>,
    offered: BTreeMap<BlobHash, Offered>,
    offered_bytes: usize,
    deferred_gets: Vec<(u64, BlobHash, u32)>,
    asked: BTreeSet<BlobHash>,
    /// Per requesting peer: the blob it is being served and when it last
    /// asked.
    serving: BTreeMap<u64, (BlobHash, u64)>,
}

/// A live editing session over one document, or over a whole project.
pub struct Session<E: TextCrdt, N: Entropy> {
    /// The document of a single-document session. In a project session it is
    /// an inert empty placeholder kept so [`Session::doc`] stays total.
    single: FileSlot<E>,
    project: Option<ProjectSlot>,
    docs: BTreeMap<FileId, FileSlot<E>>,
    keys: RoomKeys,
    entropy: N,
    config: SessionConfig,
    state: SessionState,
    presence: PresenceBook,
    presence_seq: u64,
    contributions: ContributionLog,
    local_selection: Option<Selection>,
    focus_doc: Option<FileId>,
    focus_open: Vec<FileId>,
    priority_docs: BTreeSet<FileId>,
    next_resync: u64,
    next_presence: u64,
    sync_deadline: u64,
    host_deadline: u64,
    next_wait_request: u64,
    host_timeout_reported: bool,
    awaiting_host: bool,
    failed_attempts: u32,
    now: u64,
    pacer: Pacer,
    deferred: BTreeMap<FileId, BTreeMap<u64, Deferred>>,
    wanted_reported: BTreeSet<FileId>,
    rr_cursor: Option<FileId>,
    unsynced_cursor: Option<FileId>,
    next_unsynced_retry: u64,
    manifest_wanted: bool,
    manifest_asm: BTreeMap<u64, ManifestAssembler>,
    manifest_served: BTreeMap<u64, u64>,
    blobs: BlobState,
    progress_total: usize,
    progress_done: usize,
    progress_sent: Option<(usize, usize)>,
    stats: SessionStats,
}

impl<E: TextCrdt, N: Entropy> Session<E, N> {
    /// A single-document session over `doc` in the room identified by `keys`.
    pub fn new(doc: LiveDoc<E>, keys: RoomKeys, entropy: N, config: SessionConfig) -> Self {
        Self::build(
            FileSlot::new(doc, false, false),
            None,
            keys,
            entropy,
            config,
        )
    }

    /// A project session over `project` in the room identified by `keys`
    /// (derive them with [`crate::envelope::RoomSecret::derive_project`]). A
    /// host passes its tree built from disk; a guest (`config.await_host`)
    /// passes `ProjectDoc::new(peer)` and receives the tree and the manifest
    /// from the host.
    pub fn new_project(
        project: ProjectDoc,
        keys: RoomKeys,
        entropy: N,
        config: SessionConfig,
    ) -> Result<Self, LiveError> {
        let placeholder = FileSlot::new(LiveDoc::empty(project.peer())?, true, false);
        let slot = ProjectSlot {
            doc: project,
            last_pending_request: None,
            view_cache: None,
        };
        Ok(Self::build(placeholder, Some(slot), keys, entropy, config))
    }

    fn build(
        single: FileSlot<E>,
        project: Option<ProjectSlot>,
        keys: RoomKeys,
        entropy: N,
        config: SessionConfig,
    ) -> Self {
        let ttl = config.presence_ttl_ms;
        let awaiting_host = config.await_host;
        let pacer = Pacer::new(config.pace);
        Self {
            single,
            project,
            docs: BTreeMap::new(),
            keys,
            entropy,
            config,
            state: SessionState::Idle,
            presence: PresenceBook::new(ttl),
            presence_seq: 0,
            contributions: ContributionLog::default(),
            local_selection: None,
            focus_doc: None,
            focus_open: Vec::new(),
            priority_docs: BTreeSet::new(),
            next_resync: 0,
            next_presence: 0,
            sync_deadline: 0,
            host_deadline: 0,
            next_wait_request: 0,
            host_timeout_reported: false,
            awaiting_host,
            failed_attempts: 0,
            now: 0,
            pacer,
            deferred: BTreeMap::new(),
            wanted_reported: BTreeSet::new(),
            rr_cursor: None,
            unsynced_cursor: None,
            next_unsynced_retry: 0,
            manifest_wanted: awaiting_host,
            manifest_asm: BTreeMap::new(),
            manifest_served: BTreeMap::new(),
            blobs: BlobState::default(),
            progress_total: 0,
            progress_done: 0,
            progress_sent: None,
            stats: SessionStats::default(),
        }
    }

    /// This replica's peer id.
    #[must_use]
    pub fn peer(&self) -> u64 {
        match &self.project {
            Some(p) => p.doc.peer(),
            None => self.single.doc.engine().peer(),
        }
    }

    /// Current state.
    #[must_use]
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Whether this is a project session.
    #[must_use]
    pub fn is_project(&self) -> bool {
        self.project.is_some()
    }

    /// Whether the session still waits for a first answer from the room (its
    /// document is not yet the room's).
    #[must_use]
    pub fn awaiting_host(&self) -> bool {
        self.awaiting_host
    }

    /// Counters of rejected and unexpected input.
    #[must_use]
    pub fn stats(&self) -> SessionStats {
        self.stats
    }

    /// Bytes waiting in the outbound pacing queue.
    #[must_use]
    pub fn queued_bytes(&self) -> usize {
        self.pacer.queued_bytes()
    }

    /// Frames waiting in the outbound pacing queue.
    #[must_use]
    pub fn queued_frames(&self) -> usize {
        self.pacer.queued_frames()
    }

    /// Replace the document with `doc` (normally empty, with a fresh peer id)
    /// and ask the room for its state again. The caller owns the text target
    /// and must reset it to match `doc`. Used to recover from
    /// [`Output::Diverged`]: the next answer from the room rebuilds the
    /// document and arrives as ordinary remote changes. Single-document
    /// sessions only (a project session re-fetches diverged files itself).
    pub fn resync(&mut self, now_ms: u64, doc: LiveDoc<E>) -> Vec<Output> {
        let mut out = Vec::new();
        if self.state == SessionState::Closed || self.project.is_some() {
            return out;
        }
        self.single = FileSlot::new(doc, false, false);
        self.local_selection = None;
        self.awaiting_host = true;
        if matches!(
            self.state,
            SessionState::Syncing | SessionState::Waiting | SessionState::Live
        ) {
            self.now = now_ms;
            self.on_up(now_ms, &mut out);
        }
        out
    }

    /// The bridged document of a single-document session (read access).
    #[must_use]
    pub fn doc(&self) -> &LiveDoc<E> {
        &self.single.doc
    }

    /// The bridged document of a single-document session, for applying local
    /// edits. Call [`Session::flush_local`] afterwards.
    pub fn doc_mut(&mut self) -> &mut LiveDoc<E> {
        &mut self.single.doc
    }

    /// Begin: ask the driver to open the transport.
    pub fn start(&mut self) -> Vec<Output> {
        let mut out = Vec::new();
        self.set_state(SessionState::Connecting, &mut out);
        out.push(Output::Connect);
        out
    }

    /// Turn local edits (single document: made through [`Session::doc_mut`];
    /// project: tree operations and every loaded file) into frames.
    pub fn flush_local(&mut self) -> Vec<Output> {
        let mut out = Vec::new();
        if self.project.is_none() {
            self.flush_key(Key::Single, &mut out);
        } else {
            self.flush_key(Key::Project, &mut out);
            let ids: Vec<FileId> = self.docs.keys().copied().collect();
            for id in ids {
                self.flush_key(Key::File(id), &mut out);
            }
        }
        self.end(&mut out);
        out
    }

    /// Project sessions: turn the local edits of one file into frames (the
    /// keystroke path; cheaper than [`Session::flush_local`]).
    pub fn flush_doc(&mut self, id: FileId) -> Vec<Output> {
        let mut out = Vec::new();
        self.flush_key(Key::File(id), &mut out);
        self.end(&mut out);
        out
    }

    /// Project sessions: turn local tree operations into frames.
    pub fn flush_project(&mut self) -> Vec<Output> {
        let mut out = Vec::new();
        self.flush_key(Key::Project, &mut out);
        self.end(&mut out);
        out
    }

    fn flush_key(&mut self, key: Key, out: &mut Vec<Output>) {
        let updates = match key {
            Key::Single => self.single.doc.take_local_updates(),
            Key::Project => match self.project.as_mut() {
                Some(p) => p.doc.take_local_updates(),
                None => return,
            },
            Key::File(id) => match self.docs.get_mut(&id) {
                Some(s) => s.doc.take_local_updates(),
                None => return,
            },
        };
        let updates = match updates {
            Ok(u) => u,
            Err(e) => {
                self.reject(out, e.to_string());
                return;
            }
        };
        if !self.is_connected() {
            return; // the reconnect exchange will carry them
        }
        for update in updates {
            let msg = self.update_message(key, update);
            self.push_data(Class::Interactive, msg, out);
        }
    }

    /// Set (or clear) this peer's selection, as UTF-16 offsets, and announce it.
    pub fn set_selection<T: TextTarget>(
        &mut self,
        target: &T,
        selection: Option<(usize, usize)>,
    ) -> Vec<Output> {
        self.local_selection = selection.and_then(|(anchor, head)| {
            Some(Selection {
                anchor: self.single.doc.cursor_at_utf16(target, anchor)?,
                head: self.single.doc.cursor_at_utf16(target, head)?,
            })
        });
        let mut out = Vec::new();
        if self.is_connected() && !self.awaiting_host {
            self.push_presence(&mut out);
        }
        self.end(&mut out);
        out
    }

    /// Project sessions: set (or clear) this peer's selection in file `doc`,
    /// as UTF-16 offsets of `target`, and make `doc` the file being edited.
    pub fn set_file_selection(
        &mut self,
        doc: FileId,
        target: &dyn TextTarget,
        selection: Option<(usize, usize)>,
    ) -> Vec<Output> {
        self.focus_doc = Some(doc);
        if !self.focus_open.contains(&doc) && self.focus_open.len() < crate::presence::MAX_OPEN {
            self.focus_open.push(doc);
        }
        self.priority_docs.insert(doc);
        self.local_selection = match (self.docs.get(&doc), selection) {
            (Some(slot), Some((anchor, head))) => {
                match (
                    slot.doc.cursor_at_utf16(target, anchor),
                    slot.doc.cursor_at_utf16(target, head),
                ) {
                    (Some(anchor), Some(head)) => Some(Selection { anchor, head }),
                    _ => None,
                }
            }
            _ => None,
        };
        let mut out = Vec::new();
        if self.is_connected() && !self.awaiting_host {
            self.push_presence(&mut out);
        }
        self.end(&mut out);
        out
    }

    /// Project sessions: announce which file this peer is editing and which
    /// are open. Open files are synced ahead of the rest.
    pub fn set_focus(&mut self, doc: Option<FileId>, open: Vec<FileId>) -> Vec<Output> {
        if self.focus_doc != doc {
            self.local_selection = None;
        }
        self.focus_doc = doc;
        self.focus_open = open;
        self.focus_open.truncate(crate::presence::MAX_OPEN);
        let mut out = Vec::new();
        if self.is_connected() && !self.awaiting_host {
            self.push_presence(&mut out);
        }
        self.end(&mut out);
        out
    }

    fn is_connected(&self) -> bool {
        matches!(
            self.state,
            SessionState::Syncing | SessionState::Waiting | SessionState::Live
        )
    }

    /// Remote carets resolved against the current text (single document).
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
                    anchor: self.single.doc.cursor_pos_utf16(target, &sel.anchor)?,
                    head: self.single.doc.cursor_pos_utf16(target, &sel.head)?,
                })
            })
            .collect()
    }

    /// Project sessions: remote carets in file `doc`, resolved against
    /// `target`.
    #[must_use]
    pub fn remote_file_selections(
        &self,
        doc: FileId,
        target: &dyn TextTarget,
    ) -> Vec<RemoteSelection> {
        let Some(slot) = self.docs.get(&doc) else {
            return Vec::new();
        };
        self.presence
            .iter_v2()
            .filter(|s| s.doc == Some(doc))
            .filter_map(|s| {
                let sel = s.presence.selection.as_ref()?;
                Some(RemoteSelection {
                    peer: s.presence.peer,
                    name: s.presence.name.clone(),
                    slot: s.presence.slot,
                    anchor: slot.doc.cursor_pos_utf16(target, &sel.anchor)?,
                    head: slot.doc.cursor_pos_utf16(target, &sel.head)?,
                })
            })
            .collect()
    }

    /// Everyone who edited the document through this session since the last
    /// call, for `Co-authored-by:` trailers. Call when a commit is made.
    pub fn take_contributors(&mut self) -> Vec<Contributor> {
        self.contributions.take()
    }

    /// Remote peers currently present (with or without a caret).
    #[must_use]
    pub fn peers(&self) -> Vec<(u64, String, u8)> {
        self.presence
            .iter()
            .map(|p| (p.peer, p.name.clone(), p.slot))
            .collect()
    }

    /// The presence book (peers with their open files and carets).
    #[must_use]
    pub fn presence(&self) -> &PresenceBook {
        &self.presence
    }

    /// Project sessions: the remote peers that have `file` open or are
    /// editing it.
    #[must_use]
    pub fn peers_by_file(&self, file: FileId) -> Vec<FilePresence> {
        self.presence
            .peers_by_file(file)
            .into_iter()
            .map(|FilePeer { presence, active }| FilePresence {
                peer: presence.peer,
                name: presence.name.clone(),
                slot: presence.slot,
                active,
            })
            .collect()
    }

    // -- project accessors ---------------------------------------------------

    /// The project tree (project sessions).
    #[must_use]
    pub fn project(&self) -> Option<&ProjectDoc> {
        self.project.as_ref().map(|p| &p.doc)
    }

    /// The project tree, for local tree operations (project sessions). Call
    /// [`Session::flush_project`] afterwards.
    pub fn project_mut(&mut self) -> Option<&mut ProjectDoc> {
        self.project.as_mut().map(|p| &mut p.doc)
    }

    /// The resolved project as of now, cached per tree version.
    pub fn project_view(&mut self) -> Option<Arc<ProjectView>> {
        let p = self.project.as_mut()?;
        let version = p.doc.version();
        if let Some((v, view)) = &p.view_cache
            && *v == version
        {
            return Some(Arc::clone(view));
        }
        let view = Arc::new(p.doc.view());
        p.view_cache = Some((version, Arc::clone(&view)));
        Some(view)
    }

    /// A loaded file document (read access).
    #[must_use]
    pub fn file_doc(&self, id: FileId) -> Option<&LiveDoc<E>> {
        self.docs.get(&id).map(|s| &s.doc)
    }

    /// A loaded file document, for applying local edits. Call
    /// [`Session::flush_doc`] afterwards.
    pub fn file_doc_mut(&mut self, id: FileId) -> Option<&mut LiveDoc<E>> {
        self.docs.get_mut(&id).map(|s| &mut s.doc)
    }

    /// Ids of the loaded file documents, ascending.
    pub fn loaded_docs(&self) -> impl Iterator<Item = FileId> + '_ {
        self.docs.keys().copied()
    }

    /// Whether the file's document is loaded.
    #[must_use]
    pub fn is_loaded(&self, id: FileId) -> bool {
        self.docs.contains_key(&id)
    }

    /// Whether a peer has answered for the file's document yet.
    #[must_use]
    pub fn is_doc_synced(&self, id: FileId) -> bool {
        self.docs.get(&id).is_some_and(|s| s.synced)
    }

    /// Project sessions: load `id` as an empty document and fetch it from the
    /// room. `priority` syncs it ahead of background files (use it for files
    /// the user has open). Counts toward [`Output::Progress`].
    pub fn want_doc(&mut self, now_ms: u64, id: FileId, priority: bool) -> Vec<Output> {
        let mut out = Vec::new();
        if self.project.is_none() || self.state == SessionState::Closed {
            return out;
        }
        self.now = now_ms;
        if priority {
            self.priority_docs.insert(id);
        }
        if self.docs.contains_key(&id) {
            self.push_request(Key::File(id), &mut out);
        } else {
            match LiveDoc::empty(self.peer()) {
                Ok(doc) => {
                    self.progress_total += 1;
                    self.install_doc(now_ms, id, FileSlot::new(doc, false, true), &mut out);
                }
                Err(e) => self.reject(&mut out, e.to_string()),
            }
        }
        self.end(&mut out);
        out
    }

    /// Project sessions: load `id` from a document the caller built: seeded
    /// from the file on disk (identical copies merge without duplicating
    /// text, see [`TextCrdt::seeded`]) or restored from a sidecar snapshot.
    /// Requests that peers made while the document was missing are answered
    /// now. Replaces an already loaded document only if it holds no state
    /// from the room yet.
    pub fn provide_doc(
        &mut self,
        now_ms: u64,
        id: FileId,
        doc: LiveDoc<E>,
        priority: bool,
    ) -> Vec<Output> {
        let mut out = Vec::new();
        if self.project.is_none() || self.state == SessionState::Closed {
            return out;
        }
        self.now = now_ms;
        if priority {
            self.priority_docs.insert(id);
        }
        if self.docs.get(&id).is_some_and(|s| s.synced) {
            self.reject(&mut out, "document already loaded".to_owned());
        } else {
            // A fetch that was pending stays counted: the provided document
            // replaces the empty one and is synced by the same first answer.
            let counted = self.docs.get(&id).is_some_and(|s| s.counted);
            let slot = FileSlot::new(doc, false, counted);
            self.install_doc(now_ms, id, slot, &mut out);
        }
        self.end(&mut out);
        out
    }

    fn install_doc(&mut self, now: u64, id: FileId, slot: FileSlot<E>, out: &mut Vec<Output>) {
        self.docs.insert(id, slot);
        self.wanted_reported.remove(&id);
        let replay = self.deferred.remove(&id).unwrap_or_default();
        if replay.is_empty() {
            self.push_request(Key::File(id), out);
        }
        for (from, d) in replay {
            let req = Req {
                from,
                version: &d.version,
                digest: d.digest,
                flags: d.flags,
            };
            self.answer_request(Key::File(id), &req, now, &mut NoTargets, out);
        }
    }

    /// Project sessions: drop a loaded document from memory and return its
    /// snapshot for a sidecar (restore it with [`Session::provide_doc`]).
    /// `Ok(None)` if the document is not loaded.
    pub fn unload_doc(&mut self, id: FileId) -> Result<Option<Vec<u8>>, LiveError> {
        let Some(slot) = self.docs.remove(&id) else {
            return Ok(None);
        };
        if slot.counted && !slot.synced {
            self.progress_total = self.progress_total.saturating_sub(1);
        }
        self.priority_docs.remove(&id);
        slot.doc.engine().export_snapshot().map(Some)
    }

    // -- driving -------------------------------------------------------------

    /// Feed one input at time `now_ms`; remote edits are applied to `target`
    /// (single-document sessions).
    pub fn handle<T: TextTarget>(
        &mut self,
        now_ms: u64,
        input: Input<'_>,
        target: &mut T,
    ) -> Vec<Output> {
        self.handle_with(now_ms, input, &mut OneTarget(target))
    }

    /// Feed one input at time `now_ms`; remote edits are applied to the
    /// buffers `targets` maps the files to (project sessions).
    pub fn handle_with(
        &mut self,
        now_ms: u64,
        input: Input<'_>,
        targets: &mut dyn TextTargets,
    ) -> Vec<Output> {
        let mut out = Vec::new();
        if self.state == SessionState::Closed {
            return out;
        }
        self.now = now_ms;
        match input {
            Input::Up => self.on_up(now_ms, &mut out),
            Input::Down => self.on_down(now_ms, &mut out),
            Input::Frame(bytes) => self.on_frame(now_ms, bytes, targets, &mut out),
            Input::Tick => self.on_tick(now_ms, &mut out),
            Input::Leave => self.on_leave(&mut out),
        }
        self.end(&mut out);
        out
    }

    fn set_state(&mut self, state: SessionState, out: &mut Vec<Output>) {
        if self.state != state {
            self.state = state;
            out.push(Output::State(state));
        }
    }

    fn reject(&mut self, out: &mut Vec<Output>, why: String) {
        self.stats.rejected_frames += 1;
        out.push(Output::Rejected(why));
    }

    /// Release paced frames and report progress; runs at the end of every
    /// public entry point.
    fn end(&mut self, out: &mut Vec<Output>) {
        if self.project.is_none() {
            return;
        }
        if self.is_connected() {
            let mut due = Vec::new();
            self.pacer.drain(self.now, &mut due);
            out.extend(due.into_iter().map(Output::Send));
        }
        let now_progress = (self.progress_done, self.progress_total);
        if self.progress_total > 0 && self.progress_sent != Some(now_progress) {
            self.progress_sent = Some(now_progress);
            out.push(Output::Progress {
                done: self.progress_done,
                total: self.progress_total,
            });
            if self.progress_done >= self.progress_total {
                self.progress_done = 0;
                self.progress_total = 0;
                self.progress_sent = None;
            }
        }
    }

    fn on_up(&mut self, now: u64, out: &mut Vec<Output>) {
        self.failed_attempts = 0;
        self.sync_deadline = now + self.config.sync_timeout_ms;
        self.host_deadline = now + self.config.host_timeout_ms;
        self.next_wait_request = now + self.config.waiting_request_ms;
        self.host_timeout_reported = false;
        self.next_resync = now + self.config.resync_interval_ms;
        self.next_presence = now + self.config.presence_interval_ms;
        self.next_unsynced_retry = now + self.config.waiting_request_ms * 2;
        let next = if self.awaiting_host {
            SessionState::Waiting
        } else {
            SessionState::Syncing
        };
        self.set_state(next, out);
        self.pacer.clear();
        if self.project.is_some() {
            self.request_everything(out);
        } else {
            self.push_request(Key::Single, out);
        }
        self.push_presence(out);
    }

    /// Ask for the manifest (guests) and the state of the tree and of every
    /// loaded document.
    fn request_everything(&mut self, out: &mut Vec<Output>) {
        if self.manifest_wanted {
            self.push_manifest_get(out);
        }
        self.push_request(Key::Project, out);
        if !self.awaiting_host {
            let ids: Vec<FileId> = self.docs.keys().copied().collect();
            for id in ids {
                self.push_request(Key::File(id), out);
            }
        }
        if let Some(active) = &mut self.blobs.active {
            active.requested_at = self.now;
        }
        self.pump_blob(out);
    }

    fn on_down(&mut self, now: u64, out: &mut Vec<Output>) {
        if !self.presence.is_empty() {
            self.presence.clear();
            out.push(Output::PresenceChanged);
        }
        self.pacer.clear();
        self.blobs.serving.clear();
        self.blobs.deferred_gets.clear();
        self.manifest_asm.clear();
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
            SessionState::Waiting => {
                if now >= self.host_deadline && !self.host_timeout_reported {
                    self.host_timeout_reported = true;
                    out.push(Output::HostTimeout);
                }
                if now >= self.next_wait_request {
                    self.next_wait_request = now + self.config.waiting_request_ms;
                    if self.project.is_some() {
                        if self.manifest_wanted {
                            self.push_manifest_get(out);
                        }
                        self.push_request(Key::Project, out);
                    } else {
                        self.push_request(Key::Single, out);
                    }
                    self.push_presence(out);
                }
            }
            SessionState::Live => {
                if now >= self.next_resync {
                    self.next_resync = now + self.config.resync_interval_ms;
                    if self.project.is_some() {
                        self.push_request(Key::Project, out);
                        self.anti_entropy_docs(out);
                    } else {
                        self.push_request(Key::Single, out);
                    }
                }
                if self.project.is_some() && now >= self.next_unsynced_retry {
                    self.next_unsynced_retry = now + self.config.waiting_request_ms * 2;
                    self.retry_unsynced(out);
                }
                if self.project.is_some() && self.manifest_wanted && now >= self.next_wait_request {
                    // Some page of the manifest was lost: ask again.
                    self.next_wait_request = now + self.config.waiting_request_ms;
                    self.push_manifest_get(out);
                }
                if now >= self.next_presence {
                    self.next_presence = now + self.config.presence_interval_ms;
                    self.push_presence(out);
                }
            }
            _ => {}
        }
        if self.project.is_some() && self.is_connected() {
            self.blob_tick(now, out);
        }
        if self.presence.expire(now) > 0 {
            out.push(Output::PresenceChanged);
        }
    }

    /// Ask again for documents no peer has answered for yet (the request or
    /// its answer was lost, or nobody had the file loaded), round-robin.
    fn retry_unsynced(&mut self, out: &mut Vec<Output>) {
        let batch = self.config.doc_resync_batch.max(1) * 4;
        let after = self.unsynced_cursor;
        let mut ids: Vec<FileId> = self
            .docs
            .iter()
            .filter(|(k, s)| !s.synced && after.is_none_or(|c| **k > c))
            .map(|(k, _)| *k)
            .take(batch)
            .collect();
        if ids.len() < batch {
            let more: Vec<FileId> = self
                .docs
                .iter()
                .filter(|(k, s)| !s.synced && !ids.contains(k))
                .map(|(k, _)| *k)
                .take(batch - ids.len())
                .collect();
            ids.extend(more);
        }
        self.unsynced_cursor = ids.last().copied();
        for id in ids {
            self.push_request(Key::File(id), out);
        }
    }

    /// Ask for the state of the next few loaded documents, round-robin.
    fn anti_entropy_docs(&mut self, out: &mut Vec<Output>) {
        let batch = self.config.doc_resync_batch.max(1);
        let mut ids: BTreeSet<FileId> = BTreeSet::new();
        if let Some(cursor) = self.rr_cursor {
            ids.extend(
                self.docs
                    .range((Bound::Excluded(cursor), Bound::Unbounded))
                    .map(|(k, _)| *k)
                    .take(batch),
            );
        }
        if ids.len() < batch {
            let need = batch - ids.len();
            let wrap: Vec<FileId> = self
                .docs
                .keys()
                .copied()
                .filter(|k| !ids.contains(k))
                .take(need)
                .collect();
            ids.extend(wrap);
        }
        self.rr_cursor = ids.iter().next_back().copied();
        for id in ids {
            self.push_request(Key::File(id), out);
        }
    }

    fn on_leave(&mut self, out: &mut Vec<Output>) {
        if self.is_connected() {
            let msg = PresenceMessage::Leave { peer: self.peer() };
            if let Ok(plain) = msg.encode() {
                self.push_sealed(Class::Interactive, Kind::Presence, &plain, out);
            }
            // The leave notice must not wait behind bulk traffic.
            let mut due = Vec::new();
            self.pacer.drain(self.now, &mut due);
            out.extend(due.into_iter().map(Output::Send));
        }
        self.pacer.clear();
        self.presence.clear();
        self.set_state(SessionState::Closed, out);
        out.push(Output::Close);
    }

    fn on_frame(
        &mut self,
        now: u64,
        bytes: &[u8],
        targets: &mut dyn TextTargets,
        out: &mut Vec<Output>,
    ) {
        let (kind, plain) = match self.keys.open(bytes) {
            Ok(v) => v,
            Err(e) => {
                self.reject(out, e.to_string());
                return;
            }
        };
        match kind {
            Kind::Data => match Message::decode(&plain) {
                Ok(msg) => self.on_message(now, msg, targets, out),
                Err(e) => self.reject(out, e.to_string()),
            },
            Kind::Presence => match PresenceMessage::decode(&plain) {
                Ok(PresenceMessage::State(p)) if p.peer != self.peer() => {
                    if self.presence.update(p, now) {
                        out.push(Output::PresenceChanged);
                    }
                }
                Ok(PresenceMessage::StateV2(p)) if p.presence.peer != self.peer() => {
                    if self.presence.update_v2(p, now) {
                        out.push(Output::PresenceChanged);
                    }
                }
                Ok(PresenceMessage::Leave { peer }) if peer != self.peer() => {
                    if self.presence.remove(peer) {
                        out.push(Output::PresenceChanged);
                    }
                }
                Ok(_) => {}
                Err(e) => self.reject(out, e.to_string()),
            },
        }
    }

    fn on_message(
        &mut self,
        now: u64,
        msg: Message,
        targets: &mut dyn TextTargets,
        out: &mut Vec<Output>,
    ) {
        let me = self.peer();
        let project = self.project.is_some();
        match msg {
            Message::Update { from, update } if !project && from != me => {
                self.on_update(Key::Single, now, from, &update, targets, out);
            }
            Message::Sync {
                from,
                to,
                stage,
                version,
                digest,
                update,
            } if !project => {
                let m = SyncIn {
                    from,
                    to,
                    stage,
                    version: &version,
                    digest,
                    update: &update,
                };
                self.on_sync(Key::Single, now, m, targets, out);
            }
            Message::DocUpdate { doc, from, update } if project && from != me => {
                self.on_update(Key::of(doc), now, from, &update, targets, out);
            }
            Message::DocSync {
                doc,
                from,
                to,
                stage,
                version,
                digest,
                update,
            } if project => {
                let m = SyncIn {
                    from,
                    to,
                    stage,
                    version: &version,
                    digest,
                    update: &update,
                };
                self.on_sync(Key::of(doc), now, m, targets, out);
            }
            Message::ManifestGet { from, to }
                if project && from != me && to.is_none_or(|t| t == me) =>
            {
                self.on_manifest_get(now, from, out);
            }
            Message::Manifest {
                from,
                to,
                root,
                page,
                pages,
                entries,
            } if project && from != me && to.is_none_or(|t| t == me) => {
                self.on_manifest(from, root, page, pages, entries, out);
            }
            Message::BlobGet {
                from,
                to,
                hash,
                chunk,
            } if project && from != me && to.is_none_or(|t| t == me) => {
                self.on_blob_get(now, from, hash, chunk, out);
            }
            Message::BlobChunk {
                from,
                to,
                hash,
                chunk,
                count,
                data,
            } if project && from != me && to.is_none_or(|t| t == me) => {
                self.on_blob_chunk(hash, chunk, count, &data, out);
            }
            _ => {}
        }
    }

    // -- documents -----------------------------------------------------------

    fn is_loaded_key(&self, key: Key) -> bool {
        match key {
            Key::Single => true,
            Key::Project => self.project.is_some(),
            Key::File(id) => self.docs.contains_key(&id),
        }
    }

    fn slot_version(&self, key: Key) -> Vec<u8> {
        match key {
            Key::Single => self.single.doc.engine().version(),
            Key::Project => self
                .project
                .as_ref()
                .map(|p| p.doc.version())
                .unwrap_or_default(),
            Key::File(id) => self
                .docs
                .get(&id)
                .map(|s| s.doc.engine().version())
                .unwrap_or_default(),
        }
    }

    /// Digest of the document's text; the tree has none (0).
    fn slot_digest(&mut self, key: Key) -> u64 {
        match key {
            Key::Single => self.single.digest(),
            Key::Project => 0,
            Key::File(id) => self.docs.get_mut(&id).map_or(0, FileSlot::digest),
        }
    }

    fn slot_export_since(&self, key: Key, since: &[u8]) -> Result<Vec<u8>, LiveError> {
        match key {
            Key::Single => self.single.doc.engine().export_since(since),
            Key::Project => match &self.project {
                Some(p) => p.doc.export_since(since),
                None => Ok(Vec::new()),
            },
            Key::File(id) => match self.docs.get(&id) {
                Some(s) => s.doc.engine().export_since(since),
                None => Ok(Vec::new()),
            },
        }
    }

    fn is_priority(&self, key: Key) -> bool {
        match key {
            Key::File(id) => self.priority_docs.contains(&id) || self.focus_open.contains(&id),
            Key::Single | Key::Project => true,
        }
    }

    /// Whether `key` still lacks the room's state (its Requests carry the
    /// waiting mark so a peer with an identical version still answers).
    fn is_unsynced(&self, key: Key) -> bool {
        match key {
            Key::Single | Key::Project => self.awaiting_host,
            Key::File(id) => self.docs.get(&id).is_some_and(|s| !s.synced),
        }
    }

    fn sync_message(
        &self,
        key: Key,
        to: Option<u64>,
        stage: SyncStage,
        digest: u64,
        version: Vec<u8>,
        update: Vec<u8>,
    ) -> Message {
        let from = self.peer();
        match key {
            Key::Single => Message::Sync {
                from,
                to,
                stage,
                version,
                digest,
                update,
            },
            Key::Project => Message::DocSync {
                doc: DocId::Project,
                from,
                to,
                stage,
                version,
                digest,
                update,
            },
            Key::File(id) => Message::DocSync {
                doc: DocId::File(id),
                from,
                to,
                stage,
                version,
                digest,
                update,
            },
        }
    }

    fn update_message(&self, key: Key, update: Vec<u8>) -> Message {
        let from = self.peer();
        match key {
            Key::Single => Message::Update { from, update },
            Key::Project => Message::DocUpdate {
                doc: DocId::Project,
                from,
                update,
            },
            Key::File(id) => Message::DocUpdate {
                doc: DocId::File(id),
                from,
                update,
            },
        }
    }

    fn push_request(&mut self, key: Key, out: &mut Vec<Output>) {
        if !self.is_connected() || !self.is_loaded_key(key) {
            return;
        }
        let digest = self.slot_digest(key);
        let version = self.slot_version(key);
        let mut flags = 0;
        if self.is_unsynced(key) {
            flags |= FLAG_UNSYNCED;
        }
        let priority = matches!(key, Key::File(_)) && self.is_priority(key);
        if priority {
            flags |= FLAG_PRIORITY;
        }
        let update = if flags == 0 { Vec::new() } else { vec![flags] };
        let msg = self.sync_message(key, None, SyncStage::Request, digest, version, update);
        let class = if matches!(key, Key::File(_)) && !priority {
            Class::Normal
        } else {
            Class::Interactive
        };
        self.push_data(class, msg, out);
    }

    /// Answer a Request with a Reply if we know something the sender lacks,
    /// do not know the sender yet, or the sender marked itself unsynced.
    fn answer_request(
        &mut self,
        key: Key,
        req: &Req<'_>,
        now: u64,
        targets: &mut dyn TextTargets,
        out: &mut Vec<Output>,
    ) {
        let from = req.from;
        let known = self.presence.iter().any(|p| p.peer == from);
        if self.awaiting_host {
            // Not the room's document yet: answering would mislead.
            if !known {
                self.push_presence(out);
            }
            return;
        }
        self.check_divergence(key, req, now, targets, out);
        let waiting_peer = req.flags & FLAG_UNSYNCED != 0;
        if (req.version != self.slot_version(key).as_slice() || !known || waiting_peer)
            && self.is_loaded_key(key)
        {
            self.push_reply(key, from, req.version, req.flags & FLAG_PRIORITY != 0, out);
        }
        if !known {
            self.push_presence(out);
        }
    }

    fn on_sync(
        &mut self,
        key: Key,
        now: u64,
        m: SyncIn<'_>,
        targets: &mut dyn TextTargets,
        out: &mut Vec<Output>,
    ) {
        let me = self.peer();
        if m.from == me || m.to.is_some_and(|t| t != me) {
            return;
        }
        let mut req = Req {
            from: m.from,
            version: m.version,
            digest: m.digest,
            flags: 0,
        };
        match m.stage {
            SyncStage::Request => {
                req.flags = request_flags(m.update);
                if !self.is_loaded_key(key) {
                    self.defer_request(key, &req, out);
                    return;
                }
                self.answer_request(key, &req, now, targets, out);
            }
            SyncStage::Reply => {
                if !self.is_loaded_key(key) {
                    return;
                }
                if matches!(key, Key::Single | Key::Project) {
                    self.awaiting_host = false;
                }
                self.apply(key, now, m.update, targets, out);
                // Before the divergence check: a refetch installs a fresh,
                // unsynced document that must stay unsynced.
                self.mark_synced(key);
                self.check_divergence(key, &req, now, targets, out);
                if matches!(key, Key::Single | Key::Project)
                    && matches!(self.state, SessionState::Syncing | SessionState::Waiting)
                {
                    self.set_state(SessionState::Live, out);
                }
                if m.version != self.slot_version(key).as_slice() {
                    self.push_final(key, m.from, m.version, out);
                }
            }
            SyncStage::Final => {
                if !self.is_loaded_key(key) {
                    return;
                }
                self.apply(key, now, m.update, targets, out);
                self.mark_synced(key);
                self.check_divergence(key, &req, now, targets, out);
            }
        }
    }

    fn mark_synced(&mut self, key: Key) {
        if let Key::File(id) = key
            && let Some(slot) = self.docs.get_mut(&id)
            && !slot.synced
        {
            slot.synced = true;
            if slot.counted {
                slot.counted = false;
                self.progress_done += 1;
            }
        }
    }

    /// Remember a Request for a file we have not loaded and ask the caller to
    /// load it, if the project tree says it is a text file.
    fn defer_request(&mut self, key: Key, req: &Req<'_>, out: &mut Vec<Output>) {
        let Key::File(id) = key else { return };
        if self.awaiting_host {
            return;
        }
        let is_text = self
            .project_view()
            .and_then(|v| v.get(id).map(|e| e.meta.kind == NodeKind::Text))
            .unwrap_or(false);
        if !is_text {
            self.stats.unknown_doc += 1;
            return;
        }
        if !self.deferred.contains_key(&id) && self.deferred.len() >= MAX_DEFERRED_DOCS {
            self.stats.unknown_doc += 1;
            return;
        }
        let entry = self.deferred.entry(id).or_default();
        if entry.len() >= MAX_DEFERRED_PEERS && !entry.contains_key(&req.from) {
            return;
        }
        entry.insert(
            req.from,
            Deferred {
                version: req.version.to_vec(),
                digest: req.digest,
                flags: req.flags,
            },
        );
        if self.wanted_reported.insert(id) {
            out.push(Output::DocWanted(id));
        }
    }

    fn on_update(
        &mut self,
        key: Key,
        now: u64,
        from: u64,
        update: &[u8],
        targets: &mut dyn TextTargets,
        out: &mut Vec<Output>,
    ) {
        if !self.is_loaded_key(key) {
            // Not interested yet; the state exchange on load brings it.
            if matches!(key, Key::File(_)) {
                self.stats.unknown_doc += 1;
            }
            return;
        }
        let units = self.apply(key, now, update, targets, out);
        if key == Key::Project {
            return;
        }
        let (name, email) = self.presence.iter().find(|p| p.peer == from).map_or_else(
            || (format!("peer {from}"), None),
            |p| (p.name.clone(), p.email.clone()),
        );
        let name = if name.is_empty() {
            format!("peer {from}")
        } else {
            name
        };
        self.contributions
            .record(from, &name, email.as_deref(), units);
    }

    /// Apply a remote update; returns how many characters it changed.
    fn apply(
        &mut self,
        key: Key,
        now: u64,
        update: &[u8],
        targets: &mut dyn TextTargets,
        out: &mut Vec<Output>,
    ) -> usize {
        if update.is_empty() {
            return 0;
        }
        match key {
            Key::Project => {
                self.apply_project(now, update, out);
                0
            }
            Key::Single => {
                let result = apply_to_slot(&mut self.single, update, targets.single());
                self.finish_text(key, now, result, out)
            }
            Key::File(id) => {
                let target = targets.target(id);
                let Some(slot) = self.docs.get_mut(&id) else {
                    return 0;
                };
                let result = apply_to_slot(slot, update, target);
                self.finish_text(key, now, result, out)
            }
        }
    }

    fn finish_text(
        &mut self,
        key: Key,
        now: u64,
        result: Result<TextOutcome, LiveError>,
        out: &mut Vec<Output>,
    ) -> usize {
        let doc = match key {
            Key::File(id) => Some(id),
            Key::Single | Key::Project => None,
        };
        match result {
            Ok(TextOutcome::Applied(mut applied)) => {
                applied.doc = doc;
                let pending = applied.pending;
                let units: usize = applied
                    .changes
                    .iter()
                    .map(|c| c.char_delete + c.insert.chars().count())
                    .sum();
                if !applied.changes.is_empty() {
                    out.push(Output::Remote(applied));
                }
                if pending {
                    self.request_for_pending(key, now, out);
                }
                units
            }
            Ok(TextOutcome::Detached {
                changed,
                pending,
                units,
            }) => {
                if changed && let Some(id) = doc {
                    out.push(Output::DocChanged(id));
                }
                if pending {
                    self.request_for_pending(key, now, out);
                }
                units
            }
            Err(e) => {
                self.reject(out, e.to_string());
                0
            }
        }
    }

    fn apply_project(&mut self, now: u64, update: &[u8], out: &mut Vec<Output>) {
        let Some(p) = self.project.as_mut() else {
            return;
        };
        match p.doc.apply_update(update) {
            Ok((changes, pending)) => {
                if !changes.is_empty() {
                    out.push(Output::Tree(changes));
                }
                if pending {
                    self.request_for_pending(Key::Project, now, out);
                }
            }
            Err(e) => self.reject(out, e.to_string()),
        }
    }

    /// An update needs operations we lack: ask for a resync, not too often.
    fn request_for_pending(&mut self, key: Key, now: u64, out: &mut Vec<Output>) {
        let gap = self.config.pending_request_gap_ms;
        let last = match key {
            Key::Single => &mut self.single.last_pending_request,
            Key::Project => match self.project.as_mut() {
                Some(p) => &mut p.last_pending_request,
                None => return,
            },
            Key::File(id) => match self.docs.get_mut(&id) {
                Some(s) => &mut s.last_pending_request,
                None => return,
            },
        };
        if last.is_none_or(|t| now >= t + gap) {
            *last = Some(now);
            self.push_request(key, out);
        }
    }

    fn check_divergence(
        &mut self,
        key: Key,
        req: &Req<'_>,
        now: u64,
        targets: &mut dyn TextTargets,
        out: &mut Vec<Output>,
    ) {
        if key == Key::Project
            || req.version != self.slot_version(key).as_slice()
            || req.digest == self.slot_digest(key)
        {
            return;
        }
        self.stats.divergences += 1;
        match key {
            Key::Single => out.push(Output::Diverged { peer: req.from }),
            Key::File(id) => self.diverged_file(id, req.from, now, targets, out),
            Key::Project => {}
        }
    }

    /// Same version, different text in one file. A guest drops its copy and
    /// fetches that file again (the host's wins); a host only reports.
    fn diverged_file(
        &mut self,
        id: FileId,
        from: u64,
        now: u64,
        targets: &mut dyn TextTargets,
        out: &mut Vec<Output>,
    ) {
        let guest = self.config.await_host;
        let recent = self
            .docs
            .get(&id)
            .and_then(|s| s.last_reset)
            .is_some_and(|t| now < t + REFETCH_GAP_MS);
        if !guest || recent {
            out.push(Output::DocDiverged {
                doc: id,
                peer: from,
                refetching: false,
            });
            return;
        }
        let fresh = match self.fresh_doc() {
            Ok(f) => f,
            Err(e) => {
                self.reject(out, e.to_string());
                return;
            }
        };
        out.push(Output::DocDiverged {
            doc: id,
            peer: from,
            refetching: true,
        });
        // Empty the caller's buffer now so old and fetched text never mix.
        match targets.target(id) {
            Some(t) => {
                if let Some(change) = fresh.resync_target(t) {
                    out.push(Output::Remote(RemoteApplied {
                        doc: Some(id),
                        changes: vec![change],
                        pending: false,
                    }));
                }
            }
            None => out.push(Output::DocChanged(id)),
        }
        let mut slot = FileSlot::new(fresh, false, false);
        slot.last_reset = Some(now);
        self.docs.insert(id, slot);
        self.push_request(Key::File(id), out);
    }

    /// An empty document under a fresh peer id (the old id's operations may
    /// already be known to others, so it must not be reused).
    fn fresh_doc(&mut self) -> Result<LiveDoc<E>, LiveError> {
        let mut bytes = [0u8; 8];
        self.entropy.fill(&mut bytes)?;
        let mut peer = u64::from_le_bytes(bytes) & ((1u64 << 53) - 1);
        if peer <= SEED_PEER {
            peer += 2;
        }
        LiveDoc::empty(peer)
    }

    fn push_reply(
        &mut self,
        key: Key,
        to: u64,
        their_version: &[u8],
        priority: bool,
        out: &mut Vec<Output>,
    ) {
        self.push_answer(key, to, SyncStage::Reply, their_version, priority, out);
    }

    fn push_final(&mut self, key: Key, to: u64, their_version: &[u8], out: &mut Vec<Output>) {
        let priority = self.is_priority(key);
        self.push_answer(key, to, SyncStage::Final, their_version, priority, out);
    }

    fn push_answer(
        &mut self,
        key: Key,
        to: u64,
        stage: SyncStage,
        their_version: &[u8],
        priority: bool,
        out: &mut Vec<Output>,
    ) {
        match self.slot_export_since(key, their_version) {
            Ok(update) => {
                let digest = self.slot_digest(key);
                let version = self.slot_version(key);
                let msg = self.sync_message(key, Some(to), stage, digest, version, update);
                let class = match key {
                    Key::Single | Key::Project => Class::Interactive,
                    Key::File(_) if priority || self.is_priority(key) => Class::Priority,
                    Key::File(_) => Class::Normal,
                };
                self.push_data(class, msg, out);
            }
            Err(e) => self.reject(out, e.to_string()),
        }
    }

    // -- manifest ------------------------------------------------------------

    /// Guests: ask the room for the manifest again (after a failed attempt,
    /// or to refresh it). The answer arrives as [`Output::Manifest`].
    pub fn request_manifest(&mut self, now_ms: u64) -> Vec<Output> {
        let mut out = Vec::new();
        if self.project.is_none() || self.state == SessionState::Closed {
            return out;
        }
        self.now = now_ms;
        self.manifest_wanted = true;
        self.manifest_asm.clear();
        if self.is_connected() {
            self.push_manifest_get(&mut out);
        }
        self.end(&mut out);
        out
    }

    fn push_manifest_get(&mut self, out: &mut Vec<Output>) {
        let msg = Message::ManifestGet {
            from: self.peer(),
            to: None,
        };
        self.push_data(Class::Interactive, msg, out);
    }

    fn on_manifest_get(&mut self, now: u64, from: u64, out: &mut Vec<Output>) {
        if self.awaiting_host || !self.is_connected() {
            return;
        }
        let gap = self.config.manifest_serve_gap_ms;
        if self
            .manifest_served
            .get(&from)
            .is_some_and(|t| now < t + gap)
        {
            return;
        }
        if self.manifest_served.len() > 256 {
            self.manifest_served.clear();
        }
        self.manifest_served.insert(from, now);
        let Some(view) = self.project_view() else {
            return;
        };
        let entries: Vec<ManifestEntry> = view
            .manifest()
            .into_iter()
            .filter(|e| e.validate().is_ok())
            .collect();
        let root = manifest_root(&entries);
        let pages = paginate(&entries);
        let Ok(total) = u32::try_from(pages.len()) else {
            return;
        };
        for (i, page_entries) in pages.into_iter().enumerate() {
            let Ok(page) = u32::try_from(i) else { return };
            let msg = Message::Manifest {
                from: self.peer(),
                to: Some(from),
                root,
                page,
                pages: total,
                entries: page_entries,
            };
            self.push_data(Class::Priority, msg, out);
        }
    }

    fn on_manifest(
        &mut self,
        from: u64,
        root: [u8; 16],
        page: u32,
        pages: u32,
        entries: Vec<ManifestEntry>,
        out: &mut Vec<Output>,
    ) {
        if !self.manifest_wanted {
            return;
        }
        if !self.manifest_asm.contains_key(&from) {
            if self.manifest_asm.len() >= MAX_MANIFEST_SENDERS {
                return;
            }
            match ManifestAssembler::new(root, pages) {
                Ok(a) => {
                    self.manifest_asm.insert(from, a);
                }
                Err(e) => {
                    self.reject(out, e.to_string());
                    return;
                }
            }
        }
        let Some(asm) = self.manifest_asm.get_mut(&from) else {
            return;
        };
        match asm.accept(root, page, pages, entries) {
            Ok(None) => {}
            Ok(Some(all)) => {
                self.manifest_wanted = false;
                self.manifest_asm.clear();
                self.awaiting_host = false;
                if matches!(self.state, SessionState::Syncing | SessionState::Waiting) {
                    self.set_state(SessionState::Live, out);
                }
                out.push(Output::Manifest {
                    from,
                    root,
                    entries: all,
                });
            }
            Err(e) => {
                self.manifest_asm.remove(&from);
                self.reject(out, e.to_string());
            }
        }
    }

    // -- blobs ---------------------------------------------------------------

    /// Project sessions: fetch the blob `hash` of `size` bytes (both from the
    /// verified manifest). One blob is fetched at a time; the rest wait their
    /// turn. The result arrives as [`Output::BlobReceived`] or
    /// [`Output::BlobFailed`]. Counts toward [`Output::Progress`].
    pub fn want_blob(
        &mut self,
        now_ms: u64,
        hash: BlobHash,
        size: u64,
    ) -> Result<Vec<Output>, LiveError> {
        if size > MAX_BLOB_BYTES {
            return Err(LiveError::Blob("blob exceeds the size cap"));
        }
        chunk_count(size)?;
        let mut out = Vec::new();
        if self.project.is_none() || self.state == SessionState::Closed {
            return Ok(out);
        }
        self.now = now_ms;
        let queued = self.blobs.queue.iter().any(|(h, _)| *h == hash)
            || self.blobs.active.as_ref().is_some_and(|a| a.hash == hash);
        if !queued {
            self.blobs.queue.push_back((hash, size));
            self.progress_total += 1;
            self.pump_blob(&mut out);
        }
        self.end(&mut out);
        Ok(out)
    }

    /// Project sessions: make the content of a blob available for peers to
    /// fetch. Verified against `hash`; kept in memory for a while and dropped
    /// when idle. Answers requests that were waiting for it.
    pub fn offer_blob(&mut self, now_ms: u64, hash: BlobHash, data: Vec<u8>) -> Vec<Output> {
        let mut out = Vec::new();
        if self.project.is_none() || self.state == SessionState::Closed {
            return out;
        }
        self.now = now_ms;
        if data.len() as u64 > MAX_BLOB_BYTES || blob_hash(&data) != hash {
            self.stats.blob_rejected += 1;
            out.push(Output::Rejected(
                "offered blob does not match its hash".into(),
            ));
            self.end(&mut out);
            return out;
        }
        self.blobs.asked.remove(&hash);
        if let Some(old) = self.blobs.offered.remove(&hash) {
            self.blobs.offered_bytes = self.blobs.offered_bytes.saturating_sub(old.data.len());
        }
        self.blobs.offered_bytes += data.len();
        self.blobs.offered.insert(
            hash,
            Offered {
                data: Arc::new(data),
                last_used: now_ms,
            },
        );
        self.evict_offered(Some(hash));
        let waiting: Vec<(u64, BlobHash, u32)> = {
            let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.blobs.deferred_gets)
                .into_iter()
                .partition(|(_, h, _)| *h == hash);
            self.blobs.deferred_gets = rest;
            mine
        };
        for (peer, h, chunk) in waiting {
            self.serve_chunk(now_ms, peer, h, chunk, &mut out);
        }
        self.end(&mut out);
        out
    }

    /// Project sessions: the caller cannot supply a blob a peer asked for.
    pub fn decline_blob(&mut self, hash: BlobHash) {
        self.blobs.asked.remove(&hash);
        self.blobs.deferred_gets.retain(|(_, h, _)| *h != hash);
    }

    fn evict_offered(&mut self, keep: Option<BlobHash>) {
        while self.blobs.offered_bytes > self.config.offered_blob_max_bytes {
            let oldest = self
                .blobs
                .offered
                .iter()
                .filter(|(h, _)| Some(**h) != keep)
                .min_by_key(|(_, o)| o.last_used)
                .map(|(h, _)| *h);
            let Some(h) = oldest else { break };
            if let Some(o) = self.blobs.offered.remove(&h) {
                self.blobs.offered_bytes = self.blobs.offered_bytes.saturating_sub(o.data.len());
            }
        }
    }

    fn on_blob_get(
        &mut self,
        now: u64,
        from: u64,
        hash: BlobHash,
        chunk: u32,
        out: &mut Vec<Output>,
    ) {
        if self.awaiting_host {
            return;
        }
        if let Some((current, last)) = self.blobs.serving.get(&from)
            && *current != hash
            && now < last + BLOB_SWITCH_IDLE_MS
        {
            self.stats.blob_busy += 1;
            out.push(Output::Rejected(
                "peer already has another blob in flight".into(),
            ));
            return;
        }
        if self.blobs.serving.len() > 256 {
            self.blobs.serving.clear();
        }
        self.blobs.serving.insert(from, (hash, now));
        if self.blobs.offered.contains_key(&hash) {
            self.serve_chunk(now, from, hash, chunk, out);
            return;
        }
        if self.blobs.deferred_gets.len() < MAX_DEFERRED_GETS
            && !self
                .blobs
                .deferred_gets
                .iter()
                .any(|(p, h, c)| *p == from && *h == hash && *c == chunk)
        {
            self.blobs.deferred_gets.push((from, hash, chunk));
        }
        if self.blobs.asked.len() < MAX_DEFERRED_GETS && self.blobs.asked.insert(hash) {
            out.push(Output::BlobRequested { peer: from, hash });
        }
    }

    fn serve_chunk(
        &mut self,
        now: u64,
        to: u64,
        hash: BlobHash,
        chunk: u32,
        out: &mut Vec<Output>,
    ) {
        let Some(offered) = self.blobs.offered.get_mut(&hash) else {
            return;
        };
        offered.last_used = now;
        let data = Arc::clone(&offered.data);
        let size = data.len() as u64;
        let (Ok(range), Ok(count)) = (chunk_range(size, chunk), chunk_count(size)) else {
            self.stats.blob_rejected += 1;
            out.push(Output::Rejected("blob chunk request out of range".into()));
            return;
        };
        let Some(bytes) = data.get(range) else {
            return;
        };
        let msg = Message::BlobChunk {
            from: self.peer(),
            to: Some(to),
            hash,
            chunk,
            count,
            data: bytes.to_vec(),
        };
        self.push_data(Class::Bulk, msg, out);
    }

    fn pump_blob(&mut self, out: &mut Vec<Output>) {
        if self.blobs.active.is_none()
            && let Some((hash, size)) = self.blobs.queue.pop_front()
        {
            match BlobAssembler::new(hash, size) {
                Ok(asm) => {
                    self.blobs.active = Some(ActiveBlob {
                        asm,
                        hash,
                        requested_at: self.now,
                        tries: 0,
                        mismatches: 0,
                    });
                }
                Err(e) => {
                    self.reject(out, e.to_string());
                    self.progress_done += 1;
                    out.push(Output::BlobFailed { hash });
                    return self.pump_blob(out);
                }
            }
        }
        self.request_next_chunk(out);
    }

    fn request_next_chunk(&mut self, out: &mut Vec<Output>) {
        if !self.is_connected() || self.awaiting_host {
            return;
        }
        let Some(active) = &mut self.blobs.active else {
            return;
        };
        let Some(chunk) = active.asm.missing().next() else {
            return;
        };
        active.requested_at = self.now;
        let hash = active.hash;
        let msg = Message::BlobGet {
            from: self.peer(),
            to: None,
            hash,
            chunk,
        };
        self.push_data(Class::Interactive, msg, out);
    }

    fn blob_tick(&mut self, now: u64, out: &mut Vec<Output>) {
        // Re-ask for a chunk that did not arrive.
        let retry = self.config.blob_retry_ms;
        let mut failed = None;
        if let Some(active) = &mut self.blobs.active
            && now >= active.requested_at + retry
        {
            active.tries += 1;
            if active.tries >= BLOB_MAX_TRIES {
                failed = Some(active.hash);
            }
        }
        if let Some(hash) = failed {
            self.fail_active_blob(hash, out);
        } else if self
            .blobs
            .active
            .as_ref()
            .is_some_and(|a| now >= a.requested_at + retry)
        {
            self.request_next_chunk(out);
        }
        // Drop offered blobs nobody has asked for lately.
        let ttl = self.config.offered_blob_ttl_ms;
        let stale: Vec<BlobHash> = self
            .blobs
            .offered
            .iter()
            .filter(|(_, o)| now >= o.last_used + ttl)
            .map(|(h, _)| *h)
            .collect();
        for h in stale {
            if let Some(o) = self.blobs.offered.remove(&h) {
                self.blobs.offered_bytes = self.blobs.offered_bytes.saturating_sub(o.data.len());
            }
        }
        self.blobs
            .serving
            .retain(|_, (_, last)| now < *last + BLOB_SWITCH_IDLE_MS * 6);
    }

    fn fail_active_blob(&mut self, hash: BlobHash, out: &mut Vec<Output>) {
        self.blobs.active = None;
        self.progress_done += 1;
        out.push(Output::BlobFailed { hash });
        self.pump_blob(out);
    }

    fn on_blob_chunk(
        &mut self,
        hash: BlobHash,
        chunk: u32,
        count: u32,
        data: &[u8],
        out: &mut Vec<Output>,
    ) {
        enum Got {
            Unsolicited,
            Bad(String),
            More,
            Done(Vec<u8>),
            Mismatch(String, u32),
        }
        let got = match self.blobs.active.as_mut() {
            Some(a) if a.hash == hash => match a.asm.accept(chunk, count, data) {
                Err(e) => Got::Bad(e.to_string()),
                Ok(false) => {
                    a.tries = 0;
                    Got::More
                }
                Ok(true) => match a.asm.finish() {
                    Ok(content) => Got::Done(content),
                    Err(e) => {
                        a.mismatches += 1;
                        Got::Mismatch(e.to_string(), a.mismatches)
                    }
                },
            },
            _ => Got::Unsolicited,
        };
        match got {
            Got::Unsolicited => {
                self.stats.blob_unsolicited += 1;
                out.push(Output::Rejected("unsolicited blob chunk".into()));
            }
            Got::Bad(why) => {
                self.stats.blob_rejected += 1;
                out.push(Output::Rejected(why));
            }
            Got::More => self.request_next_chunk(out),
            Got::Done(content) => {
                self.blobs.active = None;
                self.progress_done += 1;
                out.push(Output::BlobReceived {
                    hash,
                    data: content,
                });
                self.pump_blob(out);
            }
            Got::Mismatch(why, mismatches) => {
                // finish() discarded the chunks: start the blob again, but
                // not forever.
                self.stats.blob_rejected += 1;
                out.push(Output::Rejected(why));
                if mismatches >= 3 {
                    self.fail_active_blob(hash, out);
                } else {
                    self.request_next_chunk(out);
                }
            }
        }
    }

    // -- sending -------------------------------------------------------------

    fn push_data(&mut self, class: Class, msg: Message, out: &mut Vec<Output>) {
        match msg.encode() {
            Ok(plain) => self.push_sealed(class, Kind::Data, &plain, out),
            Err(e) => self.reject(out, e.to_string()),
        }
    }

    fn push_presence(&mut self, out: &mut Vec<Output>) {
        self.presence_seq += 1;
        let presence = Presence {
            peer: self.peer(),
            seq: self.presence_seq,
            name: self.config.name.clone(),
            slot: self.config.slot,
            email: self.config.email.clone(),
            selection: self.local_selection.clone(),
        };
        let msg = if self.project.is_some() {
            PresenceMessage::StateV2(PresenceV2 {
                presence,
                doc: self.focus_doc,
                open: self.focus_open.clone(),
            })
        } else {
            PresenceMessage::State(presence)
        };
        match msg.encode() {
            Ok(plain) => self.push_sealed(Class::Interactive, Kind::Presence, &plain, out),
            Err(e) => self.reject(out, e.to_string()),
        }
    }

    fn push_sealed(&mut self, class: Class, kind: Kind, plain: &[u8], out: &mut Vec<Output>) {
        match self.keys.seal(kind, plain, &mut self.entropy) {
            Ok(frame) => self.emit(class, frame, out),
            Err(e) => self.reject(out, e.to_string()),
        }
    }

    /// Send a sealed frame now, or queue it behind the pacer (project
    /// sessions only).
    fn emit(&mut self, class: Class, frame: Vec<u8>, out: &mut Vec<Output>) {
        if self.project.is_none() {
            out.push(Output::Send(frame));
        } else if self.pacer.bypasses(class, frame.len()) {
            self.pacer.charge(frame.len());
            out.push(Output::Send(frame));
        } else {
            self.pacer.push(class, frame);
        }
    }
}

/// A decoded sync message.
struct SyncIn<'a> {
    from: u64,
    to: Option<u64>,
    stage: SyncStage,
    version: &'a [u8],
    digest: u64,
    update: &'a [u8],
}

/// What a peer told us about its replica of a document.
struct Req<'a> {
    from: u64,
    version: &'a [u8],
    digest: u64,
    flags: u8,
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
