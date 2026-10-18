//! A small in-process event bus.
//!
//! Plugins [`EventBus::emit`] events on dotted topics (`project.changed`);
//! the shell and other plugins [`EventBus::subscribe`] with a pattern and read
//! them from a bounded queue.
//!
//! Design rules:
//!
//! * **Emitters never block.** Each subscriber owns a bounded queue; when it
//!   is full the event is dropped for that subscriber only and counted (see
//!   [`Subscription::dropped`]). A slow UI can therefore never stall a
//!   plugin or the watcher thread.
//! * **No callbacks.** Delivery is a non-blocking `try_send`, so there is no
//!   user code to re-enter while the bus lock is held.
//! * **Cheap clones.** [`Payload`] binary buffers are `Arc`-shared, so fanning
//!   an event out to many subscribers copies no bulk data.
//!
//! Patterns: an exact topic (`project.changed`), a namespace wildcard
//! (`project.*`, which matches every topic starting with `project.`) or `*`
//! (everything).
//!
//! The queues are `std::sync::mpsc` bounded channels, which keeps the kernel
//! free of extra dependencies.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Weak, mpsc};
use std::time::Duration;

use parking_lot::Mutex;

use crate::payload::Payload;

/// Queue depth used by [`EventBus::subscribe`].
pub const DEFAULT_CAPACITY: usize = 256;

/// One emitted event.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub topic: String,
    pub payload: Payload,
}

#[derive(Debug)]
struct Slot {
    id: u64,
    pattern: String,
    tx: SyncSender<Event>,
    dropped: Arc<AtomicU64>,
}

#[derive(Debug, Default)]
struct Inner {
    next_id: AtomicU64,
    slots: Mutex<Vec<Slot>>,
}

fn matches(pattern: &str, topic: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    match pattern.strip_suffix(".*") {
        Some(prefix) => topic
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.')),
        None => pattern == topic,
    }
}

/// Publish/subscribe hub. Cloning is cheap and yields a handle to the same bus.
#[derive(Debug, Clone, Default)]
pub struct EventBus {
    inner: Arc<Inner>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Subscribe with the default queue depth.
    pub fn subscribe(&self, pattern: &str) -> Subscription {
        self.subscribe_with_capacity(pattern, DEFAULT_CAPACITY)
    }

    /// Subscribe with an explicit queue depth (clamped to at least 1).
    pub fn subscribe_with_capacity(&self, pattern: &str, capacity: usize) -> Subscription {
        let (tx, rx) = mpsc::sync_channel(capacity.max(1));
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let dropped = Arc::new(AtomicU64::new(0));
        self.inner.slots.lock().push(Slot {
            id,
            pattern: pattern.to_owned(),
            tx,
            dropped: Arc::clone(&dropped),
        });
        Subscription {
            id,
            rx,
            dropped,
            bus: Arc::downgrade(&self.inner),
        }
    }

    /// Deliver an event to every matching subscriber without blocking.
    /// Returns how many subscribers accepted it. Subscribers whose receiver
    /// is gone are pruned.
    pub fn emit(&self, topic: &str, payload: Payload) -> usize {
        let mut delivered = 0;
        let mut slots = self.inner.slots.lock();
        slots.retain(|slot| {
            if !matches(&slot.pattern, topic) {
                return true;
            }
            let event = Event {
                topic: topic.to_owned(),
                payload: payload.clone(),
            };
            match slot.tx.try_send(event) {
                Ok(()) => {
                    delivered += 1;
                    true
                }
                Err(TrySendError::Full(_)) => {
                    slot.dropped.fetch_add(1, Ordering::Relaxed);
                    true
                }
                Err(TrySendError::Disconnected(_)) => false,
            }
        });
        delivered
    }

    /// Number of live subscriptions.
    pub fn subscriber_count(&self) -> usize {
        self.inner.slots.lock().len()
    }
}

/// A live subscription. Dropping it unsubscribes.
#[derive(Debug)]
pub struct Subscription {
    id: u64,
    rx: Receiver<Event>,
    dropped: Arc<AtomicU64>,
    bus: Weak<Inner>,
}

impl Subscription {
    /// Next event if one is queued.
    pub fn try_recv(&self) -> Option<Event> {
        self.rx.try_recv().ok()
    }

    /// Block until an event arrives or the timeout elapses.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Event> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// Block until an event arrives. `None` once the bus is gone.
    pub fn recv(&self) -> Option<Event> {
        self.rx.recv().ok()
    }

    /// Drain everything currently queued.
    pub fn drain(&self) -> Vec<Event> {
        std::iter::from_fn(|| self.try_recv()).collect()
    }

    /// True once every handle to the bus is dropped: no event can arrive any
    /// more (after the queue is drained).
    pub fn is_closed(&self) -> bool {
        self.bus.strong_count() == 0
    }

    /// Events discarded because this subscriber's queue was full.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(inner) = self.bus.upgrade() {
            inner.slots.lock().retain(|s| s.id != self.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn num(n: u64) -> Payload {
        Payload::Json(json!(n))
    }

    #[test]
    fn pattern_matching() {
        assert!(matches("*", "a.b"));
        assert!(matches("project.*", "project.changed"));
        assert!(matches("project.*", "project.a.b"));
        assert!(!matches("project.*", "project"));
        assert!(!matches("project.*", "projects.changed"));
        assert!(matches("a.b", "a.b"));
        assert!(!matches("a.b", "a.bc"));
    }

    #[test]
    fn delivers_to_matching_subscribers_only() {
        let bus = EventBus::new();
        let all = bus.subscribe("*");
        let proj = bus.subscribe("project.*");
        let other = bus.subscribe("editor.saved");
        assert_eq!(bus.emit("project.changed", num(1)), 2);
        assert_eq!(all.drain().len(), 1);
        let got = proj.try_recv();
        assert_eq!(got.map(|e| e.topic), Some("project.changed".to_owned()));
        assert!(other.try_recv().is_none());
    }

    #[test]
    fn full_queue_drops_without_blocking() {
        let bus = EventBus::new();
        let sub = bus.subscribe_with_capacity("t", 2);
        for n in 0..5 {
            bus.emit("t", num(n));
        }
        assert_eq!(sub.dropped(), 3);
        let kept: Vec<_> = sub.drain().into_iter().map(|e| e.payload).collect();
        assert_eq!(kept, vec![num(0), num(1)]);
    }

    #[test]
    fn dropping_a_subscription_unsubscribes() {
        let bus = EventBus::new();
        let sub = bus.subscribe("t");
        assert_eq!(bus.subscriber_count(), 1);
        drop(sub);
        assert_eq!(bus.subscriber_count(), 0);
        assert_eq!(bus.emit("t", Payload::Empty), 0);
    }

    #[test]
    fn subscription_reports_a_dropped_bus() {
        let bus = EventBus::new();
        let sub = bus.subscribe("t");
        let other = bus.clone();
        drop(bus);
        assert!(!sub.is_closed());
        drop(other);
        assert!(sub.is_closed());
        assert!(sub.recv().is_none());
    }

    #[test]
    fn clones_share_one_bus_and_work_across_threads() {
        let bus = EventBus::new();
        let sub = bus.subscribe("t");
        let emitter = bus.clone();
        let handle = std::thread::spawn(move || emitter.emit("t", num(7)));
        assert_eq!(handle.join().unwrap_or(0), 1);
        assert_eq!(
            sub.recv_timeout(Duration::from_secs(1)).map(|e| e.payload),
            Some(num(7))
        );
    }
}
