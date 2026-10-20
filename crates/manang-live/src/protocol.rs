//! Plaintext protocol messages, carried inside [`crate::envelope`] frames.
//!
//! Serialised with postcard (compact, no schema negotiation: the envelope
//! version gates compatibility). Presence has its own messages in
//! [`crate::presence`] and travels in [`crate::envelope::Kind::Presence`]
//! envelopes.

use serde::{Deserialize, Serialize};

use crate::blob::{BlobHash, CHUNK_SIZE, MAX_CHUNKS};
use crate::error::LiveError;
use crate::manifest::{DocId, MAX_MANIFEST_PAGES, MAX_PAGE_ENTRIES, ManifestEntry};

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
        /// Digest of the sender's text at that version. Equal versions with
        /// different digests mean the replicas diverged (a bug, or two peers
        /// that seeded different files): see `Output::Diverged`.
        digest: u64,
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
    // Wire v2 (project sessions). Appended: the variant indices above must
    // never change.
    /// The three-step state exchange for one document of a project session.
    DocSync {
        /// Which document.
        doc: DocId,
        /// Sender's peer id.
        from: u64,
        /// Intended recipient; `None` broadcasts.
        to: Option<u64>,
        /// Step of the exchange.
        stage: SyncStage,
        /// Sender's encoded version of that document.
        version: Vec<u8>,
        /// Divergence digest ([`text_digest`] for text, 0 for the tree).
        digest: u64,
        /// CRDT update bytes (may be empty).
        update: Vec<u8>,
    },
    /// An incremental local edit of one document of a project session.
    DocUpdate {
        /// Which document.
        doc: DocId,
        /// Sender's peer id.
        from: u64,
        /// CRDT update bytes.
        update: Vec<u8>,
    },
    /// One page of the project manifest.
    Manifest {
        /// Sender's peer id.
        from: u64,
        /// Intended recipient; `None` broadcasts.
        to: Option<u64>,
        /// [`crate::manifest::manifest_root`] of the whole manifest.
        root: [u8; 16],
        /// This page, from 0.
        page: u32,
        /// Total pages.
        pages: u32,
        /// At most [`MAX_PAGE_ENTRIES`] entries.
        entries: Vec<ManifestEntry>,
    },
    /// Ask for one chunk of a blob.
    BlobGet {
        /// Requester's peer id.
        from: u64,
        /// Peer asked; `None` asks everyone.
        to: Option<u64>,
        /// Content address.
        hash: BlobHash,
        /// Chunk index.
        chunk: u32,
    },
    /// One chunk of a blob.
    BlobChunk {
        /// Sender's peer id.
        from: u64,
        /// Intended recipient; `None` broadcasts.
        to: Option<u64>,
        /// Content address of the whole blob.
        hash: BlobHash,
        /// Chunk index.
        chunk: u32,
        /// Total chunks.
        count: u32,
        /// At most [`CHUNK_SIZE`] bytes.
        data: Vec<u8>,
    },
    /// Ask for the manifest.
    ManifestGet {
        /// Requester's peer id.
        from: u64,
        /// Peer asked; `None` asks everyone.
        to: Option<u64>,
    },
}

/// Largest accepted encoded version vector.
pub const MAX_VERSION_BYTES: usize = 64 * 1024;

/// 64-bit FNV-1a of the document text: a cheap divergence detector (not a
/// security feature; every room member is trusted equally).
#[must_use]
pub fn text_digest(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

impl Message {
    /// Serialise.
    pub fn encode(&self) -> Result<Vec<u8>, LiveError> {
        postcard::to_allocvec(self).map_err(|e| LiveError::Protocol(e.to_string()))
    }

    /// Parse; trailing bytes are an error. Wire v2 messages are also checked
    /// against their size bounds.
    pub fn decode(bytes: &[u8]) -> Result<Self, LiveError> {
        let msg: Self =
            postcard::from_bytes(bytes).map_err(|e| LiveError::Protocol(e.to_string()))?;
        msg.check_bounds()?;
        Ok(msg)
    }

    /// Enforce the bounds of the v2 variants (`Sync`/`Update` are bounded by
    /// the envelope's plaintext cap alone, as before).
    fn check_bounds(&self) -> Result<(), LiveError> {
        let bad = |m: &str| Err(LiveError::Protocol(m.to_owned()));
        match self {
            Self::DocSync { version, .. } if version.len() > MAX_VERSION_BYTES => {
                bad("version too large")
            }
            Self::Manifest {
                page,
                pages,
                entries,
                ..
            } => {
                if *pages == 0 || *pages > MAX_MANIFEST_PAGES || page >= pages {
                    bad("manifest page out of range")
                } else if entries.len() > MAX_PAGE_ENTRIES {
                    bad("manifest page too large")
                } else {
                    entries
                        .iter()
                        .try_for_each(ManifestEntry::validate)
                        .map_err(|e| LiveError::Protocol(e.to_string()))
                }
            }
            Self::BlobGet { chunk, .. } if *chunk >= MAX_CHUNKS => bad("chunk index too large"),
            Self::BlobChunk {
                chunk, count, data, ..
            } => {
                if *count == 0 || *count > MAX_CHUNKS || chunk >= count {
                    bad("chunk index out of range")
                } else if data.len() > CHUNK_SIZE {
                    bad("chunk too large")
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
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
            digest: 77,
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
