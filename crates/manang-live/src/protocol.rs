//! Plaintext protocol messages, carried inside [`crate::envelope`] frames.
//!
//! Serialised with postcard (compact, no schema negotiation: the envelope
//! version gates compatibility). Presence has its own messages in
//! [`crate::presence`] and travels in [`crate::envelope::Kind::Presence`]
//! envelopes.

use serde::{Deserialize, Serialize};

use crate::error::LiveError;

/// Position in the three-step state exchange.
///
/// A peer that (re)connects broadcasts a `Request` with its version. Each peer
/// that knows something the requester lacks answers with a `Reply` carrying
/// the missing operations and its own version. The requester applies the reply
/// and answers with a `Final` carrying whatever the replier lacks. After the
/// exchange both sides hold the union of their operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncStage {
    /// "Here is my version; send me what I lack."
    Request,
    /// "Here is what you lack, and my version."
    Reply,
    /// "And here is what you lacked of mine."
    Final,
}

/// A document-synchronisation message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    /// The three-step state exchange.
    Sync {
        /// Sender's peer id.
        from: u64,
        /// Intended recipient; `None` broadcasts. The relay cannot read this,
        /// so other peers decrypt and ignore targeted messages.
        to: Option<u64>,
        /// Step of the exchange.
        stage: SyncStage,
        /// Sender's encoded version.
        version: Vec<u8>,
        /// CRDT update bytes (may be empty).
        update: Vec<u8>,
    },
    /// An incremental local edit.
    Update {
        /// Sender's peer id.
        from: u64,
        /// CRDT update bytes.
        update: Vec<u8>,
    },
}

impl Message {
    /// Serialise.
    pub fn encode(&self) -> Result<Vec<u8>, LiveError> {
        postcard::to_allocvec(self).map_err(|e| LiveError::Protocol(e.to_string()))
    }

    /// Parse; trailing bytes are an error.
    pub fn decode(bytes: &[u8]) -> Result<Self, LiveError> {
        postcard::from_bytes(bytes).map_err(|e| LiveError::Protocol(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let m = Message::Sync {
            from: 7,
            to: Some(9),
            stage: SyncStage::Reply,
            version: vec![1, 2, 3],
            update: vec![9; 300],
        };
        assert_eq!(Message::decode(&m.encode().expect("enc")).expect("dec"), m);
        let u = Message::Update {
            from: 1,
            update: vec![],
        };
        assert_eq!(Message::decode(&u.encode().expect("enc")).expect("dec"), u);
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        for len in 0..64u8 {
            let junk: Vec<u8> = (0..len)
                .map(|i| i.wrapping_mul(37).wrapping_add(len))
                .collect();
            let _ = Message::decode(&junk);
        }
        assert!(Message::decode(&[]).is_err());
        assert!(Message::decode(&[0xff; 32]).is_err());
    }
}
