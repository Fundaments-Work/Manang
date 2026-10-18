//! The transport seam.
//!
//! A [`SyncTransport`] moves opaque, already-encrypted frames. It is
//! deliberately synchronous and poll-based: the core never blocks and never
//! depends on an async runtime, so the same session runs on a desktop thread,
//! in a browser (where the implementation wraps a `WebSocket`), or over a
//! direct peer connection later. Implementations own their I/O tasks and hand
//! events across through a queue.

use crate::error::LiveError;

/// Something that happened on the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportEvent {
    /// The connection is open (initially or after a reconnect).
    Up,
    /// The connection was lost or closed.
    Down,
    /// One frame from the room.
    Frame(Vec<u8>),
}

/// A frame pipe to a room.
pub trait SyncTransport {
    /// Open (or reopen) the connection. Completion is reported later as
    /// [`TransportEvent::Up`]; failure as [`TransportEvent::Down`].
    fn connect(&mut self) -> Result<(), LiveError>;

    /// Queue one frame for the room. Frames sent while down are dropped: the
    /// resync on reconnect recovers whatever they carried.
    fn send(&mut self, frame: Vec<u8>) -> Result<(), LiveError>;

    /// Next pending event, without blocking.
    fn poll(&mut self) -> Option<TransportEvent>;

    /// Close the connection for good.
    fn close(&mut self);
}
