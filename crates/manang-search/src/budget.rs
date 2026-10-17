//! Shared cooperative-stop state for one running search.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use serde::Serialize;

/// Why a search ended early.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Limit,
    Time,
    Bytes,
    Cancelled,
}

const NONE: u8 = 0;

impl StopReason {
    fn code(self) -> u8 {
        match self {
            Self::Limit => 1,
            Self::Time => 2,
            Self::Bytes => 3,
            Self::Cancelled => 4,
        }
    }

    fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Limit),
            2 => Some(Self::Time),
            3 => Some(Self::Bytes),
            4 => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// Counters and stop flags shared by every worker of one search.
pub struct Budget {
    cancel: Arc<AtomicBool>,
    deadline: Instant,
    reason: AtomicU8,
    max_results: usize,
    max_bytes: u64,
    results: AtomicUsize,
    bytes: AtomicU64,
}

impl Budget {
    pub fn new(
        cancel: Arc<AtomicBool>,
        deadline: Instant,
        max_results: usize,
        max_bytes: u64,
    ) -> Self {
        Self {
            cancel,
            deadline,
            reason: AtomicU8::new(NONE),
            max_results,
            max_bytes,
            results: AtomicUsize::new(0),
            bytes: AtomicU64::new(0),
        }
    }

    fn stop(&self, why: StopReason) {
        // First reason wins.
        let _ = self
            .reason
            .compare_exchange(NONE, why.code(), Ordering::AcqRel, Ordering::Acquire);
    }

    /// True once the search must wind down. Cheap enough to call per line.
    pub fn should_stop(&self) -> bool {
        if self.reason.load(Ordering::Acquire) != NONE {
            return true;
        }
        if self.cancel.load(Ordering::Relaxed) {
            self.stop(StopReason::Cancelled);
            return true;
        }
        if Instant::now() >= self.deadline {
            self.stop(StopReason::Time);
            return true;
        }
        false
    }

    /// Claim one result slot; false (and stop) when the cap is reached.
    pub fn claim_result(&self) -> bool {
        if self.results.fetch_add(1, Ordering::AcqRel) >= self.max_results {
            self.stop(StopReason::Limit);
            return false;
        }
        true
    }

    /// Account for `n` bytes about to be read; false (and stop) over budget.
    pub fn claim_bytes(&self, n: u64) -> bool {
        if self.bytes.fetch_add(n, Ordering::AcqRel).saturating_add(n) > self.max_bytes {
            self.stop(StopReason::Bytes);
            return false;
        }
        true
    }

    pub fn reason(&self) -> Option<StopReason> {
        StopReason::from_code(self.reason.load(Ordering::Acquire))
    }

    pub fn bytes_scanned(&self) -> u64 {
        self.bytes.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn budget(results: usize, bytes: u64, ms: u64) -> (Budget, Arc<AtomicBool>) {
        let cancel = Arc::new(AtomicBool::new(false));
        let b = Budget::new(
            cancel.clone(),
            Instant::now() + Duration::from_millis(ms),
            results,
            bytes,
        );
        (b, cancel)
    }

    #[test]
    fn result_cap_stops() {
        let (b, _) = budget(2, 100, 10_000);
        assert!(b.claim_result());
        assert!(b.claim_result());
        assert!(!b.claim_result());
        assert_eq!(b.reason(), Some(StopReason::Limit));
        assert!(b.should_stop());
    }

    #[test]
    fn byte_cap_stops() {
        let (b, _) = budget(10, 10, 10_000);
        assert!(b.claim_bytes(6));
        assert!(!b.claim_bytes(6));
        assert_eq!(b.reason(), Some(StopReason::Bytes));
    }

    #[test]
    fn cancel_and_deadline_stop() {
        let (b, c) = budget(10, 10, 10_000);
        c.store(true, Ordering::Relaxed);
        assert!(b.should_stop());
        assert_eq!(b.reason(), Some(StopReason::Cancelled));
        let (b, _) = budget(10, 10, 0);
        assert!(b.should_stop());
        assert_eq!(b.reason(), Some(StopReason::Time));
    }
}
