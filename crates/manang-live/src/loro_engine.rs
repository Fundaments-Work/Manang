//! [`TextCrdt`] over Loro (Fugue-based text, columnar change store).

use std::sync::Arc;

use loro::{
    ContainerTrait, EventTriggerKind, ExportMode, LoroDoc, LoroText, Subscription, TextDelta,
    VersionVector,
    cursor::{Cursor, PosType, Side},
    event::Diff,
};
use parking_lot::Mutex;

use crate::engine::{Applied, IndexUnit, RawChange, SEED_PEER, TextCrdt};
use crate::error::LiveError;

const TEXT_ID: &str = "text";

fn engine_err(e: impl std::fmt::Display) -> LiveError {
    LiveError::Engine(e.to_string())
}

fn decode_err(e: impl std::fmt::Display) -> LiveError {
    LiveError::Decode(e.to_string())
}

/// Loro-backed text document.
pub struct LoroEngine {
    doc: LoroDoc,
    text: LoroText,
    peer: u64,
    /// Deltas of imports, filled by the root subscription.
    import_events: Arc<Mutex<Vec<Vec<TextDelta>>>>,
    /// Update blobs of local commits, filled by the local-update subscription.
    local_updates: Arc<Mutex<Vec<Vec<u8>>>>,
    _event_sub: Subscription,
    _local_sub: Subscription,
}

impl LoroEngine {
    fn build(doc: LoroDoc, peer: u64) -> Result<Self, LiveError> {
        doc.set_peer_id(peer).map_err(engine_err)?;
        let text = doc.get_text(TEXT_ID);
        let import_events: Arc<Mutex<Vec<Vec<TextDelta>>>> = Arc::default();
        let local_updates: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
        let sink = Arc::clone(&import_events);
        let event_sub = doc.subscribe_root(Arc::new(move |event| {
            if event.triggered_by != EventTriggerKind::Import {
                return;
            }
            for container in &event.events {
                if let Diff::Text(deltas) = &container.diff {
                    sink.lock().push(deltas.clone());
                }
            }
        }));
        let updates = Arc::clone(&local_updates);
        let local_sub = doc.subscribe_local_update(Box::new(move |bytes| {
            updates.lock().push(bytes.clone());
            true
        }));
        Ok(Self {
            doc,
            text,
            peer,
            import_events,
            local_updates,
            _event_sub: event_sub,
            _local_sub: local_sub,
        })
    }

    /// Deltas to rope-applicable changes, in Unicode scalar units.
    fn deltas_to_changes(batches: Vec<Vec<TextDelta>>) -> Vec<RawChange> {
        let mut changes: Vec<RawChange> = Vec::new();
        for deltas in batches {
            let mut pos = 0usize;
            for delta in deltas {
                match delta {
                    TextDelta::Retain { retain, .. } => pos += retain,
                    TextDelta::Insert { insert, .. } => {
                        let n = insert.chars().count();
                        changes.push(RawChange {
                            start: pos,
                            delete: 0,
                            insert,
                        });
                        pos += n;
                    }
                    TextDelta::Delete { delete } => changes.push(RawChange {
                        start: pos,
                        delete,
                        insert: String::new(),
                    }),
                }
            }
        }
        merge_replacements(changes)
    }
}

/// Fold an adjacent "delete at p, insert at p" pair into one replacement.
pub(crate) fn merge_replacements(changes: Vec<RawChange>) -> Vec<RawChange> {
    let mut out: Vec<RawChange> = Vec::with_capacity(changes.len());
    for change in changes {
        if let Some(last) = out.last_mut()
            && last.insert.is_empty()
            && change.delete == 0
            && last.start == change.start
        {
            last.insert = change.insert;
            continue;
        }
        out.push(change);
    }
    out
}

impl TextCrdt for LoroEngine {
    fn name() -> &'static str {
        "loro"
    }

    fn new(peer: u64) -> Result<Self, LiveError> {
        Self::build(LoroDoc::new(), peer)
    }

    fn seeded(peer: u64, text: &str) -> Result<Self, LiveError> {
        let seed_doc = LoroDoc::new();
        seed_doc.set_peer_id(SEED_PEER).map_err(engine_err)?;
        if !text.is_empty() {
            seed_doc
                .get_text(TEXT_ID)
                .insert(0, text)
                .map_err(engine_err)?;
        }
        seed_doc.commit();
        let bytes = seed_doc
            .export(ExportMode::all_updates())
            .map_err(engine_err)?;
        let engine = Self::build(LoroDoc::new(), peer)?;
        engine.doc.import(&bytes).map_err(decode_err)?;
        engine.import_events.lock().clear();
        Ok(engine)
    }

    fn from_snapshot(peer: u64, bytes: &[u8]) -> Result<Self, LiveError> {
        let engine = Self::build(LoroDoc::new(), peer)?;
        engine.doc.import(bytes).map_err(decode_err)?;
        engine.import_events.lock().clear();
        Ok(engine)
    }

    fn peer(&self) -> u64 {
        self.peer
    }

    fn text(&self) -> String {
        self.text.to_string()
    }

    fn len_bytes(&self) -> usize {
        self.text.len_utf8()
    }

    fn insert(&mut self, at: usize, text: &str) -> Result<(), LiveError> {
        self.text.insert_utf8(at, text).map_err(engine_err)
    }

    fn delete(&mut self, at: usize, len: usize) -> Result<(), LiveError> {
        self.text.delete_utf8(at, len).map_err(engine_err)
    }

    fn take_local_updates(&mut self) -> Result<Vec<Vec<u8>>, LiveError> {
        self.doc.commit();
        Ok(std::mem::take(&mut *self.local_updates.lock()))
    }

    fn version(&self) -> Vec<u8> {
        self.doc.oplog_vv().encode()
    }

    fn export_since(&self, version: &[u8]) -> Result<Vec<u8>, LiveError> {
        let vv = VersionVector::decode(version).map_err(decode_err)?;
        self.doc
            .export(ExportMode::updates(&vv))
            .map_err(engine_err)
    }

    fn export_snapshot(&self) -> Result<Vec<u8>, LiveError> {
        self.doc.export(ExportMode::Snapshot).map_err(engine_err)
    }

    fn export_trimmed(&self) -> Result<Vec<u8>, LiveError> {
        let frontiers = self.doc.oplog_frontiers();
        self.doc
            .export(ExportMode::shallow_snapshot(&frontiers))
            .map_err(engine_err)
    }

    fn compact_memory(&mut self) {
        self.doc.compact_change_store();
        self.doc.free_history_cache();
        self.doc.free_diff_calculator();
    }

    fn apply_update(&mut self, bytes: &[u8]) -> Result<Applied, LiveError> {
        self.import_events.lock().clear();
        let status = self.doc.import(bytes).map_err(decode_err)?;
        let batches = std::mem::take(&mut *self.import_events.lock());
        Ok(Applied {
            unit: IndexUnit::Chars,
            changes: Self::deltas_to_changes(batches),
            pending: status.pending.is_some(),
        })
    }

    fn cursor_at(&self, at: usize) -> Option<Vec<u8>> {
        let unicode = self
            .text
            .convert_pos(at, PosType::Bytes, PosType::Unicode)?;
        self.text
            .get_cursor(unicode, Side::Middle)
            .map(|c| c.encode())
    }

    fn cursor_pos(&self, cursor: &[u8]) -> Option<usize> {
        let cursor = Cursor::decode(cursor).ok()?;
        // A cursor comes from a peer (presence). One that names another
        // container, a map or a tree, reaches an `unreachable!()` inside Loro
        // (found by fuzzing), so only this document's own text is resolved.
        if cursor.container != self.text.id() {
            return None;
        }
        let unicode = self.doc.get_cursor_pos(&cursor).ok()?.current.pos;
        self.text
            .convert_pos(unicode, PosType::Unicode, PosType::Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Found by fuzzing (`fuzz/regressions/live_text_apply/loro-cursor-container`):
    /// a peer's cursor that names a non-text container hit `unreachable!()`
    /// inside Loro when resolved.
    #[test]
    fn hostile_cursor_is_none_not_a_panic() {
        let crash: [u8; 6] = [0, 0, 0, 0x98, 0, 0];
        let mut e = LoroEngine::new(10).expect("engine");
        e.insert(0, "hello").expect("insert");
        assert_eq!(e.cursor_pos(&crash), None);
        // Our own cursors still resolve.
        let mine = e.cursor_at(2).expect("cursor");
        assert_eq!(e.cursor_pos(&mine), Some(2));
    }
}
