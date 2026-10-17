use std::time::Duration;

/// Resource ceilings applied to every plugin call.
///
/// Every field is a hard cap. A plugin that exceeds one is stopped or refused;
/// the host never grows past them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum linear memory a call may grow to.
    pub memory_bytes: usize,
    /// Wall-clock budget for guest code per call (instantiation included),
    /// enforced by epoch interruption. Time spent inside host I/O (`http`,
    /// `fs`) is not charged against it, but is capped by `io_time`.
    pub deadline: Duration,
    /// Largest payload the host will pass into a plugin.
    pub max_payload_bytes: usize,
    /// Largest payload the host accepts back. Guest memory is bounded by
    /// `memory_bytes`, so this also bounds what a hostile plugin can make the
    /// host copy.
    pub max_output_bytes: usize,
    /// Most elements one table may hold.
    pub table_elements: usize,
    /// Largest single file `fs` will read or write.
    pub max_fs_file_bytes: usize,
    /// Most bytes one call may move through `fs` in total.
    pub max_fs_call_bytes: usize,
    /// Largest `http` response body, and largest request body.
    pub max_http_body_bytes: usize,
    /// Wall-clock cap for a single `http` request, redirects included.
    pub http_timeout: Duration,
    /// Total host I/O time one call may use before further I/O is refused.
    pub io_time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_bytes: 64 * 1024 * 1024,
            deadline: Duration::from_millis(250),
            max_payload_bytes: 16 * 1024 * 1024,
            max_output_bytes: 16 * 1024 * 1024,
            table_elements: 100_000,
            max_fs_file_bytes: 16 * 1024 * 1024,
            max_fs_call_bytes: 64 * 1024 * 1024,
            max_http_body_bytes: 8 * 1024 * 1024,
            http_timeout: Duration::from_secs(10),
            io_time: Duration::from_secs(30),
        }
    }
}

/// Most core instances, memories and tables one component may use.
pub(crate) const MAX_INSTANCES: usize = 16;
pub(crate) const MAX_MEMORIES: usize = 4;
pub(crate) const MAX_TABLES: usize = 8;
/// Longest error text returned to the kernel; longer ones are truncated.
pub(crate) const MAX_MESSAGE_BYTES: usize = 4096;
