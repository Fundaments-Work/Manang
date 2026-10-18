//! [`TextCrdt`] over yrs (the Rust port of Yjs: YATA, GC on delete).

use std::sync::Arc;

use parking_lot::Mutex;
use yrs::block::ClientID;
use yrs::types::{Delta, Observable};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{
    Any, Assoc, Doc, GetString, IndexedSequence, OffsetKind, Options, Out, ReadTxn, StateVector,
    StickyIndex, Text, TextRef, Transact, Update,
};

use crate::engine::{Applied, IndexUnit, RawChange, SEED_PEER, TextCrdt};
use crate::error::LiveError;
use crate::loro_engine::merge_replacements;

const TEXT_ID: &str = "text";
const REMOTE_ORIGIN: &str = "remote";
/// yrs client ids must fit in 53 bits (Yjs interop).
const MAX_PEER: u64 = (1 << 53) - 1;

fn engine_err(e: impl std::fmt::Display) -> LiveError {
    LiveError::Engine(e.to_string())
}

fn decode_err(e: impl std::fmt::Display) -> LiveError {
    LiveError::Decode(e.to_string())
}

fn options(peer: u64) -> Result<Options, LiveError> {
    if peer > MAX_PEER {
        return Err(LiveError::Engine("peer id must be below 2^53".to_owned()));
    }
    Ok(Options {
        client_id: ClientID::new(peer),
        offset_kind: OffsetKind::Utf16,
        ..Options::default()
    })
}

/// yrs-backed text document.
pub struct YrsEngine {
    doc: Doc,
    text: TextRef,
    peer: u64,
    /// Deltas of remote transactions, filled by the text observer.
    remote_events: Arc<Mutex<Vec<Vec<Delta>>>>,
    /// Update blobs of local transactions since the last `take_local_updates`.
    local_updates: Vec<Vec<u8>>,
}

impl YrsEngine {
    fn build(peer: u64) -> Result<Self, LiveError> {
        let doc = Doc::with_options(options(peer)?);
        let text = doc.get_or_insert_text(TEXT_ID);
        let remote_events: Arc<Mutex<Vec<Vec<Delta>>>> = Arc::default();
        let sink = Arc::clone(&remote_events);
        text.observe("remote-deltas", move |txn, event| {
            if txn.origin().is_some() {
                sink.lock().push(event.delta(txn).to_vec());
            }
        });
        Ok(Self {
            doc,
            text,
            peer,
            remote_events,
            local_updates: Vec::new(),
        })
    }

    fn import(&self, bytes: &[u8]) -> Result<bool, LiveError> {
        let update = Update::decode_v1(bytes).map_err(decode_err)?;
        let mut txn = self.doc.transact_mut_with(REMOTE_ORIGIN);
        txn.apply_update(update).map_err(decode_err)?;
        let pending = txn.store().pending_update().is_some() || txn.store().pending_ds().is_some();
        Ok(pending)
    }

    fn deltas_to_changes(batches: Vec<Vec<Delta>>) -> Vec<RawChange> {
        let mut changes = Vec::new();
        for deltas in batches {
            let mut pos = 0usize;
            for delta in deltas {
                match delta {
                    Delta::Retain(n, _) => pos += n as usize,
                    Delta::Inserted(value, _) => {
                        let insert = match value {
                            Out::Any(Any::String(s)) => s.to_string(),
                            _ => String::new(),
                        };
                        let n: usize = insert.chars().map(char::len_utf16).sum();
                        changes.push(RawChange {
                            start: pos,
                            delete: 0,
                            insert,
                        });
                        pos += n;
                    }
                    Delta::Deleted(n) => changes.push(RawChange {
                        start: pos,
                        delete: n as usize,
                        insert: String::new(),
                    }),
                }
            }
        }
        merge_replacements(changes)
    }
}

impl TextCrdt for YrsEngine {
    fn name() -> &'static str {
        "yrs"
    }

    fn new(peer: u64) -> Result<Self, LiveError> {
        Self::build(peer)
    }

    fn seeded(peer: u64, text: &str) -> Result<Self, LiveError> {
        let seed = Self::build(SEED_PEER)?;
        if !text.is_empty() {
            let mut txn = seed.doc.transact_mut();
            seed.text.insert(&mut txn, 0, text);
        }
        let bytes = seed
            .doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        let engine = Self::build(peer)?;
        engine.import(&bytes)?;
        engine.remote_events.lock().clear();
        Ok(engine)
    }

    fn from_snapshot(peer: u64, bytes: &[u8]) -> Result<Self, LiveError> {
        let engine = Self::build(peer)?;
        engine.import(bytes)?;
        engine.remote_events.lock().clear();
        Ok(engine)
    }

    fn peer(&self) -> u64 {
        self.peer
    }

    fn text(&self) -> String {
        self.text.get_string(&self.doc.transact())
    }

    fn len_utf16(&self) -> usize {
        self.text.len(&self.doc.transact()) as usize
    }

    fn insert(&mut self, at: usize, text: &str) -> Result<(), LiveError> {
        let at = u32::try_from(at).map_err(engine_err)?;
        let mut txn = self.doc.transact_mut();
        self.text.insert(&mut txn, at, text);
        self.local_updates.push(txn.encode_update_v1());
        Ok(())
    }

    fn delete(&mut self, at: usize, len: usize) -> Result<(), LiveError> {
        let at = u32::try_from(at).map_err(engine_err)?;
        let len = u32::try_from(len).map_err(engine_err)?;
        let mut txn = self.doc.transact_mut();
        self.text.remove_range(&mut txn, at, len);
        self.local_updates.push(txn.encode_update_v1());
        Ok(())
    }

    fn take_local_updates(&mut self) -> Result<Vec<Vec<u8>>, LiveError> {
        Ok(std::mem::take(&mut self.local_updates))
    }

    fn version(&self) -> Vec<u8> {
        self.doc.transact().state_vector().encode_v1()
    }

    fn export_since(&self, version: &[u8]) -> Result<Vec<u8>, LiveError> {
        let sv = StateVector::decode_v1(version).map_err(decode_err)?;
        Ok(self.doc.transact().encode_diff_v1(&sv))
    }

    fn export_snapshot(&self) -> Result<Vec<u8>, LiveError> {
        Ok(self
            .doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default()))
    }

    fn export_trimmed(&self) -> Result<Vec<u8>, LiveError> {
        // GC is on, so deleted content is already dropped from the snapshot.
        self.export_snapshot()
    }

    fn apply_update(&mut self, bytes: &[u8]) -> Result<Applied, LiveError> {
        self.remote_events.lock().clear();
        let pending = self.import(bytes)?;
        let batches = std::mem::take(&mut *self.remote_events.lock());
        Ok(Applied {
            unit: IndexUnit::Utf16,
            changes: Self::deltas_to_changes(batches),
            pending,
        })
    }

    fn cursor_at(&self, at: usize) -> Option<Vec<u8>> {
        let at = u32::try_from(at).ok()?;
        let txn = self.doc.transact();
        self.text
            .sticky_index(&txn, at, Assoc::After)
            .map(|s| s.encode_v1())
    }

    fn cursor_pos(&self, cursor: &[u8]) -> Option<usize> {
        let sticky = StickyIndex::decode_v1(cursor).ok()?;
        let txn = self.doc.transact();
        sticky.get_offset(&txn).map(|o| o.index as usize)
    }
}
