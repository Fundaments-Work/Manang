//! What the room tells us: session outputs become disk operations, view
//! events and notices.
//!
//! Every path here comes from the network (the project tree is replicated), so
//! it is applied only through the [`DiskBridge`](crate::disk::DiskBridge),
//! whose every operation runs the path policy first. A path the policy
//! refuses is never fetched and never written; it shows up in the status as
//! skipped.

use std::collections::BTreeSet;

use manang_live::blob::{BlobHash, blob_hash};
use manang_live::bridge::RemoteApplied;
use manang_live::project::{Entry, FileId, NodeKind, TreeChange};
use manang_live::session::Output;
use serde_json::{Value, json};

use super::files::Load;
use super::{Driver, PROGRESS_GAP_MS, PeerAt, Phase};
use crate::disk::{Deleted, Observed, read_capped};

impl Driver {
    /// Handle one non-transport output; returns outputs the handling caused.
    pub(super) fn absorb(&mut self, output: Output) -> Vec<Output> {
        match output {
            Output::Remote(applied) => self.on_remote(&applied),
            Output::DocChanged(id) => {
                        self.dirty_docs.insert(id);
            }
            Output::Tree(changes) => return self.on_tree(changes),
            Output::DocWanted(id) => return self.on_doc_wanted(id),
            Output::DocDiverged {
                doc, refetching, ..
            } => self.on_diverged(doc, refetching),
            Output::Progress { done, total } => {
                self.status.lock().progress = (done < total).then_some((done, total));
                // A bulk join reports thousands of steps: the view needs a
                // few a second and the last one.
                if done >= total
                    || self.now.saturating_sub(self.last_progress_ms) >= PROGRESS_GAP_MS
                {
                    self.last_progress_ms = self.now;
                    self.emit("live.progress", json!({ "done": done, "total": total }));
                }
            }
            Output::Manifest { entries, .. } => self.on_manifest(entries),
            Output::BlobRequested { peer, hash } => return self.on_blob_requested(peer, hash),
            Output::BlobReceived { hash, data } => self.on_blob_received(hash, &data),
            Output::BlobFailed { hash } => self.on_blob_failed(hash),
            Output::PresenceChanged => {
                self.presence_dirty = true;
                self.peers_dirty = true;
            }
            Output::State(state) => self.on_state(state),
            Output::HostTimeout => self.fail(
                "Nobody has answered yet. The host may have left or be offline. Your files have not been changed; Manang keeps trying.",
            ),
            Output::Rejected(_) => self.status.lock().rejected += 1,
            Output::Diverged { .. } | Output::Send(_) | Output::Connect | Output::Close => {}
        }
        Vec::new()
    }

    /// A remote step in a file open in a tab: mirror it into the shared
    /// editor copy and tell the view.
    fn on_remote(&mut self, applied: &RemoteApplied) {
        let Some(id) = applied.doc else { return };
        if applied.changes.is_empty() {
            return;
        }
        let editor_doc = self.files.get(&id).and_then(|f| f.editor_doc);
        if let Some(doc) = editor_doc {
            for c in &applied.changes {
                if let Err(e) = self.editor.replace_silent(
                    Some(doc),
                    c.char_start,
                    c.char_start + c.char_delete,
                    &c.insert,
                ) {
                    self.fail(&format!("The shared copy of a file is gone: {e}"));
                    return;
                }
            }
        }
        let changes: Vec<Value> = applied
            .changes
            .iter()
            .map(
                |c| json!({ "start": c.utf16_start, "delete": c.utf16_delete, "insert": c.insert }),
            )
            .collect();
        let Some(path) = self.open_path_of(id) else {
            return;
        };
        let version = {
            let state = self.file(id);
            state.version += 1;
            state.version
        };
        self.emit(
            "live.remote",
            json!({ "path": path, "version": version, "ack": false, "changes": changes }),
        );
        self.touch(id);
        self.presence_dirty = true;
    }

    fn on_diverged(&mut self, id: FileId, refetching: bool) {
        let path = self.path_of(id).unwrap_or_default();
        self.skip(
            &path,
            if refetching {
                "the copies differed; this file is being fetched again"
            } else {
                "a peer's copy of this file differs"
            },
        );
        if refetching {
            // The document was replaced by an empty one: nothing may reach the
            // disk until the room's text is back.
            self.file(id).local_origin = false;
            self.arriving.insert(id);
            self.residency.pin(id);
        }
    }

    // -- the tree --------------------------------------------------------------

    fn on_tree(&mut self, changes: Vec<TreeChange>) -> Vec<Output> {
        if !matches!(self.phase, Phase::Live) {
            return Vec::new(); // a guest has decided nothing yet: the plan covers it
        }
        let view = self.view();
        let sole = self.sole_peer();
        let mut ops: Vec<Value> = Vec::new();
        let mut out: Vec<Output> = Vec::new();
        for change in changes {
            match change {
                TreeChange::Created { id, path } => {
                    // Files the join already took are not news to the view.
                    if !self.files.get(&id).is_some_and(|f| f.joined) {
                        ops.push(json!({
                            "op": "create", "path": path, "by": id.peer().to_string(),
                        }));
                    }
                    if let Some(entry) = view.get(id) {
                        out.extend(self.on_created(entry.clone()));
                    }
                }
                TreeChange::Deleted { id, path } => {
                    ops.push(json!({ "op": "delete", "path": path, "by": sole }));
                    self.on_deleted(id, &path, &sole);
                }
                TreeChange::Moved { id, from, to } => {
                    let same_parent = parent_of(&from) == parent_of(&to);
                    ops.push(json!({
                        "op": if same_parent { "rename" } else { "move" },
                        "path": to,
                        "from": from,
                        "by": sole,
                    }));
                    self.on_moved(id, &from, &to, &sole);
                }
                TreeChange::Content { id, path } => {
                    let _ = path;
                    if let Some(entry) = view.get(id) {
                        out.extend(self.on_content(entry.clone()));
                    }
                }
            }
        }
        if !ops.is_empty() {
            self.emit("live.tree", json!({ "ops": ops }));
        }
        self.peers_dirty = true;
        out
    }

    /// The peer to credit for a tree change whose author the replica does not
    /// record: the only other person present, else nobody (`""`).
    fn sole_peer(&self) -> String {
        let peers = self.session.peers();
        match peers.as_slice() {
            [(peer, _, _)] => peer.to_string(),
            _ => String::new(),
        }
    }

    fn on_deleted(&mut self, id: FileId, path: &str, by: &str) {
        self.arriving.remove(&id);
        self.pending_text.remove(&id);
        self.dirty_docs.remove(&id);
        self.meta_due.remove(&id);
        let (attached, excluded) = self
            .files
            .get(&id)
            .map_or((false, false), |f| (f.attached, f.excluded));
        if attached {
            // The tab keeps its text; the document leaves the session.
            let _ = self.session.unload_doc(id);
            self.residency.remove(&id);
            let _ = self.fs.remote_delete(path, by, true);
        } else {
            self.drop_doc(id);
            self.files.remove(&id);
        }
        if excluded || self.excluded_paths.contains(path) {
            self.notice("deleted", by, Some(path));
            return;
        }
        match self.disk.observe(path) {
            Ok(Observed::Dir) => {
                // Children are deleted one by one; the folder goes only when
                // nothing of anyone's is left in it.
                if let Ok(safe) = self.disk.policy().check_write(path) {
                    let _ = std::fs::remove_dir(&safe.abs);
                }
            }
            Ok(_) => match self.disk.delete(path) {
                Ok(Deleted::Trashed(trash)) => {
                    self.deleted_trash.insert(path.to_owned(), trash);
                }
                Ok(Deleted::AlreadyGone | Deleted::DeferredOpen) => {}
                Err(e) => self.skip(path, &e.to_string()),
            },
            Err(e) => self.skip(path, &e.to_string()),
        }
        self.notice("deleted", by, Some(path));
    }

    fn on_moved(&mut self, id: FileId, from: &str, to: &str, by: &str) {
        if let Some(state) = self.files.get_mut(&id) {
            state.known_path = Some(to.to_owned());
            if state.attached {
                state.open_path = Some(to.to_owned());
            }
        }
        let excluded =
            self.files.get(&id).is_some_and(|f| f.excluded) || self.excluded_paths.contains(from);
        if excluded {
            return;
        }
        if let Err(e) = self.disk.rename(from, to) {
            self.skip(to, &e.to_string());
        }
        self.notice("renamed", by, Some(to));
    }

    fn on_content(&mut self, entry: Entry) -> Vec<Output> {
        if self.files.get(&entry.id).is_some_and(|f| f.excluded)
            || self.excluded_paths.contains(&entry.path)
        {
            return Vec::new();
        }
        match entry.meta.kind {
            NodeKind::Text => {
                if self.session.is_loaded(entry.id) {
                    return Vec::new(); // its document sync carries the change
                }
                // An evicted document catches up (and then writes the disk).
                let (_, out) = self.ensure_loaded(entry.id);
                out
            }
            NodeKind::Blob => {
                let current = matches!(
                    self.disk.observe(&entry.path),
                    Ok(Observed::Content(h)) if h == entry.meta.sha256
                );
                if current {
                    Vec::new()
                } else {
                    self.take_entry(&entry, true)
                }
            }
            NodeKind::Dir => Vec::new(),
        }
    }

    /// A peer asked for a file's document that is not loaded here.
    fn on_doc_wanted(&mut self, id: FileId) -> Vec<Output> {
        if self.files.get(&id).is_some_and(|f| f.excluded) {
            return Vec::new(); // never answer for a copy we kept private
        }
        let (state, out) = self.ensure_loaded_with(id, false);
        if state == Load::Unavailable
            && self.role == super::Role::Host
            && !self.files.get(&id).is_some_and(|f| f.excluded)
            && self
                .path_of(id)
                .is_some_and(|p| !self.excluded_paths.contains(&p))
        {
            // The file changed on disk since it was scanned and nobody ever
            // loaded it: its seed is gone, so it becomes a new node.
            return self.replace_node(id);
        }
        out
    }

    // -- blobs -----------------------------------------------------------------

    fn on_blob_requested(&mut self, peer: u64, hash: BlobHash) -> Vec<Output> {
        let view = self.view();
        let found = view.entries().iter().find(|e| {
            e.meta.kind == NodeKind::Blob
                && e.meta.sha256 == hash
                && !self.excluded_paths.contains(&e.path)
                && self.files.get(&e.id).is_none_or(|f| !f.excluded)
        });
        let Some(entry) = found else {
            self.session.decline_blob(hash);
            return Vec::new();
        };
        let cap = self
            .disk
            .policy()
            .max_text()
            .max(crate::pathsec::MAX_BLOB_BYTES);
        let path = self.disk.policy().root().path().join(&entry.path);
        match read_capped(&path, cap) {
            Ok(bytes)
                if blob_hash(&bytes) == hash && self.disk.policy().is_shareable(&entry.path) =>
            {
                self.blob_served.entry(hash).or_default().insert(peer);
                self.session.offer_blob(self.now, hash, bytes)
            }
            _ => {
                self.session.decline_blob(hash);
                Vec::new()
            }
        }
    }

    fn on_blob_received(&mut self, hash: BlobHash, data: &[u8]) {
        let Some(ids) = self.arriving_blobs.remove(&hash) else {
            return;
        };
        for id in ids {
            let Some(path) = self.arrival_path(id) else {
                continue;
            };
            if self.excluded_paths.contains(&path) {
                continue;
            }
            let replace = std::mem::take(&mut self.file(id).replace_on_arrival);
            let result = if replace {
                self.disk.take_host(&path, data).map(|_| ())
            } else {
                self.disk.write_blob(&path, data).map(|_| ())
            };
            if let Err(e) = result {
                self.skip(&path, &e.to_string());
            }
        }
    }

    fn on_blob_failed(&mut self, hash: BlobHash) {
        let ids = self.arriving_blobs.remove(&hash).unwrap_or_default();
        let view = self.view();
        for id in ids {
            if let Some(path) = view.path_of(id) {
                let path = path.to_owned();
                self.skip(&path, "nobody could send this file");
            }
        }
    }

    // -- people ----------------------------------------------------------------

    /// The remote peers with the file each has in front of them.
    fn peer_list(&mut self) -> Vec<PeerAt> {
        let view = self.view();
        self.session
            .presence()
            .iter_v2()
            .map(|p| PeerAt {
                peer: p.presence.peer,
                name: p.presence.name.clone(),
                slot: p.presence.slot,
                path: p.doc.and_then(|d| view.path_of(d).map(str::to_owned)),
            })
            .collect()
    }

    /// Joined and left notices, the peers event and the status.
    pub(super) fn peers_changed(&mut self) {
        self.peers_dirty = false;
        let list = self.peer_list();
        let now: std::collections::BTreeMap<u64, String> =
            list.iter().map(|p| (p.peer, p.name.clone())).collect();
        let joined: Vec<u64> = now
            .keys()
            .filter(|p| !self.known_peers.contains_key(p))
            .copied()
            .collect();
        let left: Vec<u64> = self
            .known_peers
            .keys()
            .filter(|p| !now.contains_key(p))
            .copied()
            .collect();
        self.known_peers = now;
        for peer in joined {
            self.notice("joined", &peer.to_string(), None);
        }
        for peer in left {
            self.notice("left", &peer.to_string(), None);
        }
        let unsynced = self.status.lock().unsynced_files;
        let changed = list != self.last_peers || self.last_unsynced != Some(unsynced);
        self.status.lock().peers.clone_from(&list);
        if changed {
            self.last_unsynced = Some(unsynced);
            self.emit(
                "live.peers",
                json!({
                    "peers": list.iter().map(PeerAt::to_json).collect::<Vec<_>>(),
                    "unsynced_files": unsynced,
                }),
            );
            self.last_peers = list;
        }
    }

    pub(super) fn notice(&self, kind: &str, peer: &str, path: Option<&str>) {
        let mut body = json!({ "kind": kind, "peer": peer });
        if let (Some(path), Some(object)) = (path, body.as_object_mut()) {
            object.insert("path".to_owned(), json!(path));
        }
        self.emit("live.notice", body);
    }

    /// Resolved remote carets of every file open in a tab, at most every
    /// [`PRESENCE_GAP_MS`](super::PRESENCE_GAP_MS) and only when something moved.
    pub(super) fn emit_presence(&mut self) {
        if !self.presence_dirty
            || self.now.saturating_sub(self.last_presence_ms) < super::PRESENCE_GAP_MS
        {
            return;
        }
        self.presence_dirty = false;
        self.last_presence_ms = self.now;
        let attached: BTreeSet<FileId> = self.ropes.keys().copied().collect();
        for id in attached {
            let Some(path) = self.open_path_of(id) else {
                continue;
            };
            let Some(rope) = self.ropes.get(&id) else {
                continue;
            };
            let selections: Vec<Value> = self
                .session
                .remote_file_selections(id, rope)
                .into_iter()
                .map(|s| {
                    json!({
                        "peer": s.peer.to_string(),
                        "name": s.name,
                        "slot": s.slot,
                        "anchor": s.anchor,
                        "head": s.head,
                    })
                })
                .collect();
            let version = self.files.get(&id).map_or(0, |f| f.version);
            let payload = json!({ "path": path, "version": version, "selections": selections });
            if self.last_presence.get(&id) == Some(&payload) {
                continue;
            }
            self.last_presence.insert(id, payload.clone());
            self.emit("live.presence", payload);
        }
    }
}

fn parent_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}
