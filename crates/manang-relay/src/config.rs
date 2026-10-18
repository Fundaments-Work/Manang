//! Relay limits.

use std::sync::Arc;
use std::time::Duration;

/// Test instrumentation: called with every frame the relay forwards. The
/// binary never sets it.
pub type FrameTap = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// Tunable limits. Defaults suit a small beta; see the ADR for sizing.
#[derive(Clone)]
pub struct RelayConfig {
    /// Largest accepted frame. A first sync of a large document is one frame.
    pub max_frame_bytes: usize,
    /// Rooms alive at once.
    pub max_rooms: usize,
    /// Peers in one room.
    pub max_peers_per_room: usize,
    /// Connections at once, across all rooms.
    pub max_connections: usize,
    /// Sustained bytes per second one connection may send.
    pub rate_bytes_per_sec: u64,
    /// Sustained messages per second one connection may send.
    pub rate_msgs_per_sec: u64,
    /// Sustained presence messages per second (presence envelopes are
    /// chattier and less important than document data).
    pub presence_msgs_per_sec: u64,
    /// Close a connection that sent nothing (not even a pong) for this long.
    pub idle_timeout: Duration,
    /// How often the relay pings.
    pub ping_interval: Duration,
    /// How long a client may take to finish the WebSocket handshake.
    pub handshake_timeout: Duration,
    /// Frames buffered per receiving connection before it is dropped as too
    /// slow (it will reconnect and resync).
    pub outbound_queue: usize,
    /// Test instrumentation.
    pub tap: Option<FrameTap>,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            max_frame_bytes: 16 * 1024 * 1024,
            max_rooms: 1024,
            max_peers_per_room: 16,
            max_connections: 4096,
            rate_bytes_per_sec: 4 * 1024 * 1024,
            rate_msgs_per_sec: 200,
            presence_msgs_per_sec: 20,
            idle_timeout: Duration::from_secs(90),
            ping_interval: Duration::from_secs(25),
            handshake_timeout: Duration::from_secs(10),
            outbound_queue: 512,
            tap: None,
        }
    }
}
