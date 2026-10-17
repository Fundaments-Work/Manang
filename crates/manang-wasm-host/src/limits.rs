use std::time::Duration;

/// Resource ceilings applied to every plugin call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum linear memory a call may grow to.
    pub memory_bytes: usize,
    /// Wall-clock budget per call, enforced by epoch interruption.
    pub deadline: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_bytes: 64 * 1024 * 1024,
            deadline: Duration::from_millis(250),
        }
    }
}
