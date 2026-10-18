//! Retry policy for the connection phase of a request.
//!
//! Only HTTP statuses that signal a transient condition are retried, and only
//! before any stream data was delivered. 401, 402 and 400 are never retried.
//! A 429 is retried only when the server says when (`retry-after`): without
//! that hint a blind retry usually makes things worse.

use std::time::Duration;

/// How many attempts and how long to wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Total attempts including the first.
    pub max_attempts: u32,
    /// First backoff; doubles each retry (no jitter, so tests are exact).
    pub base_delay: Duration,
    /// Longest wait we accept, from backoff or from `retry-after`. A server
    /// asking for more makes the request fail instead of hanging the UI.
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    /// Never retry.
    pub fn none() -> Self {
        Self {
            max_attempts: 1,
            ..Self::default()
        }
    }

    /// The wait before retry number `retry` (1 for the first retry), or
    /// `None` when the status should not be retried or the wait is too long.
    pub(crate) fn delay_for(
        &self,
        status: u16,
        retry_after: Option<Duration>,
        retry: u32,
    ) -> Option<Duration> {
        let transient = match status {
            429 => retry_after.is_some(),
            500 | 503 | 504 | 529 => true,
            _ => false,
        };
        if !transient {
            return None;
        }
        let wait = match retry_after {
            Some(after) => after,
            None => {
                let shift = retry.saturating_sub(1).min(16);
                self.base_delay
                    .saturating_mul(1u32.checked_shl(shift).unwrap_or(u32::MAX))
            }
        };
        (wait <= self.max_delay).then_some(wait)
    }
}

/// Parses `retry-after` given in whole seconds. HTTP-date values are ignored
/// (treated as absent).
pub(crate) fn parse_retry_after(value: Option<&str>) -> Option<Duration> {
    value
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses() {
        let p = RetryPolicy::default();
        let s = Duration::from_secs;
        assert_eq!(p.delay_for(429, None, 1), None, "429 without retry-after");
        assert_eq!(p.delay_for(429, Some(s(2)), 1), Some(s(2)));
        for status in [500, 503, 504, 529] {
            assert_eq!(
                p.delay_for(status, None, 1),
                Some(Duration::from_millis(500))
            );
            assert_eq!(p.delay_for(status, None, 3), Some(Duration::from_secs(2)));
        }
        for status in [400, 401, 402, 403, 404, 413, 422] {
            assert_eq!(p.delay_for(status, Some(s(1)), 1), None, "{status}");
        }
        assert_eq!(p.delay_for(529, Some(s(31)), 1), None, "wait too long");
    }

    #[test]
    fn retry_after_parsing() {
        assert_eq!(parse_retry_after(Some(" 7 ")), Some(Duration::from_secs(7)));
        assert_eq!(
            parse_retry_after(Some("Wed, 21 Oct 2026 07:28:00 GMT")),
            None
        );
        assert_eq!(parse_retry_after(None), None);
    }
}
