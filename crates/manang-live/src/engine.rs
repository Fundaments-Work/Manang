//! The CRDT text engine abstraction.
//!
//! Everything above this trait (the rope bridge, the encrypted envelope, the
//! session machine) is engine-independent, so Loro and yrs can be measured
//! against the same code and the choice reversed without touching callers.
//!
//! The engine owns the CRDT document and nothing else. The editor's rope stays
//! the buffer of record for display; [`crate::bridge`] keeps the two in step.
//!
//! All positions on this trait are UTF-8 byte offsets. That is the unit the
//! rope converts to and from in O(log n), and the only one in which yrs keeps
//! its position lookup cheap (its UTF-16 mode scans the document on every
//! edit; measured at ~260 us on 1 MB). Callers that speak chars (the editor
//! plugin) or UTF-16 (CodeMirror) go through [`crate::bridge`].

use crate::error::LiveError;

/// What unit an engine's remote change report is expressed in.
///
/// Loro reports Unicode scalar values; yrs reports the unit it was configured
/// with (bytes here). The bridge normalises every unit against the rope while
/// applying the changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexUnit {
    /// Unicode scalar values (Rust `char`s, what ropey indexes).
    Chars,
    /// UTF-8 bytes (what the engine trait takes).
    Bytes,
    /// UTF-16 code units (what CodeMirror indexes).
    Utf16,
}

/// One replacement, `start..start+delete` becomes `insert`, in the document as
/// it stands after all previous changes in the same list were applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawChange {
    /// Start of the replaced range.
    pub start: usize,
    /// Length of the replaced range (0 for a pure insertion).
    pub delete: usize,
    /// Replacement text (empty for a pure deletion).
    pub insert: String,
}

/// The outcome of importing a remote update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// Unit of every index in `changes`.
    pub unit: IndexUnit,
    /// Sequentially applicable changes that bring the old text to the new.
    pub changes: Vec<RawChange>,
    /// The update depended on operations this replica has not seen yet; the
    /// session should ask the peers for a resync.
    pub pending: bool,
}

/// A CRDT text document.
///
/// Implementations are synchronous and CPU-bound by design: callers run them
/// on the thread that owns the document, never on a Tokio worker.
pub trait TextCrdt: Sized {
    /// Short engine name for logs and benchmark tables.
    fn name() -> &'static str;

    /// An empty document owned by `peer`. Peer ids must be unique per replica
    /// and below 2^53 (yrs compatibility).
    fn new(peer: u64) -> Result<Self, LiveError>;

    /// A document seeded with `text` as if typed by the well-known seed peer.
    /// Two replicas seeding byte-equal text produce identical operations, so
    /// they merge without duplicating the content.
    fn seeded(peer: u64, text: &str) -> Result<Self, LiveError>;

    /// Restore a replica from [`TextCrdt::export_snapshot`] or
    /// [`TextCrdt::export_trimmed`] output.
    fn from_snapshot(peer: u64, bytes: &[u8]) -> Result<Self, LiveError>;

    /// This replica's peer id.
    fn peer(&self) -> u64;

    /// The full text.
    fn text(&self) -> String;

    /// Length in UTF-8 bytes.
    fn len_bytes(&self) -> usize;

    /// Insert `text` at UTF-8 byte offset `at` (a character boundary).
    fn insert(&mut self, at: usize, text: &str) -> Result<(), LiveError>;

    /// Delete `len` bytes starting at byte offset `at` (character boundaries).
    fn delete(&mut self, at: usize, len: usize) -> Result<(), LiveError>;

    /// Commit pending local edits and return the update blobs to broadcast,
    /// one per local commit since the last call (usually zero or one).
    fn take_local_updates(&mut self) -> Result<Vec<Vec<u8>>, LiveError>;

    /// Encoded version of this replica (version vector or state vector).
    fn version(&self) -> Vec<u8>;

    /// Everything a replica at `version` is missing.
    fn export_since(&self, version: &[u8]) -> Result<Vec<u8>, LiveError>;

    /// Full state including history.
    fn export_snapshot(&self) -> Result<Vec<u8>, LiveError>;

    /// Full state with as much history dropped as the engine allows. A replica
    /// restored from this can no longer sync with peers that are behind it.
    fn export_trimmed(&self) -> Result<Vec<u8>, LiveError>;

    /// Release in-memory bookkeeping that is not needed to keep editing
    /// (change-store caches). Safe to call at any time; the default does
    /// nothing.
    fn compact_memory(&mut self) {}

    /// Import a remote update and report what changed in the text.
    fn apply_update(&mut self, bytes: &[u8]) -> Result<Applied, LiveError>;

    /// A stable position at byte offset `at` that survives concurrent edits.
    fn cursor_at(&self, at: usize) -> Option<Vec<u8>>;

    /// Resolve a stable position back to a byte offset.
    fn cursor_pos(&self, cursor: &[u8]) -> Option<usize>;
}

/// The well-known peer id used to author seed content.
pub const SEED_PEER: u64 = 1;
