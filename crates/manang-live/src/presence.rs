//! Presence: who is here, and where their caret is.
//!
//! Presence is **ephemeral**. It is never written to disk, never part of the
//! CRDT, never replayed on reconnect (a returning peer simply broadcasts a
//! fresh state), and expires if a peer goes quiet. Carets are stored as
//! engine-native stable positions, so they stay attached to the text they were
//! on while concurrent edits move it; they are resolved to UTF-16 offsets only
//! when drawn.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::LiveError;

/// Number of distinct colour slots. The UI maps a slot to one of the existing
/// design tokens; presence never carries a colour value.
pub const PALETTE_SLOTS: u8 = 8;
/// Longest accepted display name, in bytes.
pub const MAX_NAME: usize = 64;
/// Longest accepted stable-position blob, in bytes.
pub const MAX_CURSOR: usize = 256;

/// A selection as two stable positions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    /// The fixed end (where the drag started).
    pub anchor: Vec<u8>,
    /// The moving end (the caret).
    pub head: Vec<u8>,
}

/// One peer's presence state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    /// The peer.
    pub peer: u64,
    /// Monotonic per-peer counter; stale or replayed states are ignored.
    pub seq: u64,
    /// Display name (from git identity, or chosen at join).
    pub name: String,
    /// Colour slot in `0..PALETTE_SLOTS`.
    pub slot: u8,
    /// Current selection, if the peer has a caret in this document.
    pub selection: Option<Selection>,
}

impl Presence {
    /// Clamp hostile values rather than trusting a remote peer.
    fn sanitised(mut self) -> Option<Self> {
        if let Some(sel) = &self.selection
            && (sel.anchor.len() > MAX_CURSOR || sel.head.len() > MAX_CURSOR)
        {
            return None;
        }
        if self.name.len() > MAX_NAME {
            let mut cut = MAX_NAME;
            while !self.name.is_char_boundary(cut) {
                cut -= 1;
            }
            self.name.truncate(cut);
        }
        self.slot %= PALETTE_SLOTS;
        Some(self)
    }
}

/// Messages carried in presence envelopes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PresenceMessage {
    /// A peer's current state.
    State(Presence),
    /// A peer left deliberately.
    Leave {
        /// The peer that left.
        peer: u64,
    },
}

impl PresenceMessage {
    /// Serialise.
    pub fn encode(&self) -> Result<Vec<u8>, LiveError> {
        postcard::to_allocvec(self).map_err(|e| LiveError::Protocol(e.to_string()))
    }

    /// Parse.
    pub fn decode(bytes: &[u8]) -> Result<Self, LiveError> {
        postcard::from_bytes(bytes).map_err(|e| LiveError::Protocol(e.to_string()))
    }
}

/// The set of remote peers currently believed present.
#[derive(Debug, Default)]
pub struct PresenceBook {
    peers: BTreeMap<u64, (Presence, u64)>,
    ttl_ms: u64,
}

impl PresenceBook {
    /// A book that forgets peers silent for `ttl_ms`.
    #[must_use]
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            peers: BTreeMap::new(),
            ttl_ms,
        }
    }

    /// Record a state seen at `now_ms`. Returns whether the book changed.
    pub fn update(&mut self, presence: Presence, now_ms: u64) -> bool {
        let Some(presence) = presence.sanitised() else {
            return false;
        };
        match self.peers.get_mut(&presence.peer) {
            Some((existing, seen)) => {
                if presence.seq <= existing.seq {
                    return false;
                }
                *seen = now_ms;
                let changed = *existing != presence;
                *existing = presence;
                changed
            }
            None => {
                self.peers.insert(presence.peer, (presence, now_ms));
                true
            }
        }
    }

    /// Forget a peer. Returns whether it was present.
    pub fn remove(&mut self, peer: u64) -> bool {
        self.peers.remove(&peer).is_some()
    }

    /// Drop peers silent for longer than the TTL; returns how many.
    pub fn expire(&mut self, now_ms: u64) -> usize {
        let ttl = self.ttl_ms;
        let before = self.peers.len();
        self.peers
            .retain(|_, (_, seen)| now_ms.saturating_sub(*seen) <= ttl);
        before - self.peers.len()
    }

    /// Remove everything (on leaving the room).
    pub fn clear(&mut self) {
        self.peers.clear();
    }

    /// Current peers, ordered by peer id.
    pub fn iter(&self) -> impl Iterator<Item = &Presence> {
        self.peers.values().map(|(p, _)| p)
    }

    /// Number of remote peers present.
    #[must_use]
    pub fn len(&self) -> usize {
        self.peers.len()
    }

    /// Whether nobody else is present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(peer: u64, seq: u64) -> Presence {
        Presence {
            peer,
            seq,
            name: "Ada".into(),
            slot: 3,
            selection: None,
        }
    }

    #[test]
    fn stale_and_replayed_states_are_ignored() {
        let mut b = PresenceBook::new(1000);
        assert!(b.update(p(1, 5), 0));
        assert!(!b.update(p(1, 5), 1));
        assert!(!b.update(p(1, 4), 1));
        let mut newer = p(1, 6);
        newer.name = "Grace".into();
        assert!(b.update(newer, 2));
        assert_eq!(b.iter().next().map(|x| x.name.as_str()), Some("Grace"));
    }

    #[test]
    fn expiry_and_leave() {
        let mut b = PresenceBook::new(100);
        b.update(p(1, 1), 0);
        b.update(p(2, 1), 90);
        assert_eq!(b.expire(150), 1);
        assert_eq!(b.len(), 1);
        assert!(b.remove(2));
        assert!(b.is_empty());
    }

    #[test]
    fn hostile_values_are_clamped() {
        let mut b = PresenceBook::new(100);
        let mut big = p(1, 1);
        big.name = "é".repeat(100);
        big.slot = 200;
        assert!(b.update(big, 0));
        let got = b.iter().next().expect("one");
        assert!(got.name.len() <= MAX_NAME);
        assert!(got.slot < PALETTE_SLOTS);
        let mut huge = p(2, 1);
        huge.selection = Some(Selection {
            anchor: vec![0; MAX_CURSOR + 1],
            head: vec![],
        });
        assert!(!b.update(huge, 0));
    }

    #[test]
    fn message_roundtrip() {
        let m = PresenceMessage::State(p(1, 1));
        assert_eq!(
            PresenceMessage::decode(&m.encode().expect("e")).expect("d"),
            m
        );
        assert!(PresenceMessage::decode(&[9, 9, 9]).is_err());
    }
}
