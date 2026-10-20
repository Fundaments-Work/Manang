//! Documents in memory: loading, evicting to the sidecar, and getting the
//! room's text onto the disk.
//!
//! A project can hold far more text files than we keep documents for.
//! [`Residency`] tracks recency; the least recently used documents that
//! nothing needs (not open in a tab, not still being fetched, nothing queued
//! for the disk) are parked as full snapshots in the [`SidecarStore`] and
//! unloaded from the session. Loading one again restores the snapshot (never
//! re-seeds edited text: seeding is only valid for text that is exactly the
//! file's original content, whose fingerprint the tree records), and the
//! state exchange on load brings whatever the room did meanwhile.

use manang_live::TextCrdt;
use manang_live::bridge::LiveDoc;
use manang_live::envelope::seed_fingerprint;
use manang_live::project::{FileId, NodeKind};
use manang_live::session::Output;

use super::{Driver, META_QUIET_MS};
use crate::disk::{Observed, read_capped, sha256};
use crate::fsbridge::text_edits;
use crate::sidecar::Rehydrated;

/// What [`Driver::ensure_loaded`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Load {
    /// The document is here and usable.
    Ready,
    /// The document is being fetched from the room.
    Fetching,
    /// This side cannot produce the document (nothing to restore or seed
    /// from, nobody to fetch from).
    Unavailable,
}

impl Driver {
    /// Whether the file's document is loaded and holds the room's state (or
    /// this side's own, which is the same thing for a document it made).
    pub(super) fn doc_usable(&self, id: FileId) -> bool {
        self.session.is_loaded(id)
            && (self.files.get(&id).is_some_and(|f| f.local_origin)
                || self.session.is_doc_synced(id))
    }

    /// The text of a loaded document.
    pub(super) fn doc_text(&self, id: FileId) -> Option<String> {
        self.session.file_doc(id).map(|d| d.engine().text())
    }

    /// Mark a document used; park the ones that fell out of the cap.
    pub(super) fn touch(&mut self, id: FileId) {
        let evicted = self.residency.touch(id);
        self.evict_all(evicted);
    }

    pub(super) fn evict_all(&mut self, ids: Vec<FileId>) {
        for id in ids {
            self.try_evict(id);
        }
    }

    /// Park one document, or keep it for now (pinned, retried later).
    fn try_evict(&mut self, id: FileId) {
        if !self.session.is_loaded(id) {
            self.residency.remove(&id);
            return;
        }
        let busy = self.files.get(&id).is_some_and(|f| f.attached)
            || self.arriving.contains(&id)
            || self.pending_text.contains_key(&id)
            || !self.doc_usable(id);
        let Some(path) = self.path_of(id) else {
            self.drop_doc(id);
            return;
        };
        if busy || !self.park(id, &path) {
            self.residency.pin(id);
            self.deferred_evict.insert(id);
        }
    }

    /// Write the document to the sidecar and unload it. `false` if the disk
    /// has not caught up or the sidecar cannot be written (the document then
    /// stays loaded).
    fn park(&mut self, id: FileId, path: &str) -> bool {
        let Some(doc) = self.session.file_doc(id) else {
            return true;
        };
        let text = doc.engine().text();
        let Ok(snapshot) = doc.engine().export_snapshot() else {
            return false;
        };
        let _ = self.disk.flush_path(path);
        if self.disk.is_behind(path) {
            return false;
        }
        // Only a document the disk already agrees with is parked: a file the
        // disk has not caught up with, or one changed outside the session
        // that the tree bridge has not read yet, stays loaded.
        let sha = sha256(text.as_bytes());
        if !matches!(self.disk.observe(path), Ok(Observed::Content(h)) if h == sha) {
            return false;
        }
        if self
            .sidecar
            .save(&self.room_hex, id, &sha, &snapshot)
            .is_err()
        {
            return false;
        }
        let _ = self.session.unload_doc(id);
        self.residency.remove(&id);
        self.deferred_evict.remove(&id);
        self.dirty_docs.remove(&id);
        let state = self.file(id);
        state.synced_sha = Some(sha);
        state.local_origin = false;
        true
    }

    /// Retry the evictions that had to wait.
    pub(super) fn retry_evictions(&mut self) {
        let ids: Vec<FileId> = self.deferred_evict.iter().copied().collect();
        for id in ids {
            if self.files.get(&id).is_some_and(|f| f.attached) || self.arriving.contains(&id) {
                continue;
            }
            self.deferred_evict.remove(&id);
            let more = self.residency.unpin(&id);
            self.evict_all(more);
            self.try_evict(id);
        }
    }

    /// Forget a document without keeping it (excluded, deleted, replaced).
    pub(super) fn drop_doc(&mut self, id: FileId) {
        let _ = self.session.unload_doc(id);
        self.residency.remove(&id);
        self.deferred_evict.remove(&id);
        self.ropes.remove(&id);
        self.sidecar.discard(&self.room_hex, id);
        if let Some(state) = self.files.get_mut(&id) {
            state.local_origin = false;
            state.synced_sha = None;
        }
    }

    /// Have the document of `id` loaded: restore it from the sidecar, seed it
    /// from the disk (only if the disk still holds the file's original text),
    /// or fetch it from the room (guests). Loading answers whatever peers
    /// asked for while it was missing.
    pub(super) fn ensure_loaded(&mut self, id: FileId) -> (Load, Vec<Output>) {
        if self.session.is_loaded(id) {
            self.touch(id);
            let state = if self.doc_usable(id) {
                Load::Ready
            } else {
                Load::Fetching
            };
            return (state, Vec::new());
        }
        let view = self.view();
        let Some(entry) = view.get(id) else {
            return (Load::Unavailable, Vec::new());
        };
        if entry.meta.kind != NodeKind::Text
            || self.files.get(&id).is_some_and(|f| f.excluded)
            || self.excluded_paths.contains(&entry.path)
            || self.disk.policy().check_rel(&entry.path).is_err()
        {
            return (Load::Unavailable, Vec::new());
        }
        let now = self.now;
        if let Some(sha) = self.files.get(&id).and_then(|f| f.synced_sha) {
            match self.sidecar.load(&self.room_hex, id, &sha) {
                Rehydrated::Hit(bytes) => {
                    match manang_live::LoroEngine::from_snapshot(self.peer, &bytes) {
                        Ok(engine) => {
                            let out = self.session.provide_doc(
                                now,
                                id,
                                LiveDoc::from_engine(engine),
                                false,
                            );
                            self.file(id).local_origin = true;
                            self.touch(id);
                            return (Load::Ready, out);
                        }
                        Err(_) => self.sidecar.discard(&self.room_hex, id),
                    }
                }
                Rehydrated::Miss | Rehydrated::Discarded(_) => {}
            }
        }
        if let Some(text) = self.original_text(&entry.path, &entry.meta.seed_fp)
            && let Ok(doc) = LiveDoc::seeded(self.peer, &text)
        {
            let out = self.session.provide_doc(now, id, doc, false);
            let state = self.file(id);
            state.local_origin = true;
            state.synced_sha = None;
            self.touch(id);
            return (Load::Ready, out);
        }
        if self.role == super::Role::Guest {
            self.residency.pin(id);
            self.touch(id);
            self.file(id).local_origin = false;
            self.arriving.insert(id);
            return (Load::Fetching, self.session.want_doc(now, id, false));
        }
        (Load::Unavailable, Vec::new())
    }

    /// The file's text if it is exactly the original seed (`seed_fp`).
    fn original_text(&self, path: &str, seed_fp: &[u8; 16]) -> Option<String> {
        let bytes = read_capped(
            &self.disk.policy().root().path().join(path),
            self.disk.policy().max_text(),
        )
        .ok()?;
        let text = String::from_utf8(bytes).ok()?;
        (seed_fingerprint(&text) == *seed_fp).then_some(text)
    }

    // -- the room's text onto the disk ---------------------------------------

    /// Queue the document's text for the disk (a file open in a tab is the
    /// web view's to write; an excluded one is not ours).
    pub(super) fn queue_disk(&mut self, id: FileId) {
        if self
            .files
            .get(&id)
            .is_some_and(|f| f.attached || f.excluded)
        {
            return;
        }
        let (Some(path), Some(text)) = (self.path_of(id), self.doc_text(id)) else {
            return;
        };
        if self.excluded_paths.contains(&path) {
            return;
        }
        if let Err(e) = self.disk.queue_text(&path, &text) {
            self.skip(&path, &e.to_string());
        }
    }

    /// Remote updates reached documents: write them (or, for a document whose
    /// first content is still arriving, wait until it is complete).
    pub(super) fn flush_dirty_docs(&mut self) {
        let ids: Vec<FileId> = self.dirty_docs.iter().copied().collect();
        for id in ids {
            if self.arriving.contains(&id) {
                if self.doc_usable(id) {
                    self.dirty_docs.remove(&id);
                    self.complete_arrival(id);
                }
                continue;
            }
            self.dirty_docs.remove(&id);
            self.queue_disk(id);
            self.touch(id);
        }
    }

    /// Files whose fetch finished without any change (an empty file).
    pub(super) fn settle_arrivals(&mut self) {
        let ids: Vec<FileId> = self.arriving.iter().copied().collect();
        for id in ids {
            if self.doc_usable(id) {
                self.dirty_docs.remove(&id);
                self.complete_arrival(id);
            }
        }
    }

    /// The first full content of a fetched file is here: put it on the disk.
    /// A differing local copy is saved to the trash first.
    fn complete_arrival(&mut self, id: FileId) {
        self.arriving.remove(&id);
        let replace = {
            let state = self.file(id);
            std::mem::take(&mut state.replace_on_arrival)
        };
        let attached = self.files.get(&id).is_some_and(|f| f.attached);
        if !attached
            && !self.pending_text.contains_key(&id)
            && let (Some(path), Some(text)) = (self.arrival_path(id), self.doc_text(id))
            && !self.excluded_paths.contains(&path)
        {
            // The tab, or the guest's own pending edit, comes first; any
            // other file's first content goes to the disk now.
            let result = if replace {
                self.disk.take_host(&path, text.as_bytes()).map(|_| ())
            } else {
                self.disk.queue_text(&path, &text).map(|_| ())
            };
            if let Err(e) = result {
                self.skip(&path, &e.to_string());
            }
        }
        // Only now may the document be parked: its text is on its way.
        let evicted = self.residency.unpin(&id);
        self.evict_all(evicted);
        self.touch(id);
    }

    /// The path of a file whose content just arrived: the tree's, or the
    /// manifest's while the tree has not reached a guest yet.
    pub(super) fn arrival_path(&mut self, id: FileId) -> Option<String> {
        match self.path_of(id) {
            Some(path) => Some(path),
            None => self.files.get(&id).and_then(|f| f.known_path.clone()),
        }
    }

    /// Edits made on disk while the document was still being fetched.
    pub(super) fn apply_pending_text(&mut self) {
        let ids: Vec<FileId> = self.pending_text.keys().copied().collect();
        for id in ids {
            if !self.doc_usable(id) || self.arriving.contains(&id) {
                continue;
            }
            let Some(text) = self.pending_text.remove(&id) else {
                continue;
            };
            let out = self.apply_text(id, &text);
            self.perform(out);
        }
    }

    /// Make the document's text equal `new` with minimal local edits (so
    /// remote carets survive), and share them.
    pub(super) fn apply_text(&mut self, id: FileId, new: &str) -> Vec<Output> {
        let Some(old) = self.doc_text(id) else {
            return Vec::new();
        };
        let edits = text_edits(&old, new);
        if edits.is_empty() {
            return Vec::new();
        }
        let Some(doc) = self.session.file_doc_mut(id) else {
            return Vec::new();
        };
        for edit in edits.iter().rev() {
            if let Err(e) = doc.record_local_bytes(edit.start, edit.end - edit.start, &edit.insert)
            {
                let message = e.to_string();
                if let Some(path) = self.path_of(id) {
                    self.skip(&path, &message);
                }
                break;
            }
        }
        self.meta_due.insert(id, self.now + META_QUIET_MS);
        self.touch(id);
        self.session.flush_doc(id)
    }

    /// Record the new size and hash of files edited here, once the typing
    /// pauses, so a late joiner's plan compares against the current text.
    pub(super) fn update_metas(&mut self) {
        let now = self.now;
        let due: Vec<FileId> = self
            .meta_due
            .iter()
            .filter(|(_, at)| **at <= now)
            .map(|(id, _)| *id)
            .collect();
        if due.is_empty() {
            return;
        }
        let view = self.view();
        let mut changed = false;
        for id in due {
            self.meta_due.remove(&id);
            let Some(entry) = view.get(id) else { continue };
            let Some(text) = self.doc_text(id) else {
                continue;
            };
            let sha = sha256(text.as_bytes());
            if entry.meta.sha256 == sha {
                continue;
            }
            if let Some(project) = self.session.project_mut()
                && project
                    .set_content(id, text.len() as u64, sha, entry.meta.seed_fp)
                    .is_ok()
            {
                changed = true;
            }
        }
        if changed {
            let out = self.session.flush_project();
            self.perform(out);
        }
    }
}
