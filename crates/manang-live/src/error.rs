//! Error type for the live-editing core.

use thiserror::Error;

/// Everything that can go wrong in `manang-live`.
///
/// The variants carry text rather than wrapping the CRDT engines' error types
/// so that the public API does not leak which engine is in use.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum LiveError {
    /// The CRDT engine refused an edit (an index out of range, say).
    #[error("text engine error: {0}")]
    Engine(String),
    /// A CRDT update, snapshot or version blob could not be decoded.
    #[error("malformed document data: {0}")]
    Decode(String),
    /// An edit referred to a position outside the document.
    #[error("range {from}..{to} is outside the document ({len} units)")]
    OutOfRange {
        /// Start of the offending range.
        from: usize,
        /// End of the offending range.
        to: usize,
        /// Length of the document in the unit the range used.
        len: usize,
    },
    /// An envelope was too short, had an unknown version or kind, or failed
    /// authentication (wrong room secret or tampered bytes).
    #[error("envelope rejected: {0}")]
    Envelope(&'static str),
    /// A decrypted protocol message could not be parsed.
    #[error("malformed protocol message: {0}")]
    Protocol(String),
    /// The room secret was not 32 bytes.
    #[error("room secret must be exactly 32 bytes")]
    BadSecret,
    /// The transport reported a failure.
    #[error("transport error: {0}")]
    Transport(String),
    /// A blob chunk, size or hash was rejected (see [`crate::blob`]).
    #[error("blob rejected: {0}")]
    Blob(&'static str),
    /// A project-tree operation or manifest was rejected.
    #[error("project error: {0}")]
    Project(String),
}
