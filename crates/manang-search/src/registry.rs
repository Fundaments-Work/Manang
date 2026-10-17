//! Search ids and cooperative cancellation.
//!
//! Every text search or replace preview takes a monotonically increasing id.
//! Starting a new one cancels every older search still running, so typing in
//! the search box never piles up work; `search.cancel` cancels one id (or all
//! of them) explicitly.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use parking_lot::Mutex;

#[derive(Default)]
struct Inner {
    next: AtomicU64,
    active: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

/// Tracks in-flight searches.
#[derive(Clone, Default)]
pub struct Registry {
    inner: Arc<Inner>,
}

/// Held for the lifetime of one search; deregisters on drop.
pub struct Ticket {
    pub id: u64,
    pub cancel: Arc<AtomicBool>,
    inner: Arc<Inner>,
}

impl Drop for Ticket {
    fn drop(&mut self) {
        self.inner.active.lock().remove(&self.id);
    }
}

impl Registry {
    /// Start a search: cancels all older ones and returns a fresh ticket.
    pub fn begin(&self) -> Ticket {
        let mut active = self.inner.active.lock();
        for flag in active.values() {
            flag.store(true, Ordering::Relaxed);
        }
        let id = self.inner.next.fetch_add(1, Ordering::Relaxed) + 1;
        let cancel = Arc::new(AtomicBool::new(false));
        active.insert(id, cancel.clone());
        Ticket {
            id,
            cancel,
            inner: self.inner.clone(),
        }
    }

    /// Cancel search `id`. True if it was still running.
    pub fn cancel(&self, id: u64) -> bool {
        match self.inner.active.lock().get(&id) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// Cancel everything in flight; returns how many were running.
    pub fn cancel_all(&self) -> usize {
        let active = self.inner.active.lock();
        for flag in active.values() {
            flag.store(true, Ordering::Relaxed);
        }
        active.len()
    }

    pub fn running(&self) -> usize {
        self.inner.active.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_increase_and_newer_cancels_older() {
        let r = Registry::default();
        let a = r.begin();
        assert!(!a.cancel.load(Ordering::Relaxed));
        let b = r.begin();
        assert!(b.id > a.id);
        assert!(a.cancel.load(Ordering::Relaxed));
        assert!(!b.cancel.load(Ordering::Relaxed));
    }

    #[test]
    fn explicit_cancel_and_cleanup() {
        let r = Registry::default();
        let a = r.begin();
        assert!(r.cancel(a.id));
        assert!(a.cancel.load(Ordering::Relaxed));
        assert_eq!(r.running(), 1);
        let id = a.id;
        drop(a);
        assert_eq!(r.running(), 0);
        assert!(!r.cancel(id));
        assert_eq!(r.cancel_all(), 0);
    }
}
