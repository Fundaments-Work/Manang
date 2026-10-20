//! Tree bridge: turns file system changes into tree and content intents.
//!
//! The project watcher publishes `project.changed` events
//! (`{root, changes: [{path, kind, from?}]}`; see
//! `manang_project::watch`). [`FsBridge`] decides what each change means for
//! the shared project and returns [`Intent`]s; the session owner (package P4)
//! turns intents into tree operations and text edits. The bridge itself
//! writes nothing and holds no CRDT: it sees the shared project through the
//! [`SharedView`] trait and the disk through [`DiskView`] (implemented by
//! [`DiskBridge`]), so it is unit-testable with a temp dir and a fake clock.
//!
//! # Rules
//!
//! * **Echo and no-ops are dropped.** A change whose observed state is what
//!   the [`DiskBridge`] just wrote ([`DiskView::is_echo`]), whose content hash
//!   equals the shared node's, or whose file is waiting for a debounced
//!   remote write ([`DiskView::is_behind`]: the disk is *behind* the shared
//!   state, a diff would undo the remote edit) produces nothing.
//! * **Renames and moves.** A `renamed` change with `from` of a tracked file
//!   is [`Intent::Rename`]. A removal is parked for [`MOVE_WINDOW_MS`] (2 s):
//!   a file created in that window with the same SHA-256 (and size > 0) makes
//!   the pair a rename, otherwise [`FsBridge::tick`] emits the
//!   [`Intent::Delete`].
//! * **External edits** of a text file become [`Intent::Edit`]: a minimal
//!   list of byte-range replacements against the shared text (line diff,
//!   common prefix and suffix trimmed per hunk), so remote carets survive.
//!   Without the shared text the intent is [`Intent::ReplaceText`]. A file
//!   open in a tab yields only [`Intent::OpenChanged`]: the editor owns it.
//! * **Large rewrites pause and ask.** More than [`Thresholds::max_files`]
//!   changed files in one batch, or more than [`Thresholds::rewrite_pct`] of
//!   the lines of the modified text files changed (when at least
//!   [`Thresholds::min_lines`] lines are involved), such as a `git checkout`,
//!   returns one [`Intent::Pause`] and holds everything until
//!   [`FsBridge::resolve_pause`] (share the changes, or revert the disk to
//!   the shared state).
//! * **Not shareable** paths (dotfiles, `.git`, deny-list, ignored, excluded
//!   with [`FsBridge::exclude`]) never produce intents. A rename out of the
//!   shareable set is a delete, a rename into it a create.
//! * **Remote delete with an open tab** ([`FsBridge::remote_delete`]) keeps
//!   the tab as "deleted by X"; [`FsBridge::restore`] re-creates the node
//!   from the disk copy.

use std::collections::{BTreeMap, BTreeSet};

use manang_live::project::NodeKind;
use serde::Deserialize;
use similar::{Algorithm, DiffTag, capture_diff_slices};

use crate::disk::{Clock, DiskBridge, DiskError, Hash, Observed, read_capped, sha256};
use crate::pathsec::PathError;

/// How long a removal waits for a matching creation before it is a delete.
pub const MOVE_WINDOW_MS: u64 = 2_000;

/// Kind of a watcher change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FsKind {
    /// A new entry.
    Created,
    /// Changed content.
    Modified,
    /// Gone.
    Removed,
    /// Moved (with `from`).
    Renamed,
}

/// One entry of a `project.changed` event.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FsChange {
    /// Project-relative path, `/` separated.
    pub path: String,
    /// What happened.
    pub kind: FsKind,
    /// The old path of a rename.
    #[serde(default)]
    pub from: Option<String>,
}

impl From<&manang_project::watch::Change> for FsChange {
    fn from(c: &manang_project::watch::Change) -> Self {
        use manang_project::watch::ChangeKind;
        Self {
            path: c.path.clone(),
            kind: match c.kind {
                ChangeKind::Created => FsKind::Created,
                ChangeKind::Modified => FsKind::Modified,
                ChangeKind::Removed => FsKind::Removed,
                ChangeKind::Renamed => FsKind::Renamed,
            },
            from: c.from.clone(),
        }
    }
}

/// The changes of a `project.changed` payload; malformed entries are skipped.
#[must_use]
pub fn parse_changed_event(payload: &serde_json::Value) -> Vec<FsChange> {
    payload
        .get("changes")
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// What the shared project knows about a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedNode {
    /// Directory, text file or blob.
    pub kind: NodeKind,
    /// Content hash recorded in the tree (zeros for directories).
    pub sha256: Hash,
}

/// Read access to the shared project (implemented by the session owner).
pub trait SharedView {
    /// The node at a project path, if any.
    fn node(&self, path: &str) -> Option<SharedNode>;
    /// The current shared text of a text file, if its document is loaded or
    /// can be loaded.
    fn text(&self, path: &str) -> Option<String>;
}

/// Read access to the disk and the disk bridge's state.
pub trait DiskView {
    /// Whether a local path may be shared at all.
    fn shareable(&self, path: &str) -> bool;
    /// Gone, a directory, or a file with its content hash.
    fn observe(&self, path: &str) -> Result<Observed, DiskError>;
    /// The file's bytes (capped).
    fn read(&self, path: &str) -> Result<Vec<u8>, DiskError>;
    /// Whether the observation is the echo of our own write.
    fn is_echo(&self, path: &str, observed: Observed) -> bool;
    /// Whether the file is open in a tab.
    fn is_open(&self, path: &str) -> bool;
    /// Whether a remote change to the file has not reached the disk yet.
    fn is_behind(&self, path: &str) -> bool;
    /// Size cap check for a node of `kind`.
    fn check_size(&self, kind: NodeKind, size: u64) -> Result<(), PathError>;
}

impl<C: Clock> DiskView for DiskBridge<C> {
    fn shareable(&self, path: &str) -> bool {
        self.policy().is_shareable(path)
    }

    fn observe(&self, path: &str) -> Result<Observed, DiskError> {
        DiskBridge::observe(self, path)
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, DiskError> {
        let cap = self.policy().max_text().max(crate::pathsec::MAX_BLOB_BYTES);
        read_capped(&self.policy().root().path().join(path), cap)
    }

    fn is_echo(&self, path: &str, observed: Observed) -> bool {
        DiskBridge::is_echo(self, path, observed)
    }

    fn is_open(&self, path: &str) -> bool {
        DiskBridge::is_open(self, path)
    }

    fn is_behind(&self, path: &str) -> bool {
        DiskBridge::is_behind(self, path)
    }

    fn check_size(&self, kind: NodeKind, size: u64) -> Result<(), PathError> {
        self.policy().check_size(kind, size)
    }
}

/// One replacement in a text: bytes `start..end` of the *old* text become
/// `insert`. A list of edits is sorted, non-overlapping and expressed against
/// the old text; apply from the last to the first (or use [`apply_edits`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// First replaced byte (a char boundary of the old text).
    pub start: usize,
    /// One past the last replaced byte.
    pub end: usize,
    /// Replacement text.
    pub insert: String,
}

/// Why a large rewrite was held back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PauseInfo {
    /// Files touched in the batch.
    pub files: usize,
    /// Lines changed (removed plus inserted) in modified text files.
    pub changed_lines: usize,
    /// Lines involved (old plus new) in modified text files.
    pub total_lines: usize,
}

/// What the shared project should do about a disk change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// A new directory.
    CreateDir {
        /// Project path.
        path: String,
    },
    /// A new file. `text` is the content of a text file (blobs are read by
    /// path when sent).
    CreateFile {
        /// Project path.
        path: String,
        /// Text or blob.
        kind: NodeKind,
        /// Size in bytes.
        size: u64,
        /// Content hash.
        sha256: Hash,
        /// UTF-8 content of a text file.
        text: Option<String>,
    },
    /// A minimal edit of a text file's shared document.
    Edit {
        /// Project path.
        path: String,
        /// Hash of the new content.
        sha256: Hash,
        /// Replacements against the shared text.
        edits: Vec<TextEdit>,
    },
    /// The shared text is not available: replace it entirely.
    ReplaceText {
        /// Project path.
        path: String,
        /// Hash of the new content.
        sha256: Hash,
        /// The whole new text.
        text: String,
    },
    /// New content of a blob (last writer wins).
    SetBlob {
        /// Project path.
        path: String,
        /// Size in bytes.
        size: u64,
        /// Content hash.
        sha256: Hash,
    },
    /// A node was deleted (a directory takes its subtree with it).
    Delete {
        /// Project path.
        path: String,
    },
    /// A node was renamed or moved.
    Rename {
        /// Old path.
        from: String,
        /// New path.
        to: String,
    },
    /// A file open in a tab changed on disk; the editor handles it.
    OpenChanged {
        /// Project path.
        path: String,
    },
    /// A change that cannot be shared (too large, unreadable).
    Skipped {
        /// Project path.
        path: String,
        /// Why.
        reason: String,
    },
    /// A large rewrite is held until [`FsBridge::resolve_pause`].
    Pause(PauseInfo),
}

/// When a batch of changes counts as a large rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    /// More changed files than this pauses.
    pub max_files: usize,
    /// More than this percentage of lines changed pauses...
    pub rewrite_pct: usize,
    /// ...when at least this many lines are involved.
    pub min_lines: usize,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            max_files: 25,
            rewrite_pct: 50,
            min_lines: 40,
        }
    }
}

/// Decision about a remote delete of a file the local user has open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteDelete {
    /// Apply it (no tab, or nothing to lose).
    Apply,
    /// Keep the tab open as "deleted by `by`"; offer restore.
    KeepOpen {
        /// The file.
        path: String,
        /// Who deleted it.
        by: String,
    },
}

/// Outcome of [`FsBridge::resolve_pause`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PauseResolution {
    /// Intents of the held changes (when sharing them).
    pub intents: Vec<Intent>,
    /// Tracked paths to rewrite from the shared state (when discarding).
    pub revert: Vec<String>,
}

#[derive(Debug, Clone)]
struct Parked {
    path: String,
    node: SharedNode,
    at_ms: u64,
}

/// Turns watcher changes into intents. See the module docs.
#[derive(Debug)]
pub struct FsBridge<C: Clock> {
    clock: C,
    thresholds: Thresholds,
    parked: Vec<Parked>,
    held: Option<Vec<FsChange>>,
    excluded: BTreeSet<String>,
    deleted_open: BTreeMap<String, String>,
}

impl<C: Clock> FsBridge<C> {
    /// A bridge with default thresholds.
    pub fn new(clock: C) -> Self {
        Self::with_thresholds(clock, Thresholds::default())
    }

    /// A bridge with explicit thresholds.
    pub fn with_thresholds(clock: C, thresholds: Thresholds) -> Self {
        Self {
            clock,
            thresholds,
            parked: Vec::new(),
            held: None,
            excluded: BTreeSet::new(),
            deleted_open: BTreeMap::new(),
        }
    }

    /// Keep a local path (a file, or a directory with everything below it)
    /// private: it never produces intents.
    pub fn exclude(&mut self, path: &str) {
        self.excluded.insert(path.to_owned());
    }

    /// Undo [`exclude`](Self::exclude).
    pub fn include(&mut self, path: &str) {
        self.excluded.remove(path);
    }

    fn is_excluded(&self, path: &str) -> bool {
        self.excluded.iter().any(|e| {
            path == e
                || path
                    .strip_prefix(e.as_str())
                    .is_some_and(|r| r.starts_with('/'))
        })
    }

    /// Whether a large rewrite is waiting for an answer.
    #[must_use]
    pub fn is_paused(&self) -> bool {
        self.held.is_some()
    }

    fn eligible(&self, disk: &impl DiskView, path: &str) -> bool {
        disk.shareable(path) && !self.is_excluded(path)
    }

    /// Interpret a batch of changes.
    pub fn process(
        &mut self,
        changes: &[FsChange],
        disk: &impl DiskView,
        shared: &impl SharedView,
    ) -> Vec<Intent> {
        if let Some(held) = &mut self.held {
            held.extend_from_slice(changes);
            return Vec::new();
        }
        let before = self.parked.clone();
        let (intents, stats) = self.plan(changes, disk, shared);
        if stats.should_pause(&self.thresholds) {
            self.parked = before;
            let info = PauseInfo {
                files: stats.files,
                changed_lines: stats.changed_lines,
                total_lines: stats.total_lines,
            };
            self.held = Some(changes.to_vec());
            return vec![Intent::Pause(info)];
        }
        intents
    }

    /// Emit the deletes of removals that found no matching creation in time.
    pub fn tick(&mut self) -> Vec<Intent> {
        let now = self.clock.now_ms();
        let mut out = Vec::new();
        self.parked.retain(|p| {
            let expired = now.saturating_sub(p.at_ms) > MOVE_WINDOW_MS;
            if expired {
                out.push(Intent::Delete {
                    path: p.path.clone(),
                });
            }
            !expired
        });
        out
    }

    /// Answer a pause: `share` the held changes (they become intents,
    /// whatever their size) or discard them (the disk should be rewritten from
    /// the shared state for the returned tracked paths).
    pub fn resolve_pause(
        &mut self,
        share: bool,
        disk: &impl DiskView,
        shared: &impl SharedView,
    ) -> PauseResolution {
        let Some(held) = self.held.take() else {
            return PauseResolution::default();
        };
        if share {
            let (mut intents, _) = self.plan(&held, disk, shared);
            // Everything held is decided now: removals do not wait for a move.
            intents.extend(self.tick_all());
            return PauseResolution {
                intents,
                revert: Vec::new(),
            };
        }
        let mut revert = BTreeSet::new();
        for c in &held {
            for p in std::iter::once(&c.path).chain(c.from.iter()) {
                if shared.node(p).is_some_and(|n| n.kind != NodeKind::Dir) && self.eligible(disk, p)
                {
                    revert.insert(p.clone());
                }
            }
        }
        PauseResolution {
            intents: Vec::new(),
            revert: revert.into_iter().collect(),
        }
    }

    fn tick_all(&mut self) -> Vec<Intent> {
        self.parked
            .drain(..)
            .map(|p| Intent::Delete { path: p.path })
            .collect()
    }

    /// A remote peer deleted `path`. If the local tab is open with unsaved
    /// work, keep it open as "deleted by `by`" instead of applying.
    pub fn remote_delete(&mut self, path: &str, by: &str, tab_open_dirty: bool) -> RemoteDelete {
        if tab_open_dirty {
            self.deleted_open.insert(path.to_owned(), by.to_owned());
            RemoteDelete::KeepOpen {
                path: path.to_owned(),
                by: by.to_owned(),
            }
        } else {
            RemoteDelete::Apply
        }
    }

    /// Who deleted a file that is still open locally.
    #[must_use]
    pub fn deleted_by(&self, path: &str) -> Option<&str> {
        self.deleted_open.get(path).map(String::as_str)
    }

    /// The tab of a deleted file closed without restoring.
    pub fn forget_deleted(&mut self, path: &str) {
        self.deleted_open.remove(path);
    }

    /// Restore a file kept open as deleted: re-create its node from the copy
    /// on disk.
    pub fn restore(&mut self, path: &str, disk: &impl DiskView) -> Result<Intent, DiskError> {
        let bytes = disk.read(path)?;
        self.deleted_open.remove(path);
        let (kind, text) = classify(&bytes);
        disk.check_size(kind, bytes.len() as u64)
            .map_err(DiskError::Path)?;
        Ok(Intent::CreateFile {
            path: path.to_owned(),
            kind,
            size: bytes.len() as u64,
            sha256: sha256(&bytes),
            text,
        })
    }

    fn plan(
        &mut self,
        changes: &[FsChange],
        disk: &impl DiskView,
        shared: &impl SharedView,
    ) -> (Vec<Intent>, Stats) {
        let now = self.clock.now_ms();
        let mut stats = Stats::default();
        let mut intents = Vec::new();
        let mut renames: Vec<(String, String)> = Vec::new();
        let mut removed: Vec<String> = Vec::new();
        let mut upserts: BTreeSet<String> = BTreeSet::new();

        // Sort the changes into renames, removals and creations/modifications.
        for c in changes {
            match (c.kind, &c.from) {
                (FsKind::Renamed, Some(from)) => {
                    let from_ok = self.eligible(disk, from);
                    let to_ok = self.eligible(disk, &c.path);
                    let tracked = shared.node(from).is_some();
                    match (from_ok && tracked, to_ok) {
                        (true, true) => {
                            if !self.rename_is_echo(disk, &c.path) {
                                renames.push((from.clone(), c.path.clone()));
                            }
                        }
                        (true, false) => removed.push(from.clone()),
                        (false, true) => {
                            upserts.insert(c.path.clone());
                        }
                        (false, false) => {}
                    }
                }
                (FsKind::Removed, _) => {
                    if self.eligible(disk, &c.path) {
                        removed.push(c.path.clone());
                    }
                }
                _ => {
                    if self.eligible(disk, &c.path) {
                        upserts.insert(c.path.clone());
                    }
                }
            }
        }

        for (from, to) in renames {
            stats.files += 1;
            self.parked.retain(|p| p.path != from && p.path != to);
            intents.push(Intent::Rename { from, to });
        }

        // Removals: a path that exists again (atomic save) is an upsert; a
        // tracked file waits for a matching creation; a directory is deleted
        // at once and its children's removals are covered by it.
        let mut removed_dirs: Vec<String> = Vec::new();
        removed.sort();
        removed.dedup();
        for path in removed {
            let Some(node) = shared.node(&path) else {
                continue;
            };
            match disk.observe(&path) {
                Ok(Observed::Gone) | Err(_) => {}
                Ok(_) => {
                    upserts.insert(path);
                    continue;
                }
            }
            if disk.is_echo(&path, Observed::Gone) {
                continue;
            }
            if removed_dirs.iter().any(|d| {
                path.strip_prefix(d.as_str())
                    .is_some_and(|r| r.starts_with('/'))
            }) {
                continue;
            }
            if node.kind == NodeKind::Dir {
                removed_dirs.push(path.clone());
                intents.push(Intent::Delete { path });
            } else {
                stats.files += 1;
                self.parked.retain(|p| p.path != path);
                self.parked.push(Parked {
                    path,
                    node,
                    at_ms: now,
                });
            }
        }

        let mut dirs = Vec::new();
        let mut files = Vec::new();
        for path in upserts {
            self.parked.retain(|p| p.path != path);
            self.upsert(&path, disk, shared, &mut stats, &mut dirs, &mut files);
        }
        // Parents before children.
        dirs.sort_by(|a: &Intent, b: &Intent| intent_path(a).cmp(intent_path(b)));
        files.sort_by(|a: &Intent, b: &Intent| intent_path(a).cmp(intent_path(b)));
        // Renames discovered by pairing come before everything else.
        let mut paired = Vec::new();
        files.retain_mut(|i| match i {
            Intent::Rename { .. } => {
                paired.push(i.clone());
                false
            }
            _ => true,
        });
        let mut out = paired;
        out.extend(intents);
        out.extend(dirs);
        out.extend(files);
        (out, stats)
    }

    fn rename_is_echo(&self, disk: &impl DiskView, to: &str) -> bool {
        match disk.observe(to) {
            Ok(o) => disk.is_echo(to, o),
            Err(_) => false,
        }
    }

    fn upsert(
        &mut self,
        path: &str,
        disk: &impl DiskView,
        shared: &impl SharedView,
        stats: &mut Stats,
        dirs: &mut Vec<Intent>,
        files: &mut Vec<Intent>,
    ) {
        let now = self.clock.now_ms();
        let observed = match disk.observe(path) {
            Ok(o) => o,
            Err(e) => {
                files.push(Intent::Skipped {
                    path: path.to_owned(),
                    reason: e.to_string(),
                });
                return;
            }
        };
        let node = shared.node(path);
        let hash = match observed {
            Observed::Gone => {
                if let Some(node) = node.filter(|_| !disk.is_echo(path, Observed::Gone)) {
                    if node.kind == NodeKind::Dir {
                        dirs.push(Intent::Delete {
                            path: path.to_owned(),
                        });
                    } else {
                        stats.files += 1;
                        self.parked.push(Parked {
                            path: path.to_owned(),
                            node,
                            at_ms: now,
                        });
                    }
                }
                return;
            }
            Observed::Dir => {
                if node.is_none() && !disk.is_echo(path, Observed::Dir) {
                    dirs.push(Intent::CreateDir {
                        path: path.to_owned(),
                    });
                }
                return;
            }
            Observed::Content(h) => h,
        };
        if disk.is_echo(path, observed) || disk.is_behind(path) {
            return;
        }
        if node.is_some_and(|n| n.kind != NodeKind::Dir && n.sha256 == hash) {
            return;
        }
        let bytes = match disk.read(path) {
            Ok(b) => b,
            Err(e) => {
                files.push(Intent::Skipped {
                    path: path.to_owned(),
                    reason: e.to_string(),
                });
                return;
            }
        };
        let size = bytes.len() as u64;
        let (kind, text) = classify(&bytes);
        if let Err(e) = disk.check_size(kind, size) {
            files.push(Intent::Skipped {
                path: path.to_owned(),
                reason: e.to_string(),
            });
            return;
        }
        stats.files += 1;
        match node {
            None => {
                // A creation: perhaps the other half of a move.
                if size > 0
                    && let Some(i) = self.parked.iter().position(|p| {
                        p.node.sha256 == hash
                            && p.node.kind != NodeKind::Dir
                            && now.saturating_sub(p.at_ms) <= MOVE_WINDOW_MS
                    })
                {
                    let p = self.parked.remove(i);
                    files.push(Intent::Rename {
                        from: p.path,
                        to: path.to_owned(),
                    });
                    return;
                }
                files.push(Intent::CreateFile {
                    path: path.to_owned(),
                    kind,
                    size,
                    sha256: hash,
                    text,
                });
            }
            Some(n) if n.kind == NodeKind::Dir || (n.kind == NodeKind::Text && text.is_none()) => {
                // A directory became a file, or a text file became binary:
                // a new node replaces the old one.
                files.push(Intent::Delete {
                    path: path.to_owned(),
                });
                files.push(Intent::CreateFile {
                    path: path.to_owned(),
                    kind,
                    size,
                    sha256: hash,
                    text,
                });
            }
            Some(_) if disk.is_open(path) => {
                files.push(Intent::OpenChanged {
                    path: path.to_owned(),
                });
            }
            Some(n) if n.kind == NodeKind::Blob => {
                files.push(Intent::SetBlob {
                    path: path.to_owned(),
                    size,
                    sha256: hash,
                });
            }
            Some(_) => {
                let Some(new) = text else { return };
                match shared.text(path) {
                    Some(old) => {
                        let (edits, removed, inserted) = diff_stats(&old, &new);
                        stats.changed_lines += removed + inserted;
                        stats.total_lines += count_lines(&old) + count_lines(&new);
                        if edits.is_empty() {
                            return;
                        }
                        files.push(Intent::Edit {
                            path: path.to_owned(),
                            sha256: hash,
                            edits,
                        });
                    }
                    None => files.push(Intent::ReplaceText {
                        path: path.to_owned(),
                        sha256: hash,
                        text: new,
                    }),
                }
            }
        }
    }
}

fn intent_path(i: &Intent) -> &str {
    match i {
        Intent::CreateDir { path }
        | Intent::CreateFile { path, .. }
        | Intent::Edit { path, .. }
        | Intent::ReplaceText { path, .. }
        | Intent::SetBlob { path, .. }
        | Intent::Delete { path }
        | Intent::OpenChanged { path }
        | Intent::Skipped { path, .. } => path,
        Intent::Rename { to, .. } => to,
        Intent::Pause(_) => "",
    }
}

#[derive(Debug, Default)]
struct Stats {
    files: usize,
    changed_lines: usize,
    total_lines: usize,
}

impl Stats {
    fn should_pause(&self, t: &Thresholds) -> bool {
        self.files > t.max_files
            || (self.total_lines >= t.min_lines
                && self.changed_lines * 100 > self.total_lines * t.rewrite_pct)
    }
}

/// Text (valid UTF-8 without NUL) or blob.
fn classify(bytes: &[u8]) -> (NodeKind, Option<String>) {
    if bytes.contains(&0) {
        return (NodeKind::Blob, None);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => (NodeKind::Text, Some(s.to_owned())),
        Err(_) => (NodeKind::Blob, None),
    }
}

fn count_lines(s: &str) -> usize {
    s.split_inclusive('\n').count()
}

/// Minimal edits turning `old` into `new`: a line diff, each changed hunk
/// trimmed of its common prefix and suffix. Empty when equal.
#[must_use]
pub fn text_edits(old: &str, new: &str) -> Vec<TextEdit> {
    diff_stats(old, new).0
}

fn diff_stats(old: &str, new: &str) -> (Vec<TextEdit>, usize, usize) {
    if old == new {
        return (Vec::new(), 0, 0);
    }
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new.split_inclusive('\n').collect();
    let starts = |lines: &[&str]| {
        let mut v = Vec::with_capacity(lines.len() + 1);
        let mut at = 0;
        v.push(0);
        for l in lines {
            at += l.len();
            v.push(at);
        }
        v
    };
    let old_at = starts(&old_lines);
    let new_at = starts(&new_lines);
    let (mut removed, mut inserted) = (0, 0);
    let mut edits = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, &old_lines, &new_lines) {
        let (tag, o, n) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            continue;
        }
        removed += o.len();
        inserted += n.len();
        let (mut start, mut end) = (old_at[o.start], old_at[o.end]);
        let mut insert = &new[new_at[n.start]..new_at[n.end]];
        // Trim the common prefix and suffix (on char boundaries).
        let removed_text = &old[start..end];
        let prefix: usize = removed_text
            .chars()
            .zip(insert.chars())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len_utf8())
            .sum();
        start += prefix;
        insert = &insert[prefix..];
        let removed_rest = &old[start..end];
        let suffix: usize = removed_rest
            .chars()
            .rev()
            .zip(insert.chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len_utf8())
            .sum();
        end -= suffix;
        insert = &insert[..insert.len() - suffix];
        if start < end || !insert.is_empty() {
            edits.push(TextEdit {
                start,
                end,
                insert: insert.to_owned(),
            });
        }
    }
    (edits, removed, inserted)
}

/// Apply [`text_edits`] output to the old text. `None` if an edit is out of
/// range or not on a char boundary.
#[must_use]
pub fn apply_edits(old: &str, edits: &[TextEdit]) -> Option<String> {
    let mut out = String::with_capacity(old.len());
    let mut at = 0;
    for e in edits {
        if e.start < at
            || e.end < e.start
            || !old.is_char_boundary(e.start)
            || !old.is_char_boundary(e.end)
        {
            return None;
        }
        out.push_str(old.get(at..e.start)?);
        out.push_str(&e.insert);
        at = e.end;
    }
    out.push_str(old.get(at..)?);
    Some(out)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::disk::{ManualClock, WriteOutcome};
    use crate::pathsec::PathPolicy;

    #[derive(Default)]
    struct Shared(BTreeMap<String, (SharedNode, Option<String>)>);

    impl Shared {
        fn set_text(&mut self, path: &str, content: &str) {
            self.0.insert(
                path.to_owned(),
                (
                    SharedNode {
                        kind: NodeKind::Text,
                        sha256: sha256(content.as_bytes()),
                    },
                    Some(content.to_owned()),
                ),
            );
        }
        fn blob(&mut self, path: &str, content: &[u8]) {
            self.0.insert(
                path.to_owned(),
                (
                    SharedNode {
                        kind: NodeKind::Blob,
                        sha256: sha256(content),
                    },
                    None,
                ),
            );
        }
        fn dir(&mut self, path: &str) {
            self.0.insert(
                path.to_owned(),
                (
                    SharedNode {
                        kind: NodeKind::Dir,
                        sha256: [0; 32],
                    },
                    None,
                ),
            );
        }
    }

    impl SharedView for Shared {
        fn node(&self, path: &str) -> Option<SharedNode> {
            self.0.get(path).map(|(n, _)| *n)
        }
        fn text(&self, path: &str) -> Option<String> {
            self.0.get(path).and_then(|(_, t)| t.clone())
        }
    }

    struct Fx {
        _tmp: tempfile::TempDir,
        clock: ManualClock,
        disk: DiskBridge<ManualClock>,
        fs: FsBridge<ManualClock>,
        shared: Shared,
    }

    fn fx() -> Fx {
        let tmp = tempfile::tempdir().expect("tmp");
        let clock = ManualClock::new();
        let policy = PathPolicy::new(tmp.path())
            .expect("policy")
            .with_case_insensitive(false);
        Fx {
            disk: DiskBridge::new(policy, clock.clone()),
            fs: FsBridge::new(clock.clone()),
            clock,
            shared: Shared::default(),
            _tmp: tmp,
        }
    }

    fn ch(kind: FsKind, path: &str) -> FsChange {
        FsChange {
            path: path.to_owned(),
            kind,
            from: None,
        }
    }

    fn renamed(from: &str, to: &str) -> FsChange {
        FsChange {
            path: to.to_owned(),
            kind: FsKind::Renamed,
            from: Some(from.to_owned()),
        }
    }

    impl Fx {
        fn put(&self, rel: &str, content: &str) {
            let p = self.disk.policy().root().path().join(rel);
            fs::create_dir_all(p.parent().expect("parent")).expect("dirs");
            fs::write(p, content).expect("write");
        }
        fn rm(&self, rel: &str) {
            fs::remove_file(self.disk.policy().root().path().join(rel)).expect("rm");
        }
        fn run(&mut self, changes: &[FsChange]) -> Vec<Intent> {
            self.fs.process(changes, &self.disk, &self.shared)
        }
    }

    #[test]
    fn parses_the_watcher_payload() {
        let payload = serde_json::json!({"root": "/p", "changes": [
            {"path": "a.typ", "kind": "created"},
            {"path": "n.typ", "kind": "renamed", "from": "o.typ"},
            {"path": "bad", "kind": "weird"},
            "junk",
        ]});
        let got = parse_changed_event(&payload);
        assert_eq!(got.len(), 2);
        assert_eq!(got[1], renamed("o.typ", "n.typ"));
        assert!(parse_changed_event(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn a_new_file_becomes_a_create_intent() {
        let mut f = fx();
        f.put("notes/a.md", "hello\n");
        f.put("fig.png", "x");
        fs::write(
            f.disk.policy().root().path().join("bin.dat"),
            [0u8, 159, 146, 150],
        )
        .expect("bin");
        let got = f.run(&[
            ch(FsKind::Created, "notes/a.md"),
            ch(FsKind::Created, "notes"),
            ch(FsKind::Created, "bin.dat"),
        ]);
        assert_eq!(got.len(), 3, "{got:?}");
        assert_eq!(
            got[0],
            Intent::CreateDir {
                path: "notes".into()
            }
        );
        assert!(
            matches!(&got[1], Intent::CreateFile { path, kind: NodeKind::Blob, text: None, .. } if path == "bin.dat")
        );
        assert!(
            matches!(&got[2], Intent::CreateFile { path, kind: NodeKind::Text, size: 6, text: Some(t), .. } if path == "notes/a.md" && t == "hello\n")
        );
    }

    #[test]
    fn echo_of_our_own_writes_is_dropped() {
        let mut f = fx();
        f.shared.set_text("a.md", "old");
        assert!(matches!(
            f.disk.queue_text("a.md", "new").expect("q"),
            WriteOutcome::Queued { .. }
        ));
        f.clock.advance(500);
        f.disk.tick();
        assert!(f.run(&[ch(FsKind::Modified, "a.md")]).is_empty());
        // The echo of a brand-new remote file, of a remote delete and rename.
        f.disk.queue_text("b.md", "b").expect("q");
        f.clock.advance(500);
        f.disk.tick();
        f.shared.set_text("b.md", "b");
        f.disk.rename("b.md", "c.md").expect("rename");
        f.shared.set_text("c.md", "b");
        assert!(
            f.run(&[
                ch(FsKind::Created, "b.md"),
                ch(FsKind::Removed, "b.md"),
                renamed("b.md", "c.md"),
            ])
            .is_empty()
        );
        f.disk.delete("c.md").expect("delete");
        assert!(f.run(&[ch(FsKind::Removed, "c.md")]).is_empty());
        f.clock.advance(5_000);
        assert!(f.fs.tick().is_empty());
        // A genuine local edit right after is not an echo.
        f.put("a.md", "typed by me");
        let got = f.run(&[ch(FsKind::Modified, "a.md")]);
        assert!(matches!(got.as_slice(), [Intent::Edit { .. }]));
    }

    #[test]
    fn unchanged_content_and_files_behind_a_pending_write_are_dropped() {
        let mut f = fx();
        f.shared.set_text("a.md", "same");
        f.put("a.md", "same");
        assert!(f.run(&[ch(FsKind::Modified, "a.md")]).is_empty());
        // The shared text moved on and its write is still queued: the stale
        // disk must not be diffed back over it.
        f.shared.set_text("p.md", "remote edit");
        f.put("p.md", "stale");
        f.disk.queue_text("p.md", "remote edit").expect("q");
        assert!(f.run(&[ch(FsKind::Modified, "p.md")]).is_empty());
    }

    #[test]
    fn external_edit_becomes_a_minimal_diff() {
        let mut f = fx();
        f.shared.set_text("a.md", "line1\nline2\nline3\nline4\n");
        f.put("a.md", "line1\nline2 edited\nline3\nline4\nline5\n");
        let got = f.run(&[ch(FsKind::Modified, "a.md")]);
        let [
            Intent::Edit {
                edits, sha256: h, ..
            },
        ] = got.as_slice()
        else {
            panic!("{got:?}");
        };
        assert_eq!(*h, sha256(b"line1\nline2 edited\nline3\nline4\nline5\n"));
        assert_eq!(
            edits,
            &vec![
                TextEdit {
                    start: 11,
                    end: 11,
                    insert: " edited".into()
                },
                TextEdit {
                    start: 24,
                    end: 24,
                    insert: "line5\n".into()
                },
            ]
        );
        assert_eq!(
            apply_edits("line1\nline2\nline3\nline4\n", edits).as_deref(),
            Some("line1\nline2 edited\nline3\nline4\nline5\n")
        );
    }

    #[test]
    fn without_shared_text_the_intent_replaces_it() {
        let mut f = fx();
        f.shared.0.insert(
            "a.md".into(),
            (
                SharedNode {
                    kind: NodeKind::Text,
                    sha256: [1; 32],
                },
                None,
            ),
        );
        f.put("a.md", "new");
        assert!(matches!(
            f.run(&[ch(FsKind::Modified, "a.md")]).as_slice(),
            [Intent::ReplaceText { text, .. }] if text == "new"
        ));
    }

    #[test]
    fn blob_edits_and_retyping() {
        let mut f = fx();
        f.shared.blob("fig.svg", b"old");
        f.put("fig.svg", "<svg/>");
        assert!(matches!(
            f.run(&[ch(FsKind::Modified, "fig.svg")]).as_slice(),
            [Intent::SetBlob { size: 6, .. }]
        ));
        // A text file that turned binary is a new node.
        f.shared.set_text("t.md", "text");
        fs::write(f.disk.policy().root().path().join("t.md"), [0u8, 1, 2]).expect("w");
        let got = f.run(&[ch(FsKind::Modified, "t.md")]);
        assert!(matches!(
            got.as_slice(),
            [
                Intent::Delete { .. },
                Intent::CreateFile {
                    kind: NodeKind::Blob,
                    ..
                }
            ]
        ));
    }

    #[test]
    fn open_files_report_instead_of_diffing() {
        let mut f = fx();
        f.shared.set_text("a.md", "old");
        f.put("a.md", "new");
        f.disk.set_open("a.md");
        assert_eq!(
            f.run(&[ch(FsKind::Modified, "a.md")]),
            vec![Intent::OpenChanged {
                path: "a.md".into()
            }]
        );
    }

    #[test]
    fn rename_with_from_is_a_rename_intent() {
        let mut f = fx();
        f.shared.set_text("old.md", "x");
        f.put("sub/new.md", "x");
        assert_eq!(
            f.run(&[renamed("old.md", "sub/new.md")]),
            vec![Intent::Rename {
                from: "old.md".into(),
                to: "sub/new.md".into()
            }]
        );
        // An untracked source is just a creation.
        f.put("fresh.md", "y");
        assert!(matches!(
            f.run(&[renamed("nowhere.md", "fresh.md")]).as_slice(),
            [Intent::CreateFile { .. }]
        ));
    }

    #[test]
    fn remove_plus_create_with_the_same_hash_is_a_move() {
        let mut f = fx();
        f.shared.set_text("a.md", "content");
        f.put("a.md", "content");
        f.rm("a.md");
        // Removal alone is parked, not yet a delete.
        assert!(f.run(&[ch(FsKind::Removed, "a.md")]).is_empty());
        f.clock.advance(1_500);
        assert!(f.fs.tick().is_empty());
        f.put("moved/b.md", "content");
        assert_eq!(
            f.run(&[
                ch(FsKind::Created, "moved/b.md"),
                ch(FsKind::Created, "moved")
            ]),
            vec![
                Intent::Rename {
                    from: "a.md".into(),
                    to: "moved/b.md".into()
                },
                Intent::CreateDir {
                    path: "moved".into()
                },
            ]
        );
        f.clock.advance(5_000);
        assert!(f.fs.tick().is_empty(), "the pair must not also delete");
    }

    #[test]
    fn a_move_in_one_batch_and_in_any_order() {
        let mut f = fx();
        f.shared.set_text("a.md", "content");
        f.put("b.md", "content");
        let got = f.run(&[ch(FsKind::Created, "b.md"), ch(FsKind::Removed, "a.md")]);
        assert_eq!(
            got,
            vec![Intent::Rename {
                from: "a.md".into(),
                to: "b.md".into()
            }]
        );
    }

    #[test]
    fn an_unmatched_removal_becomes_a_delete_after_the_window() {
        let mut f = fx();
        f.shared.set_text("a.md", "content");
        assert!(f.run(&[ch(FsKind::Removed, "a.md")]).is_empty());
        f.clock.advance(MOVE_WINDOW_MS);
        assert!(f.fs.tick().is_empty());
        f.clock.advance(1);
        assert_eq!(
            f.fs.tick(),
            vec![Intent::Delete {
                path: "a.md".into()
            }]
        );
        assert!(f.fs.tick().is_empty());
        // A creation after the window is a plain create.
        f.shared.set_text("b.md", "other");
        f.run(&[ch(FsKind::Removed, "b.md")]);
        f.clock.advance(MOVE_WINDOW_MS + 1);
        f.put("c.md", "other");
        let got = f.run(&[ch(FsKind::Created, "c.md")]);
        assert!(matches!(got.as_slice(), [Intent::CreateFile { .. }]));
        assert_eq!(
            f.fs.tick(),
            vec![Intent::Delete {
                path: "b.md".into()
            }]
        );
    }

    #[test]
    fn remove_and_recreate_at_the_same_path_is_a_modify() {
        let mut f = fx();
        f.shared.set_text("a.md", "one");
        f.put("a.md", "two");
        let got = f.run(&[ch(FsKind::Removed, "a.md"), ch(FsKind::Created, "a.md")]);
        assert!(matches!(got.as_slice(), [Intent::Edit { .. }]));
        f.clock.advance(5_000);
        assert!(f.fs.tick().is_empty());
    }

    #[test]
    fn removed_directories_delete_once() {
        let mut f = fx();
        f.shared.dir("d");
        f.shared.set_text("d/a.md", "a");
        f.shared.set_text("d/b.md", "b");
        let got = f.run(&[
            ch(FsKind::Removed, "d/a.md"),
            ch(FsKind::Removed, "d/b.md"),
            ch(FsKind::Removed, "d"),
        ]);
        assert_eq!(got, vec![Intent::Delete { path: "d".into() }]);
        f.clock.advance(5_000);
        assert!(f.fs.tick().is_empty());
    }

    #[test]
    fn unshareable_and_hostile_paths_are_ignored() {
        let mut f = fx();
        for p in [".env", ".git/config", "manang.toml", "node_modules/x.js"] {
            f.put(p, "x");
        }
        let got = f.run(&[
            ch(FsKind::Created, ".env"),
            ch(FsKind::Modified, ".git/config"),
            ch(FsKind::Created, "manang.toml"),
            ch(FsKind::Created, "node_modules/x.js"),
            ch(FsKind::Created, "../outside.txt"),
            ch(FsKind::Created, "/etc/passwd"),
        ]);
        assert!(got.is_empty(), "{got:?}");
        // Rename into the private set is a delete; out of it a create.
        f.shared.set_text("a.md", "x");
        f.put(".hidden", "x");
        f.put("shown.md", "y");
        f.shared.set_text("tracked.md", "t");
        assert_eq!(f.run(&[renamed("a.md", ".hidden")]), vec![]);
        assert!(f.fs.tick().is_empty());
        let got = f.run(&[renamed(".hidden", "shown.md")]);
        assert!(matches!(got.as_slice(), [Intent::CreateFile { .. }]));
    }

    #[test]
    fn rename_to_an_unshareable_name_deletes_the_tracked_node() {
        let mut f = fx();
        f.shared.set_text("a.md", "x");
        let got = f.run(&[renamed("a.md", ".a.md.swp")]);
        assert!(got.is_empty());
        f.clock.advance(MOVE_WINDOW_MS + 1);
        assert_eq!(
            f.fs.tick(),
            vec![Intent::Delete {
                path: "a.md".into()
            }]
        );
    }

    #[test]
    fn excluded_paths_stay_private() {
        let mut f = fx();
        f.put("private/a.md", "x");
        f.put("pub.md", "y");
        f.fs.exclude("private");
        let got = f.run(&[
            ch(FsKind::Created, "private/a.md"),
            ch(FsKind::Created, "pub.md"),
        ]);
        assert!(matches!(got.as_slice(), [Intent::CreateFile { path, .. }] if path == "pub.md"));
        f.fs.include("private");
        let got = f.run(&[ch(FsKind::Created, "private/a.md")]);
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn oversized_files_are_skipped_not_shared() {
        let tmp = tempfile::tempdir().expect("tmp");
        let policy = PathPolicy::new(tmp.path()).expect("p").with_caps(8, 16);
        let clock = ManualClock::new();
        let disk = DiskBridge::new(policy, clock.clone());
        let mut fsb = FsBridge::new(clock);
        fs::write(tmp.path().join("big.md"), "0123456789").expect("w");
        let got = fsb.process(&[ch(FsKind::Created, "big.md")], &disk, &Shared::default());
        assert!(matches!(got.as_slice(), [Intent::Skipped { path, .. }] if path == "big.md"));
    }

    fn big_text(tag: &str, lines: usize) -> String {
        (0..lines).map(|i| format!("{tag} line {i}\n")).collect()
    }

    #[test]
    fn a_large_rewrite_pauses_and_can_be_shared() {
        let mut f = fx();
        for i in 0..3 {
            let p = format!("doc{i}.md");
            f.shared.set_text(&p, &big_text("old", 30));
            f.put(&p, &big_text("new", 30));
        }
        let changes: Vec<FsChange> = (0..3)
            .map(|i| ch(FsKind::Modified, &format!("doc{i}.md")))
            .collect();
        let got = f.run(&changes);
        let [Intent::Pause(info)] = got.as_slice() else {
            panic!("{got:?}");
        };
        assert_eq!(info.files, 3);
        assert!(info.changed_lines * 2 > info.total_lines);
        assert!(f.fs.is_paused());
        // Later changes are held too.
        f.put("later.md", "x");
        assert!(f.run(&[ch(FsKind::Created, "later.md")]).is_empty());
        let r = f.fs.resolve_pause(true, &f.disk, &f.shared);
        assert!(r.revert.is_empty());
        assert_eq!(r.intents.len(), 4, "{:?}", r.intents);
        assert!(!f.fs.is_paused());
        // Nothing is held anymore: a small edit goes straight through.
        f.shared.set_text("later.md", "x");
        f.put("later.md", "xy");
        assert!(matches!(
            f.run(&[ch(FsKind::Modified, "later.md")]).as_slice(),
            [Intent::Edit { .. }]
        ));
    }

    #[test]
    fn a_large_rewrite_can_be_discarded_and_reverted() {
        let mut f = fx();
        for i in 0..3 {
            let p = format!("doc{i}.md");
            f.shared.set_text(&p, &big_text("old", 30));
            f.put(&p, &big_text("new", 30));
        }
        f.put("untracked.md", "mine");
        let mut changes: Vec<FsChange> = (0..3)
            .map(|i| ch(FsKind::Modified, &format!("doc{i}.md")))
            .collect();
        changes.push(ch(FsKind::Created, "untracked.md"));
        assert!(matches!(f.run(&changes).as_slice(), [Intent::Pause(_)]));
        let r = f.fs.resolve_pause(false, &f.disk, &f.shared);
        assert!(r.intents.is_empty());
        assert_eq!(r.revert, vec!["doc0.md", "doc1.md", "doc2.md"]);
        assert!(!f.fs.is_paused());
        // Answering twice is harmless.
        assert_eq!(
            f.fs.resolve_pause(true, &f.disk, &f.shared),
            PauseResolution::default()
        );
    }

    #[test]
    fn many_changed_files_pause_even_when_small() {
        let mut f = fx();
        let mut changes = Vec::new();
        for i in 0..26 {
            let p = format!("f{i}.md");
            f.put(&p, "x");
            changes.push(ch(FsKind::Created, &p));
        }
        assert!(matches!(
            f.run(&changes).as_slice(),
            [Intent::Pause(PauseInfo { files: 26, .. })]
        ));
        // The parked state of the paused batch was rolled back.
        assert!(f.fs.parked.is_empty());
        let mut g = fx();
        let mut changes = Vec::new();
        for i in 0..25 {
            let p = format!("f{i}.md");
            g.put(&p, "x");
            changes.push(ch(FsKind::Created, &p));
        }
        assert_eq!(g.run(&changes).len(), 25);
    }

    #[test]
    fn a_normal_edit_in_a_big_file_does_not_pause() {
        let mut f = fx();
        let old = big_text("l", 200);
        let new = old.replace("l line 100\n", "l line 100 edited\n");
        f.shared.set_text("a.md", &old);
        f.put("a.md", &new);
        assert!(matches!(
            f.run(&[ch(FsKind::Modified, "a.md")]).as_slice(),
            [Intent::Edit { .. }]
        ));
    }

    #[test]
    fn remote_delete_with_a_dirty_open_tab_keeps_it_and_restore_recreates_the_node() {
        let mut f = fx();
        f.shared.set_text("a.md", "shared");
        f.put("a.md", "my unsaved-then-autosaved work");
        f.disk.set_open("a.md");
        assert_eq!(
            f.fs.remote_delete("a.md", "Ada", true),
            RemoteDelete::KeepOpen {
                path: "a.md".into(),
                by: "Ada".into()
            }
        );
        assert_eq!(f.fs.deleted_by("a.md"), Some("Ada"));
        // The disk bridge defers the delete while the tab is open.
        assert_eq!(
            f.disk.delete("a.md").expect("d"),
            crate::disk::Deleted::DeferredOpen
        );
        let intent = f.fs.restore("a.md", &f.disk).expect("restore");
        assert!(
            matches!(&intent, Intent::CreateFile { path, kind: NodeKind::Text, text: Some(t), .. }
            if path == "a.md" && t == "my unsaved-then-autosaved work")
        );
        assert_eq!(f.fs.deleted_by("a.md"), None);
        assert_eq!(
            f.fs.remote_delete("b.md", "Ada", false),
            RemoteDelete::Apply
        );
        f.fs.remote_delete("c.md", "Bo", true);
        f.fs.forget_deleted("c.md");
        assert_eq!(f.fs.deleted_by("c.md"), None);
        assert!(f.fs.restore("missing.md", &f.disk).is_err());
    }

    #[test]
    fn edits_round_trip_on_unicode_and_edges() {
        for (old, new) in [
            ("", "abc"),
            ("abc", ""),
            (
                "héllo wörld\nsecond ünï\n",
                "héllo wörld!\nsecond ünï\nthird\n",
            ),
            ("a\nb\nc", "a\nB\nc"),
            ("same", "same"),
            ("日本語\nテキスト\n", "日本\nテキスト語\n"),
        ] {
            let edits = text_edits(old, new);
            assert_eq!(
                apply_edits(old, &edits).as_deref(),
                Some(new),
                "{old:?} -> {new:?}"
            );
        }
        assert!(text_edits("x", "x").is_empty());
        assert_eq!(
            apply_edits(
                "ab",
                &[TextEdit {
                    start: 5,
                    end: 6,
                    insert: String::new()
                }]
            ),
            None
        );
    }

    mod prop {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn applying_the_edits_gives_the_new_text(
                old in "[a-zé日\n ]{0,60}",
                new in "[a-zé日\n ]{0,60}",
            ) {
                let edits = text_edits(&old, &new);
                prop_assert_eq!(apply_edits(&old, &edits), Some(new));
            }
        }
    }
}
