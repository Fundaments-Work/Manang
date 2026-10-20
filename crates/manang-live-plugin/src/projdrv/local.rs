//! What this side does to the project: edits in tabs, files on disk, and
//! the commands that manage the file set.
//!
//! * `push` / `selection` / `snapshot` / `attach` / `detach` serve the web
//!   view for files open in a tab (the same version protocol as a single-file
//!   session, per file).
//! * [`Driver::fs_poll`] reads `project.changed` events from the kernel bus,
//!   lets the [`FsBridge`](crate::fsbridge::FsBridge) decide what each disk
//!   change means, and turns the resulting intents into tree operations and
//!   text edits. Its echo suppression comes from the same
//!   [`DiskBridge`](crate::disk::DiskBridge) that wrote the file.
//! * A large rewrite (a checkout) pauses: the thread reports it with a
//!   `live.paused` event and holds the changes until `live.resync` with
//!   `paused: "share"` or `"discard"`.

use manang_editor::DocId;
use manang_kernel::Payload;
use manang_live::TextCrdt;
use manang_live::bridge::{LiveDoc, TextTarget};
use manang_live::envelope::seed_fingerprint;
use manang_live::project::{FileId, NodeKind, NodeMeta, ProjectView};
use manang_live::session::{Output, Session};
use ropey::Rope;
use serde_json::json;

use super::files::Load;
use super::{ATTACH_WAIT_MS, Driver, FileNode, META_QUIET_MS, PendingAttach, Phase, Reply};
use crate::disk::{Observed, read_capped, sha256};
use crate::driver::{Change, PushOutcome};
use crate::error::LiveHostError;
use crate::fsbridge::{Intent, SharedNode, SharedView, parse_changed_event};
use crate::scan::{ensure_dir, scan_dir, split};

/// The shared project as the tree bridge sees it.
struct Shared<'a> {
    view: &'a ProjectView,
    session: &'a Session<manang_live::LoroEngine, manang_live::envelope::OsEntropy>,
    files: &'a std::collections::BTreeMap<FileId, super::FileState>,
}

impl Shared<'_> {
    fn usable(&self, id: FileId) -> bool {
        self.session.is_loaded(id)
            && (self.files.get(&id).is_some_and(|f| f.local_origin)
                || self.session.is_doc_synced(id))
    }
}

impl SharedView for Shared<'_> {
    fn node(&self, path: &str) -> Option<SharedNode> {
        let id = self.view.id_at_path(path)?;
        let entry = self.view.get(id)?;
        let sha256 = if entry.meta.kind == NodeKind::Text && self.usable(id) {
            self.session
                .file_doc(id)
                .map_or(entry.meta.sha256, |d| sha256(d.engine().text().as_bytes()))
        } else {
            entry.meta.sha256
        };
        Some(SharedNode {
            kind: entry.meta.kind,
            sha256,
        })
    }

    fn text(&self, path: &str) -> Option<String> {
        let id = self.view.id_at_path(path)?;
        let entry = self.view.get(id)?;
        (entry.meta.kind == NodeKind::Text && self.usable(id))
            .then(|| self.session.file_doc(id).map(|d| d.engine().text()))
            .flatten()
    }
}

impl Driver {
    /// The path the tab of `id` shows.
    pub(super) fn open_path_of(&mut self, id: FileId) -> Option<String> {
        match self.files.get(&id).and_then(|f| f.open_path.clone()) {
            Some(path) => Some(path),
            None => self.path_of(id),
        }
    }

    /// The file open in a tab at `path` (also after a remote delete removed
    /// it from the tree).
    fn find_attached(&mut self, path: &str) -> Option<FileId> {
        if let Some(id) = self.view().id_at_path(path)
            && self.files.get(&id).is_some_and(|f| f.attached)
        {
            return Some(id);
        }
        self.files
            .iter()
            .find(|(_, f)| f.attached && f.open_path.as_deref() == Some(path))
            .map(|(id, _)| *id)
    }

    fn require_live(&self) -> Result<(), LiveHostError> {
        if self.closed {
            return Err(LiveHostError::NotLive);
        }
        match self.phase {
            Phase::Live => Ok(()),
            Phase::AwaitingManifest => Err(LiveHostError::Waiting),
            Phase::AwaitingResolve(_) => Err(LiveHostError::Invalid(
                "Choose what to do with the files first.".to_owned(),
            )),
        }
    }

    // -- the web view ----------------------------------------------------------

    pub(super) fn push(
        &mut self,
        path: &str,
        version: u64,
        changes: &[Change],
    ) -> Result<PushOutcome, LiveHostError> {
        self.require_live()?;
        let id = self
            .find_attached(path)
            .ok_or_else(|| LiveHostError::NotInSession(path.to_owned()))?;
        let current = self.files.get(&id).map_or(0, |f| f.version);
        if version != current {
            return Ok(PushOutcome::Stale(current));
        }
        let mut result = Ok(());
        for change in changes {
            if let Err(e) = self.apply_local(id, change) {
                result = Err(e);
                break;
            }
        }
        // Even a half-applied batch is a step: the view re-reads the file
        // after an error, and the version must then name what it reads.
        let version = {
            let state = self.file(id);
            state.version += 1;
            state.version
        };
        result?;
        self.emit(
            "live.remote",
            json!({ "path": path, "version": version, "ack": true, "changes": [] }),
        );
        let flushed = self.session.flush_doc(id);
        self.perform(flushed);
        self.meta_due.insert(id, self.now + META_QUIET_MS);
        self.touch(id);
        self.presence_dirty = true;
        Ok(PushOutcome::Accepted(version))
    }

    fn apply_local(&mut self, id: FileId, change: &Change) -> Result<(), LiveHostError> {
        let end = change
            .start
            .checked_add(change.delete)
            .ok_or_else(|| LiveHostError::Invalid("change is out of range".to_owned()))?;
        let rope = self
            .ropes
            .get_mut(&id)
            .ok_or_else(|| LiveHostError::NotInSession("this file".to_owned()))?;
        let len = rope.len_utf16();
        if end > len {
            return Err(LiveHostError::Invalid(format!(
                "change {}..{end} is outside the document ({len} units)",
                change.start
            )));
        }
        let from = rope.utf16_cu_to_char(change.start);
        let to = rope.utf16_cu_to_char(end);
        let doc = self
            .session
            .file_doc_mut(id)
            .ok_or_else(|| LiveHostError::NotInSession("this file".to_owned()))?;
        doc.local_replace_utf16(rope, change.start, end, &change.insert)?;
        if let Some(editor_doc) = self.files.get(&id).and_then(|f| f.editor_doc) {
            self.editor
                .replace_silent(Some(editor_doc), from, to, &change.insert)
                .map_err(|e| LiveHostError::Editor(e.to_string()))?;
        }
        Ok(())
    }

    pub(super) fn selection(
        &mut self,
        path: &str,
        version: u64,
        selection: Option<(usize, usize)>,
    ) {
        if self.require_live().is_err() {
            return;
        }
        let Some(id) = self.find_attached(path) else {
            return;
        };
        if self.files.get(&id).map_or(0, |f| f.version) != version {
            return;
        }
        let Some(rope) = self.ropes.get(&id) else {
            return;
        };
        let out = self.session.set_file_selection(id, rope, selection);
        self.perform(out);
    }

    pub(super) fn snapshot(&mut self, path: &str) -> Result<(u64, String), LiveHostError> {
        self.require_live()?;
        let id = match self.find_attached(path) {
            Some(id) => id,
            None => self.id_of(path)?,
        };
        let version = self.files.get(&id).map_or(0, |f| f.version);
        if let Some(rope) = self.ropes.get(&id) {
            return Ok((version, rope.to_string()));
        }
        match self.doc_text(id) {
            Some(text) if self.doc_usable(id) => Ok((version, text)),
            _ => Err(LiveHostError::Fetching(path.to_owned())),
        }
    }

    /// Start editing a file: load it, wait for it if it is still being
    /// fetched, give the view its text and version.
    pub(super) fn attach(&mut self, path: String, doc: Option<DocId>, reply: Reply<(u64, String)>) {
        match self.begin_attach(&path) {
            Err(e) => {
                let _ = reply.send(Err(e));
            }
            Ok(id) => {
                if self.doc_usable(id) && !self.arriving.contains(&id) {
                    let _ = reply.send(self.finish_attach(id, &path, doc));
                } else {
                    self.pending_attach.push(PendingAttach {
                        id,
                        path,
                        doc,
                        deadline_ms: self.now + ATTACH_WAIT_MS,
                        reply,
                    });
                }
            }
        }
    }

    fn begin_attach(&mut self, path: &str) -> Result<FileId, LiveHostError> {
        self.require_live()?;
        let mut id = self.id_of(path)?;
        let view = self.view();
        let kind = view.get(id).map(|e| e.meta.kind);
        if kind != Some(NodeKind::Text) {
            return Err(LiveHostError::NotText(path.to_owned()));
        }
        if self.files.get(&id).is_some_and(|f| f.excluded) || self.excluded_paths.contains(path) {
            return Err(LiveHostError::Excluded(path.to_owned()));
        }
        let (state, out) = self.ensure_loaded(id);
        self.perform(out);
        if state == Load::Unavailable {
            if self.role == super::Role::Host {
                let out = self.replace_node(id);
                self.perform(out);
                id = self.id_of(path)?;
            } else {
                return Err(LiveHostError::Fetching(path.to_owned()));
            }
        }
        Ok(id)
    }

    /// Attaches whose file arrived, or gave up waiting.
    pub(super) fn finish_attaches(&mut self) {
        if self.pending_attach.is_empty() {
            return;
        }
        let waiting = std::mem::take(&mut self.pending_attach);
        for pending in waiting {
            let gone = self.view().get(pending.id).is_none();
            if gone {
                let _ = pending
                    .reply
                    .send(Err(LiveHostError::NotInSession(pending.path.clone())));
            } else if self.doc_usable(pending.id) && !self.arriving.contains(&pending.id) {
                let answer = self.finish_attach(pending.id, &pending.path, pending.doc);
                let _ = pending.reply.send(answer);
            } else if self.now >= pending.deadline_ms {
                let _ = pending
                    .reply
                    .send(Err(LiveHostError::Fetching(pending.path.clone())));
            } else {
                self.pending_attach.push(pending);
            }
        }
    }

    fn finish_attach(
        &mut self,
        id: FileId,
        path: &str,
        doc: Option<DocId>,
    ) -> Result<(u64, String), LiveHostError> {
        let text = self
            .doc_text(id)
            .ok_or_else(|| LiveHostError::NotInSession(path.to_owned()))?;
        let editor_doc = doc.or_else(|| self.editor_doc_for(path));
        if let Some(editor_doc) = editor_doc
            && let Ok(shown) = self.editor.text(Some(editor_doc))
            && shown != text
        {
            // The kernel's shared copy follows the room, not the other way.
            self.editor
                .replace_silent(Some(editor_doc), 0, shown.chars().count(), &text)
                .map_err(|e| LiveHostError::Editor(e.to_string()))?;
        }
        self.ropes.insert(id, Rope::from_str(&text));
        let version = {
            let state = self.file(id);
            state.attached = true;
            state.open_path = Some(path.to_owned());
            state.editor_doc = editor_doc;
            state.version
        };
        self.disk.set_open(path);
        self.residency.pin(id);
        self.touch(id);
        self.focus = Some(id);
        self.announce_focus();
        Ok((version, text))
    }

    /// The kernel editor document that shows `path`, if one is open.
    fn editor_doc_for(&self, path: &str) -> Option<DocId> {
        let abs = self.disk.policy().root().path().join(path);
        self.editor
            .list()
            .into_iter()
            .find(|d| {
                d.path
                    .as_deref()
                    .is_some_and(|p| std::path::Path::new(p) == abs)
            })
            .map(|d| d.id)
    }

    pub(super) fn detach(&mut self, path: &str) -> Result<bool, LiveHostError> {
        let Some(id) = self.find_attached(path) else {
            return Ok(false);
        };
        self.ropes.remove(&id);
        self.last_presence.remove(&id);
        {
            let state = self.file(id);
            state.attached = false;
            state.open_path = None;
            state.editor_doc = None;
        }
        let evicted = self.residency.unpin(&id);
        self.evict_all(evicted);
        let closed = self
            .disk
            .close(path)
            .map_err(|e| LiveHostError::Io(e.to_string()))?;
        // The tab was the writer while it was open; the room's text is the
        // file's now (nothing is written when the disk already has it).
        if closed.trashed.is_none() {
            self.queue_disk(id);
        }
        if let Some(trash) = closed.trashed {
            self.deleted_trash.insert(path.to_owned(), trash);
            self.files.remove(&id);
        }
        if self.focus == Some(id) {
            self.focus = self.ropes.keys().next_back().copied();
        }
        self.announce_focus();
        Ok(true)
    }

    /// Tell the room which file this side is editing and which are open.
    pub(super) fn announce_focus(&mut self) {
        let open: Vec<FileId> = self.ropes.keys().copied().collect();
        let out = self.session.set_focus(self.focus, open);
        self.perform(out);
    }

    // -- managing the file set -------------------------------------------------

    pub(super) fn file_list(&mut self) -> Vec<FileNode> {
        let view = self.view();
        let mut list = Vec::new();
        for entry in view.entries() {
            if entry.meta.kind == NodeKind::Dir
                || self.files.get(&entry.id).is_some_and(|f| f.excluded)
                || self.excluded_paths.contains(&entry.path)
            {
                continue;
            }
            list.push(FileNode {
                path: entry.path.clone(),
                binary: entry.meta.kind != NodeKind::Text,
                peers: self
                    .session
                    .peers_by_file(entry.id)
                    .into_iter()
                    .map(|p| p.peer)
                    .collect(),
            });
        }
        list
    }

    pub(super) fn exclude(&mut self, path: &str) -> Result<bool, LiveHostError> {
        self.require_live()?;
        if let Some(id) = self.view().id_at_path(path) {
            if self.files.get(&id).is_some_and(|f| f.attached) {
                self.detach(path)?;
            }
            self.exclude_local(id, path);
        } else {
            self.excluded_paths.insert(path.to_owned());
            self.fs.exclude(path);
        }
        Ok(true)
    }

    /// Put a file back: a node of the room is fetched (the host's version
    /// wins and a differing local copy goes to the trash); a private
    /// guest-only file is shared.
    pub(super) fn include(&mut self, path: &str) -> Result<bool, LiveHostError> {
        self.require_live()?;
        let node = self.view().id_at_path(path);
        let was_excluded = self.excluded_paths.remove(path)
            || node.is_some_and(|id| self.files.get(&id).is_some_and(|f| f.excluded));
        if !was_excluded {
            return Ok(false);
        }
        self.fs.include(path);
        let out = match node {
            Some(id) => {
                self.file(id).excluded = false;
                let Some(entry) = self.view().get(id).cloned() else {
                    return Ok(false);
                };
                let differs = matches!(
                    self.disk.observe(path),
                    Ok(Observed::Content(h)) if h != entry.meta.sha256
                );
                self.take_entry(&entry, differs)
            }
            None => self.create_from_disk(path)?,
        };
        self.perform(out);
        Ok(true)
    }

    /// Bring back a file a peer deleted: from the tab that is still open on
    /// it, or from the trash.
    pub(super) fn restore(&mut self, path: &str) -> Result<bool, LiveHostError> {
        self.require_live()?;
        let open = self
            .files
            .iter()
            .find(|(_, f)| f.attached && f.open_path.as_deref() == Some(path))
            .map(|(id, _)| *id);
        let out = if let Some(old) = open
            && self.view().id_at_path(path).is_none()
        {
            self.disk.cancel_delete(path);
            self.fs.forget_deleted(path);
            let text = self
                .ropes
                .get(&old)
                .map(Rope::to_string)
                .ok_or_else(|| LiveHostError::NotDeleted(path.to_owned()))?;
            let out = self.create_node(
                path,
                NodeKind::Text,
                text.len() as u64,
                sha256(text.as_bytes()),
                Some(&text),
            )?;
            self.rebind(old, path)?;
            out
        } else if let Some(trash) = self.deleted_trash.get(path).cloned() {
            let abs = self.disk.policy().root().path().join(&trash);
            let bytes = read_capped(&abs, crate::pathsec::MAX_BLOB_BYTES)
                .map_err(|e| LiveHostError::NotDeleted(format!("{path} ({e})")))?;
            self.disk
                .take_host(path, &bytes)
                .map_err(|e| LiveHostError::BadPath(e.to_string()))?;
            self.deleted_trash.remove(path);
            self.create_from_disk(path)?
        } else {
            return Err(LiveHostError::NotDeleted(path.to_owned()));
        };
        self.perform(out);
        self.emit(
            "live.tree",
            json!({ "ops": [{ "op": "create", "path": path, "by": self.peer.to_string() }] }),
        );
        Ok(true)
    }

    /// A restored file got a new node: the open tab follows it.
    fn rebind(&mut self, old: FileId, path: &str) -> Result<(), LiveHostError> {
        let new = self.id_of(path)?;
        if let Some(rope) = self.ropes.remove(&old) {
            self.ropes.insert(new, rope);
        }
        if let Some(mut state) = self.files.remove(&old) {
            let local_origin = self.files.get(&new).is_some_and(|f| f.local_origin);
            state.local_origin = local_origin || state.local_origin;
            self.files.insert(new, state);
        }
        if self.focus == Some(old) {
            self.focus = Some(new);
        }
        self.residency.unpin(&old);
        self.residency.remove(&old);
        self.residency.pin(new);
        self.announce_focus();
        Ok(())
    }

    /// `live.resync`: answer a held large change, or ask again for the manifest.
    pub(super) fn resync(&mut self, paused: Option<bool>) -> Result<(), LiveHostError> {
        if self.closed {
            return Err(LiveHostError::NotLive);
        }
        let Some(share) = paused else {
            if self.fs.is_paused() {
                return Err(LiveHostError::Paused);
            }
            if matches!(
                self.phase,
                Phase::AwaitingManifest | Phase::AwaitingResolve(_)
            ) {
                let out = self.session.request_manifest(self.now);
                self.perform(out);
            }
            return Ok(());
        };
        if !self.fs.is_paused() {
            return Ok(());
        }
        let view = self.view();
        let resolution = {
            let shared = Shared {
                view: &view,
                session: &self.session,
                files: &self.files,
            };
            self.fs.resolve_pause(share, &self.disk, &shared)
        };
        self.status.lock().paused = None;
        let mut out = self.apply_intents(resolution.intents);
        for path in resolution.revert {
            out.extend(self.revert_path(&path));
        }
        self.perform(out);
        Ok(())
    }

    /// Discarding a held change: put the shared state back on disk (the
    /// local copy goes to the trash first).
    fn revert_path(&mut self, path: &str) -> Vec<Output> {
        let Some(id) = self.view().id_at_path(path) else {
            return Vec::new();
        };
        let Some(entry) = self.view().get(id).cloned() else {
            return Vec::new();
        };
        match entry.meta.kind {
            NodeKind::Text => {
                let (state, out) = self.ensure_loaded(id);
                if state == Load::Ready
                    && let Some(text) = self.doc_text(id)
                    && let Err(e) = self.disk.take_host(path, text.as_bytes())
                {
                    self.skip(path, &e.to_string());
                }
                out
            }
            NodeKind::Blob => self.take_entry(&entry, true),
            NodeKind::Dir => Vec::new(),
        }
    }

    // -- the disk --------------------------------------------------------------

    /// Read `project.changed` events and share what they mean.
    pub(super) fn fs_poll(&mut self) {
        let mut changes = Vec::new();
        while let Some(event) = self.fs_events.try_recv() {
            let Payload::Json(body) = event.payload else {
                continue;
            };
            let ours = body
                .get("root")
                .and_then(serde_json::Value::as_str)
                .is_none_or(|root| {
                    let root = std::path::Path::new(root);
                    root == self.disk.policy().root().path()
                        || root
                            .canonicalize()
                            .is_ok_and(|c| c == self.disk.policy().root().path())
                });
            if ours {
                changes.extend(parse_changed_event(&body));
            }
        }
        if changes.is_empty() || !matches!(self.phase, Phase::Live) {
            return;
        }
        if changes.iter().any(|c| c.path == ".gitignore") {
            self.disk.policy_mut().reload_ignore();
        }
        let view = self.view();
        let intents = {
            let shared = Shared {
                view: &view,
                session: &self.session,
                files: &self.files,
            };
            self.fs.process(&changes, &self.disk, &shared)
        };
        let out = self.apply_intents(intents);
        self.perform(out);
    }

    /// Removals that found no matching creation in time become deletes.
    pub(super) fn fs_tick(&mut self) -> Vec<Output> {
        let intents = self.fs.tick();
        if intents.is_empty() {
            return Vec::new();
        }
        self.apply_intents(intents)
    }

    fn apply_intents(&mut self, intents: Vec<Intent>) -> Vec<Output> {
        let mut out = Vec::new();
        for intent in intents {
            match self.apply_intent(intent) {
                Ok(more) => out.extend(more),
                Err(e) => {
                    let message = e.to_string();
                    self.skip("", &message);
                }
            }
        }
        out.extend(self.session.flush_project());
        out
    }

    fn apply_intent(&mut self, intent: Intent) -> Result<Vec<Output>, LiveHostError> {
        match intent {
            Intent::CreateDir { path } => {
                let project = self
                    .session
                    .project_mut()
                    .ok_or(LiveHostError::NotProject)?;
                ensure_dir(project, &path)?;
                // The watcher may attach to a new directory after its first
                // files were written (and a moved-in folder reports only
                // itself): look inside so nothing is missed.
                let mut out = Vec::new();
                let scan = scan_dir(self.disk.policy(), &path)
                    .map_err(|e| LiveHostError::Io(e.to_string()))?;
                for item in scan.items {
                    if self.view().id_at_path(&item.rel).is_some() {
                        continue;
                    }
                    if item.kind == NodeKind::Dir {
                        let project = self
                            .session
                            .project_mut()
                            .ok_or(LiveHostError::NotProject)?;
                        ensure_dir(project, &item.rel)?;
                    } else {
                        match self.create_from_disk(&item.rel) {
                            Ok(more) => out.extend(more),
                            Err(e) => self.skip(&item.rel, &e.to_string()),
                        }
                    }
                }
                Ok(out)
            }
            Intent::CreateFile {
                path,
                kind,
                size,
                sha256,
                text,
            } => {
                if self.view().id_at_path(&path).is_some() {
                    return Ok(Vec::new());
                }
                self.create_node(&path, kind, size, sha256, text.as_deref())
            }
            Intent::Edit { path, edits, .. } => {
                let id = self.id_of(&path)?;
                let Some(doc) = self.session.file_doc_mut(id) else {
                    return Ok(Vec::new());
                };
                for edit in edits.iter().rev() {
                    doc.record_local_bytes(edit.start, edit.end - edit.start, &edit.insert)?;
                }
                self.meta_due.insert(id, self.now + META_QUIET_MS);
                self.touch(id);
                Ok(self.session.flush_doc(id))
            }
            Intent::ReplaceText { path, text, .. } => {
                let id = self.id_of(&path)?;
                let (state, mut out) = self.ensure_loaded(id);
                match state {
                    Load::Ready => out.extend(self.apply_text(id, &text)),
                    Load::Fetching => {
                        self.pending_text.insert(id, text);
                    }
                    Load::Unavailable => out.extend(self.replace_node(id)),
                }
                Ok(out)
            }
            Intent::SetBlob { path, size, sha256 } => {
                let id = self.id_of(&path)?;
                let project = self
                    .session
                    .project_mut()
                    .ok_or(LiveHostError::NotProject)?;
                project.set_content(id, size, sha256, [0; 16])?;
                Ok(Vec::new())
            }
            Intent::Delete { path } => {
                let id = self.id_of(&path)?;
                // A directory takes everything below it along.
                let prefix = format!("{path}/");
                let below: Vec<FileId> = self
                    .view()
                    .entries()
                    .iter()
                    .filter(|e| e.path.starts_with(&prefix))
                    .map(|e| e.id)
                    .collect();
                for gone in below.into_iter().chain([id]) {
                    self.drop_doc(gone);
                    self.files.remove(&gone);
                    self.arriving.remove(&gone);
                    self.pending_text.remove(&gone);
                    self.meta_due.remove(&gone);
                    self.dirty_docs.remove(&gone);
                }
                let project = self
                    .session
                    .project_mut()
                    .ok_or(LiveHostError::NotProject)?;
                project.delete(id)?;
                Ok(Vec::new())
            }
            Intent::Rename { from, to } => {
                let id = self.id_of(&from)?;
                let (old_parent, old_name) = split(&from);
                let (new_parent, new_name) = split(&to);
                let project = self
                    .session
                    .project_mut()
                    .ok_or(LiveHostError::NotProject)?;
                if old_parent != new_parent {
                    let parent = match new_parent {
                        Some(dir) => Some(ensure_dir(project, dir)?),
                        None => None,
                    };
                    project.move_to(id, parent)?;
                }
                if old_name != new_name {
                    project.rename(id, new_name)?;
                }
                if let Some(state) = self.files.get_mut(&id)
                    && state.attached
                {
                    state.open_path = Some(to);
                }
                Ok(Vec::new())
            }
            Intent::OpenChanged { .. } => Ok(Vec::new()),
            Intent::Skipped { path, reason } => {
                self.skip(&path, &reason);
                Ok(Vec::new())
            }
            Intent::Pause(info) => {
                let body = json!({
                    "files": info.files,
                    "changed_lines": info.changed_lines,
                    "total_lines": info.total_lines,
                });
                self.status.lock().paused = Some(body.clone());
                self.emit("live.paused", body);
                Ok(Vec::new())
            }
        }
    }

    /// Create the node of a local file (and its document, for text), under
    /// directories created as needed.
    pub(super) fn create_node(
        &mut self,
        path: &str,
        kind: NodeKind,
        size: u64,
        sha256: [u8; 32],
        text: Option<&str>,
    ) -> Result<Vec<Output>, LiveHostError> {
        self.disk
            .policy()
            .check_rel(path)
            .map_err(|e| LiveHostError::BadPath(e.to_string()))?;
        let (parent_path, name) = split(path);
        let seed_fp = text.map_or([0; 16], seed_fingerprint);
        let project = self
            .session
            .project_mut()
            .ok_or(LiveHostError::NotProject)?;
        let parent = match parent_path {
            Some(dir) => Some(ensure_dir(project, dir)?),
            None => None,
        };
        let id = project.create(parent, &NodeMeta::file(name, kind, size, sha256, seed_fp))?;
        let mut out = self.session.flush_project();
        if let Some(text) = text {
            let doc = LiveDoc::seeded(self.peer, text)?;
            out.extend(self.session.provide_doc(self.now, id, doc, false));
            let state = self.file(id);
            state.local_origin = true;
            state.synced_sha = None;
            self.touch(id);
        }
        Ok(out)
    }

    /// Create the node of the file at `path`, reading it from disk.
    pub(super) fn create_from_disk(&mut self, path: &str) -> Result<Vec<Output>, LiveHostError> {
        let abs = self
            .disk
            .policy()
            .check_write(path)
            .map_err(|e| LiveHostError::BadPath(e.to_string()))?
            .abs;
        let bytes = read_capped(&abs, crate::pathsec::MAX_BLOB_BYTES)
            .map_err(|e| LiveHostError::Io(e.to_string()))?;
        let (kind, text) = crate::fsbridge::classify(&bytes);
        let (kind, text) = match (kind, text) {
            (NodeKind::Text, Some(t))
                if self
                    .disk
                    .policy()
                    .check_size(NodeKind::Text, bytes.len() as u64)
                    .is_ok() =>
            {
                (NodeKind::Text, Some(t))
            }
            _ => (NodeKind::Blob, None),
        };
        self.create_node(
            path,
            kind,
            bytes.len() as u64,
            sha256(&bytes),
            text.as_deref(),
        )
    }

    /// A host file whose seed is gone (it changed on disk before anyone
    /// loaded it): it becomes a new node, so nobody's history is mixed with
    /// text it was never part of.
    pub(super) fn replace_node(&mut self, id: FileId) -> Vec<Output> {
        let Some(path) = self.path_of(id) else {
            return Vec::new();
        };
        self.drop_doc(id);
        self.files.remove(&id);
        if let Some(project) = self.session.project_mut() {
            let _ = project.delete(id);
        }
        match self.create_from_disk(&path) {
            Ok(out) => out,
            Err(e) => {
                self.skip(&path, &e.to_string());
                self.session.flush_project()
            }
        }
    }
}
