//! Outbound pacing for project sessions.
//!
//! A project's first sync is hundreds of small exchanges plus a few large
//! ones, and the relays cap a connection at 200 messages/s, 4 MiB/s and one
//! 16 MiB frame. The [`Pacer`] sits between the session and the transport: a
//! token bucket for bytes and one for messages, and four priority classes so
//! that what the user is looking at (open files, the project tree, live
//! keystrokes) is never queued behind bulk traffic. It is sans-IO: time is
//! passed in, frames come out when they are due.
//!
//! Small interactive frames bypass the buckets (but are charged to them), so
//! typing never waits; everything else leaves in class order, then FIFO.

use std::collections::VecDeque;

/// Largest interactive frame that skips the queue.
pub const INTERACTIVE_BYPASS_BYTES: usize = 16 * 1024;

/// Priority of an outbound frame; lower is more urgent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Class {
    /// Live edits, requests, the project tree, files the user has open.
    Interactive = 0,
    /// Manifest pages and answers about files the peer asked for first.
    Priority = 1,
    /// Everything else about documents (background anti-entropy, first sync
    /// of files nobody looks at).
    Normal = 2,
    /// Blob chunks.
    Bulk = 3,
}

const CLASSES: usize = 4;

/// Pacing limits. A rate of zero disables pacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaceConfig {
    /// Sustained bytes per second.
    pub bytes_per_sec: u64,
    /// Bytes that may go out at once after a quiet period.
    pub burst_bytes: u64,
    /// Sustained messages per second.
    pub msgs_per_sec: u64,
    /// Messages that may go out at once after a quiet period.
    pub burst_msgs: u64,
}

impl Default for PaceConfig {
    fn default() -> Self {
        Self {
            bytes_per_sec: 2 * 1024 * 1024,
            burst_bytes: 1024 * 1024,
            msgs_per_sec: 100,
            burst_msgs: 20,
        }
    }
}

/// The paced outbound queue.
#[derive(Debug)]
pub struct Pacer {
    cfg: PaceConfig,
    queues: [VecDeque<Vec<u8>>; CLASSES],
    byte_tokens: i64,
    /// Message tokens in thousandths (one message costs 1000).
    msg_tokens: i64,
    last_ms: Option<u64>,
    queued_bytes: usize,
}

fn clamp_i64(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(i64::MAX)
}

impl Pacer {
    /// A pacer holding a full burst of tokens.
    #[must_use]
    pub fn new(cfg: PaceConfig) -> Self {
        Self {
            cfg,
            queues: Default::default(),
            byte_tokens: clamp_i64(cfg.burst_bytes),
            msg_tokens: clamp_i64(cfg.burst_msgs.saturating_mul(1000)),
            last_ms: None,
            queued_bytes: 0,
        }
    }

    fn unlimited(&self) -> bool {
        self.cfg.bytes_per_sec == 0
    }

    /// Account for a frame sent outside the queue.
    pub fn charge(&mut self, bytes: usize) {
        self.byte_tokens = self.byte_tokens.saturating_sub(clamp_i64(bytes as u64));
        self.msg_tokens = self.msg_tokens.saturating_sub(1000);
    }

    /// Whether `frame` of `class` may skip the queue.
    #[must_use]
    pub fn bypasses(&self, class: Class, len: usize) -> bool {
        self.unlimited() || (class == Class::Interactive && len <= INTERACTIVE_BYPASS_BYTES)
    }

    /// Queue a frame.
    pub fn push(&mut self, class: Class, frame: Vec<u8>) {
        self.queued_bytes += frame.len();
        self.queues[class as usize].push_back(frame);
    }

    fn refill(&mut self, now_ms: u64) {
        let Some(last) = self.last_ms else {
            self.last_ms = Some(now_ms);
            return;
        };
        let elapsed = now_ms.saturating_sub(last);
        self.last_ms = Some(now_ms.max(last));
        let add_bytes = u128::from(elapsed) * u128::from(self.cfg.bytes_per_sec) / 1000;
        let add_msgs = u128::from(elapsed) * u128::from(self.cfg.msgs_per_sec);
        let add_bytes = i64::try_from(add_bytes).unwrap_or(i64::MAX);
        let add_msgs = i64::try_from(add_msgs).unwrap_or(i64::MAX);
        self.byte_tokens = self
            .byte_tokens
            .saturating_add(add_bytes)
            .min(clamp_i64(self.cfg.burst_bytes));
        self.msg_tokens = self
            .msg_tokens
            .saturating_add(add_msgs)
            .min(clamp_i64(self.cfg.burst_msgs.saturating_mul(1000)));
    }

    /// Release the frames due at `now_ms`, most urgent first.
    pub fn drain(&mut self, now_ms: u64, out: &mut Vec<Vec<u8>>) {
        self.refill(now_ms);
        let unlimited = self.unlimited();
        for q in &mut self.queues {
            while !q.is_empty() {
                if !unlimited && (self.byte_tokens <= 0 || self.msg_tokens < 1000) {
                    return;
                }
                let Some(frame) = q.pop_front() else { break };
                self.queued_bytes = self.queued_bytes.saturating_sub(frame.len());
                self.byte_tokens = self
                    .byte_tokens
                    .saturating_sub(clamp_i64(frame.len() as u64));
                self.msg_tokens = self.msg_tokens.saturating_sub(1000);
                out.push(frame);
            }
        }
    }

    /// Drop everything queued and refill the buckets (the connection died:
    /// the exchange on reconnect re-sends what mattered).
    pub fn clear(&mut self) {
        for q in &mut self.queues {
            q.clear();
        }
        self.queued_bytes = 0;
        self.byte_tokens = clamp_i64(self.cfg.burst_bytes);
        self.msg_tokens = clamp_i64(self.cfg.burst_msgs.saturating_mul(1000));
        self.last_ms = None;
    }

    /// Bytes waiting in the queue.
    #[must_use]
    pub fn queued_bytes(&self) -> usize {
        self.queued_bytes
    }

    /// Frames waiting in the queue.
    #[must_use]
    pub fn queued_frames(&self) -> usize {
        self.queues.iter().map(VecDeque::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> PaceConfig {
        PaceConfig {
            bytes_per_sec: 1000,
            burst_bytes: 1000,
            msgs_per_sec: 10,
            burst_msgs: 2,
        }
    }

    #[test]
    fn rate_limits_and_orders_by_class() {
        let mut p = Pacer::new(cfg());
        p.push(Class::Bulk, vec![0; 400]);
        p.push(Class::Normal, vec![1; 400]);
        p.push(Class::Priority, vec![2; 400]);
        let mut out = Vec::new();
        p.drain(0, &mut out);
        // Burst of two messages: the two most urgent leave first.
        assert_eq!(out.iter().map(|f| f[0]).collect::<Vec<_>>(), vec![2, 1]);
        out.clear();
        p.drain(50, &mut out);
        assert!(out.is_empty(), "no token yet");
        p.drain(1_000, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(p.queued_frames(), 0);
        assert_eq!(p.queued_bytes(), 0);
    }

    #[test]
    fn interactive_small_frames_skip_the_queue_but_are_charged() {
        let mut p = Pacer::new(cfg());
        assert!(p.bypasses(Class::Interactive, 10));
        assert!(!p.bypasses(Class::Interactive, INTERACTIVE_BYPASS_BYTES + 1));
        assert!(!p.bypasses(Class::Normal, 10));
        p.charge(900);
        p.charge(900);
        p.push(Class::Normal, vec![9; 10]);
        let mut out = Vec::new();
        p.drain(0, &mut out);
        assert!(out.is_empty(), "in debt");
    }

    #[test]
    fn sustained_throughput_stays_under_the_cap() {
        let mut p = Pacer::new(PaceConfig::default());
        for _ in 0..200 {
            p.push(Class::Bulk, vec![0; 100 * 1024]);
        }
        let mut sent = 0usize;
        let mut out = Vec::new();
        for t in (0..=10_000).step_by(100) {
            out.clear();
            p.drain(t, &mut out);
            sent += out.iter().map(Vec::len).sum::<usize>();
        }
        // 10 s at 2 MiB/s plus the burst and one frame of debt.
        let cap = 10 * 2 * 1024 * 1024 + 1024 * 1024 + 100 * 1024;
        assert!(sent <= cap, "{sent} > {cap}");
        assert!(sent > 10 * 1024 * 1024, "{sent}");
    }

    #[test]
    fn zero_rate_disables_pacing_and_clear_empties() {
        let mut p = Pacer::new(PaceConfig {
            bytes_per_sec: 0,
            ..cfg()
        });
        for _ in 0..50 {
            p.push(Class::Bulk, vec![0; 10]);
        }
        let mut out = Vec::new();
        p.drain(0, &mut out);
        assert_eq!(out.len(), 50);
        p.push(Class::Bulk, vec![0; 10]);
        p.clear();
        assert_eq!(p.queued_frames(), 0);
    }
}
