//! The project document: the file tree of a live project session.
//!
//! One CRDT tree holds every file and directory of the project, keyed by a
//! stable [`FileId`] that survives renames and moves. Node metadata is
//! `{name, kind, size, sha256, seed_fp}`; text content lives in per-file
//! documents and binary content in content-addressed blobs
//! ([`crate::blob`]), neither of which is stored here.
//!
//! The CRDT engine sits behind the [`TreeCrdt`] trait (Loro's movable tree in
//! [`LoroTreeEngine`]). Everything that decides what a *path* is lives above
//! the trait, in [`resolve`], so it is engine-independent and pure:
//!
//! * Names are sanitised ([`valid_name`]); an invalid remote name is replaced
//!   by `invalid-<id>` so a hostile peer can never produce `..` or `/`.
//! * Two directories with the same name (case-folded) under the same parent
//!   **merge**: both peers created `src/`, the project has one `src/`.
//! * Two non-directory nodes with the same path are a **collision**. The node
//!   created first (lowest Lamport timestamp, then peer id, then counter)
//!   keeps the name; every other node is shown as
//!   `stem (conflict xxxxxxxx).ext`, `xxxxxxxx` being the start of its id.
//!   Every replica computes the same answer from the same tree state.
//! * Nodes deeper than [`MAX_DEPTH`] are not part of the view.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::sync::Arc;

use loro::{
    ExportMode, ID, LoroDoc, LoroTree, LoroValue, Subscription, TreeID, TreeParentId,
    ValueOrContainer, VersionVector,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::blob::MAX_BLOB_BYTES;
use crate::error::LiveError;
use crate::manifest::ManifestEntry;

/// Longest accepted file or directory name, in bytes.
pub const MAX_NAME_BYTES: usize = 255;
/// Deepest nesting of the view (a root entry has depth 1).
pub const MAX_DEPTH: usize = 64;
/// Most live nodes a local edit may grow the tree to.
pub const MAX_NODES: usize = 65_536;

fn project_err(msg: impl Into<String>) -> LiveError {
    LiveError::Project(msg.into())
}

/// A stable 128-bit identity of a file or directory.
///
/// Minted by the creating peer as `peer << 64 | counter` (the CRDT's own
/// operation id of the creation), so ids never collide between peers and
/// need no central allocator.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FileId(pub u128);

impl FileId {
    fn from_tree(id: TreeID) -> Self {
        Self((u128::from(id.peer) << 64) | u128::from(id.counter.unsigned_abs()))
    }

    fn to_tree(self) -> Option<TreeID> {
        let peer = u64::try_from(self.0 >> 64).ok()?;
        let counter = i32::try_from(self.0 & 0xffff_ffff_ffff_ffff).ok()?;
        (counter >= 0).then(|| TreeID::new(peer, counter))
    }

    /// The creating peer.
    #[must_use]
    pub fn peer(self) -> u64 {
        (self.0 >> 64) as u64
    }

    /// Lowercase 32-digit hex.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("{:032x}", self.0)
    }

    /// Parse [`FileId::to_hex`] output.
    #[must_use]
    pub fn from_hex(s: &str) -> Option<Self> {
        (s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| u128::from_str_radix(s, 16).ok())
            .flatten()
            .map(Self)
    }
}

impl fmt::Debug for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileId({:032x})", self.0)
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    /// A directory.
    Dir,
    /// A text file with its own live document.
    Text,
    /// Any other file: a content-addressed blob.
    Blob,
}

impl NodeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Dir => "dir",
            Self::Text => "text",
            Self::Blob => "blob",
        }
    }

    /// Unknown kinds from a newer or hostile peer degrade to an inert blob.
    fn parse(s: &str) -> Self {
        match s {
            "dir" => Self::Dir,
            "text" => Self::Text,
            _ => Self::Blob,
        }
    }
}

/// Node metadata, as stored in the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeMeta {
    /// File or directory name (one path component).
    pub name: String,
    /// Kind of node.
    pub kind: NodeKind,
    /// Content size in bytes (0 for directories).
    pub size: u64,
    /// SHA-256 of the content (zeros for directories).
    pub sha256: [u8; 32],
    /// Seed fingerprint of a text file's initial content
    /// ([`crate::envelope::seed_fingerprint`]); zeros otherwise.
    pub seed_fp: [u8; 16],
}

impl NodeMeta {
    /// Metadata of a directory.
    #[must_use]
    pub fn dir(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: NodeKind::Dir,
            size: 0,
            sha256: [0; 32],
            seed_fp: [0; 16],
        }
    }

    /// Metadata of a file.
    #[must_use]
    pub fn file(
        name: impl Into<String>,
        kind: NodeKind,
        size: u64,
        sha256: [u8; 32],
        seed_fp: [u8; 16],
    ) -> Self {
        Self {
            name: name.into(),
            kind,
            size,
            sha256,
            seed_fp,
        }
    }
}

/// Whether `name` is a safe single path component.
///
/// Rejects empty names, `.` and `..`, separators, control characters,
/// Windows-hostile characters and trailing dots or spaces, and names over
/// [`MAX_NAME_BYTES`]. Reserved Windows device names and the deny-list of
/// owner decision 1 (dotfiles, `.git`) are enforced by the disk bridge, not
/// here: the tree must be able to represent what the host has.
#[must_use]
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_BYTES
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
        && !name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*')
        })
}

/// A live node as the engine reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawNode {
    /// Identity.
    pub id: FileId,
    /// Parent directory, `None` for a top-level entry.
    pub parent: Option<FileId>,
    /// Stored metadata (unvalidated: it may come from a hostile peer).
    pub meta: NodeMeta,
    /// Creation order key: Lamport timestamp of the creating operation.
    pub born_lamport: u64,
}

/// The tree CRDT the project document needs. Implemented over Loro's
/// movable tree by [`LoroTreeEngine`]; another engine can replace it with one
/// adapter, as with [`crate::TextCrdt`].
pub trait TreeCrdt: Sized {
    /// An empty tree authored by `peer`.
    fn new(peer: u64) -> Result<Self, LiveError>;
    /// Restore from a snapshot.
    fn from_snapshot(peer: u64, bytes: &[u8]) -> Result<Self, LiveError>;
    /// This replica's peer id.
    fn peer(&self) -> u64;
    /// Create a node under `parent` and return its id.
    fn create(&mut self, parent: Option<FileId>, meta: &NodeMeta) -> Result<FileId, LiveError>;
    /// Replace a node's metadata.
    fn set_meta(&mut self, id: FileId, meta: &NodeMeta) -> Result<(), LiveError>;
    /// Move a node under `parent` (a cycle is rejected by the engine).
    fn move_to(&mut self, id: FileId, parent: Option<FileId>) -> Result<(), LiveError>;
    /// Delete a node (and, implicitly, its subtree).
    fn delete(&mut self, id: FileId) -> Result<(), LiveError>;
    /// Every live node whose ancestors are all live.
    fn nodes(&self) -> Vec<RawNode>;
    /// Update blobs of local operations since the last call.
    fn take_local_updates(&mut self) -> Result<Vec<Vec<u8>>, LiveError>;
    /// Encoded version.
    fn version(&self) -> Vec<u8>;
    /// Operations a peer at `version` lacks.
    fn export_since(&self, version: &[u8]) -> Result<Vec<u8>, LiveError>;
    /// Full state.
    fn export_snapshot(&self) -> Result<Vec<u8>, LiveError>;
    /// Import remote operations; returns whether some depend on missing ones.
    fn apply_update(&mut self, bytes: &[u8]) -> Result<bool, LiveError>;
}

// ---------------------------------------------------------------------------
// Path resolution (engine independent)
// ---------------------------------------------------------------------------

/// One node of the resolved project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Identity.
    pub id: FileId,
    /// Parent directory, `None` at the top level.
    pub parent: Option<FileId>,
    /// Sanitised metadata (`name` is the node's own, possibly invalid-replaced
    /// name; see `path` for what it is called in the project).
    pub meta: NodeMeta,
    /// Project-relative path with `/` separators, after collision resolution.
    pub path: String,
}

fn fold(name: &str) -> String {
    name.to_lowercase()
}

fn short(id: FileId, digits: usize) -> String {
    let hex = id.to_hex();
    hex[..digits.min(32)].to_owned()
}

/// `stem (conflict xxxxxxxx).ext`, shortened to fit [`MAX_NAME_BYTES`].
fn conflict_name(name: &str, id: FileId, digits: usize) -> String {
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => name.split_at(i),
        _ => (name, ""),
    };
    let suffix = format!(" (conflict {})", short(id, digits));
    let budget = MAX_NAME_BYTES.saturating_sub(suffix.len() + ext.len());
    let mut cut = stem.len().min(budget);
    while !stem.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{}{}", &stem[..cut], suffix, ext)
}

/// Resolve raw nodes into entries with unique paths, sorted by path then id.
///
/// Pure and deterministic: it depends only on the set of nodes, never on the
/// order they are given in.
#[must_use]
pub fn resolve(nodes: &[RawNode]) -> Vec<Entry> {
    let mut by_id: BTreeMap<FileId, &RawNode> = BTreeMap::new();
    for n in nodes {
        by_id.insert(n.id, n);
    }
    let mut children: BTreeMap<Option<FileId>, Vec<&RawNode>> = BTreeMap::new();
    for n in by_id.values() {
        children.entry(n.parent).or_default().push(n);
    }
    for list in children.values_mut() {
        list.sort_by_key(|n| (n.born_lamport, n.id.peer(), n.id));
    }
    let mut out: Vec<Entry> = Vec::with_capacity(nodes.len());
    // Work list of logical directories: (member dir ids, path prefix, depth).
    let mut work: Vec<(Vec<FileId>, String, usize)> = vec![(Vec::new(), String::new(), 1)];
    while let Some((members, prefix, depth)) = work.pop() {
        // Children of every member; the virtual root has the `None` parent.
        let mut kids: Vec<&RawNode> = Vec::new();
        if members.is_empty() {
            if let Some(list) = children.get(&None) {
                kids.extend(list.iter().copied());
            }
        } else {
            for m in &members {
                if let Some(list) = children.get(&Some(*m)) {
                    kids.extend(list.iter().copied());
                }
            }
        }
        kids.sort_by_key(|n| (n.born_lamport, n.id.peer(), n.id));
        let mut groups: BTreeMap<String, Vec<&RawNode>> = BTreeMap::new();
        let display = |n: &RawNode| {
            if valid_name(&n.meta.name) {
                n.meta.name.clone()
            } else {
                format!("invalid-{}", short(n.id, 8))
            }
        };
        for n in &kids {
            groups.entry(fold(&display(n))).or_default().push(n);
        }
        let mut used: BTreeSet<String> = groups.keys().cloned().collect();
        for group in groups.values() {
            let dirs: Vec<&RawNode> = group
                .iter()
                .copied()
                .filter(|n| n.meta.kind == NodeKind::Dir)
                .collect();
            let files: Vec<&RawNode> = group
                .iter()
                .copied()
                .filter(|n| n.meta.kind != NodeKind::Dir)
                .collect();
            let mut files_iter = files.iter().copied();
            if let Some(first_dir) = dirs.first() {
                let path = format!("{prefix}{}", display(first_dir));
                for d in &dirs {
                    out.push(entry_of(d, &path, display(d)));
                }
                if depth < MAX_DEPTH {
                    work.push((
                        dirs.iter().map(|d| d.id).collect(),
                        format!("{path}/"),
                        depth + 1,
                    ));
                }
            } else if let Some(winner) = files_iter.next() {
                out.push(entry_of(
                    winner,
                    &format!("{prefix}{}", display(winner)),
                    display(winner),
                ));
            }
            for loser in files_iter {
                let base = display(loser);
                let mut digits = 8;
                let name = loop {
                    let candidate = conflict_name(&base, loser.id, digits);
                    if digits >= 32 || used.insert(fold(&candidate)) {
                        break candidate;
                    }
                    digits += 4;
                };
                out.push(entry_of(loser, &format!("{prefix}{name}"), display(loser)));
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path).then(a.id.cmp(&b.id)));
    out
}

fn entry_of(n: &RawNode, path: &str, name: String) -> Entry {
    let mut meta = n.meta.clone();
    meta.name = name;
    if meta.kind == NodeKind::Dir {
        meta.size = 0;
        meta.sha256 = [0; 32];
        meta.seed_fp = [0; 16];
    } else {
        meta.size = meta.size.min(MAX_BLOB_BYTES);
    }
    Entry {
        id: n.id,
        parent: n.parent,
        meta,
        path: path.to_owned(),
    }
}

/// A change between two views of the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeChange {
    /// A node appeared.
    Created {
        /// Node.
        id: FileId,
        /// Its path.
        path: String,
    },
    /// A node vanished.
    Deleted {
        /// Node.
        id: FileId,
        /// Its former path.
        path: String,
    },
    /// A node's path changed (rename, move, or a collision resolving).
    Moved {
        /// Node.
        id: FileId,
        /// Former path.
        from: String,
        /// New path.
        to: String,
    },
    /// Size, hash or seed fingerprint changed (content changed elsewhere).
    Content {
        /// Node.
        id: FileId,
        /// Its path.
        path: String,
    },
}

/// A resolved, indexed snapshot of the project tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectView {
    entries: Vec<Entry>,
    index: HashMap<FileId, usize>,
}

impl ProjectView {
    /// Resolve raw nodes into a view.
    #[must_use]
    pub fn from_nodes(nodes: &[RawNode]) -> Self {
        let entries = resolve(nodes);
        let index = entries.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
        Self { entries, index }
    }

    /// All entries, sorted by path.
    #[must_use]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Entry by id.
    #[must_use]
    pub fn get(&self, id: FileId) -> Option<&Entry> {
        self.index.get(&id).and_then(|&i| self.entries.get(i))
    }

    /// Path of a node.
    #[must_use]
    pub fn path_of(&self, id: FileId) -> Option<&str> {
        self.get(id).map(|e| e.path.as_str())
    }

    /// The entry at `path`. For a directory merged from several nodes, the
    /// first by id.
    #[must_use]
    pub fn id_at_path(&self, path: &str) -> Option<FileId> {
        let i = self.entries.partition_point(|e| e.path.as_str() < path);
        self.entries.get(i).filter(|e| e.path == path).map(|e| e.id)
    }

    /// Changes from `old` to `self`, ordered by path.
    #[must_use]
    pub fn diff_from(&self, old: &Self) -> Vec<TreeChange> {
        let mut out = Vec::new();
        for e in &self.entries {
            match old.get(e.id) {
                None => out.push(TreeChange::Created {
                    id: e.id,
                    path: e.path.clone(),
                }),
                Some(o) => {
                    if o.path != e.path {
                        out.push(TreeChange::Moved {
                            id: e.id,
                            from: o.path.clone(),
                            to: e.path.clone(),
                        });
                    } else if o.meta != e.meta {
                        out.push(TreeChange::Content {
                            id: e.id,
                            path: e.path.clone(),
                        });
                    }
                }
            }
        }
        for o in &old.entries {
            if self.get(o.id).is_none() {
                out.push(TreeChange::Deleted {
                    id: o.id,
                    path: o.path.clone(),
                });
            }
        }
        out
    }

    /// The manifest of this view: every entry, directories included.
    #[must_use]
    pub fn manifest(&self) -> Vec<ManifestEntry> {
        self.entries
            .iter()
            .map(|e| ManifestEntry {
                id: e.id,
                path: e.path.clone(),
                kind: e.meta.kind,
                size: e.meta.size,
                sha256: e.meta.sha256,
                seed_fp: e.meta.seed_fp,
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Loro engine
// ---------------------------------------------------------------------------

const TREE_ID: &str = "files";
const K_NAME: &str = "name";
const K_KIND: &str = "kind";
const K_SIZE: &str = "size";
const K_SHA: &str = "sha";
const K_SEED: &str = "seed";

fn engine_err(e: impl fmt::Display) -> LiveError {
    LiveError::Engine(e.to_string())
}

fn decode_err(e: impl fmt::Display) -> LiveError {
    LiveError::Decode(e.to_string())
}

fn parent_id(parent: Option<FileId>) -> Result<TreeParentId, LiveError> {
    match parent {
        None => Ok(TreeParentId::Root),
        Some(p) => p
            .to_tree()
            .map(TreeParentId::Node)
            .ok_or_else(|| project_err("invalid parent id")),
    }
}

fn read_value(map: &loro::LoroMap, key: &str) -> Option<LoroValue> {
    match map.get(key)? {
        ValueOrContainer::Value(v) => Some(v),
        ValueOrContainer::Container(_) => None,
    }
}

fn read_bytes<const N: usize>(map: &loro::LoroMap, key: &str) -> [u8; N] {
    let mut out = [0u8; N];
    if let Some(LoroValue::Binary(b)) = read_value(map, key)
        && b.len() == N
    {
        out.copy_from_slice(&b);
    }
    out
}

fn write_meta(map: &loro::LoroMap, meta: &NodeMeta) -> Result<(), LiveError> {
    map.insert(K_NAME, meta.name.as_str()).map_err(engine_err)?;
    map.insert(K_KIND, meta.kind.as_str()).map_err(engine_err)?;
    let size = i64::try_from(meta.size).unwrap_or(i64::MAX);
    map.insert(K_SIZE, size).map_err(engine_err)?;
    map.insert(K_SHA, meta.sha256.to_vec())
        .map_err(engine_err)?;
    map.insert(K_SEED, meta.seed_fp.to_vec())
        .map_err(engine_err)?;
    Ok(())
}

fn read_meta(map: &loro::LoroMap) -> NodeMeta {
    let name = match read_value(map, K_NAME) {
        Some(LoroValue::String(s)) => s.to_string(),
        _ => String::new(),
    };
    let kind = match read_value(map, K_KIND) {
        Some(LoroValue::String(s)) => NodeKind::parse(&s),
        _ => NodeKind::Blob,
    };
    let size = match read_value(map, K_SIZE) {
        Some(LoroValue::I64(n)) => u64::try_from(n).unwrap_or(0),
        _ => 0,
    };
    NodeMeta {
        name,
        kind,
        size,
        sha256: read_bytes(map, K_SHA),
        seed_fp: read_bytes(map, K_SEED),
    }
}

/// [`TreeCrdt`] over Loro's movable tree.
pub struct LoroTreeEngine {
    doc: LoroDoc,
    tree: LoroTree,
    peer: u64,
    local_updates: Arc<Mutex<Vec<Vec<u8>>>>,
    _local_sub: Subscription,
}

impl LoroTreeEngine {
    fn build(doc: LoroDoc, peer: u64) -> Result<Self, LiveError> {
        doc.set_peer_id(peer).map_err(engine_err)?;
        let tree = doc.get_tree(TREE_ID);
        let local_updates: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
        let sink = Arc::clone(&local_updates);
        let sub = doc.subscribe_local_update(Box::new(move |bytes| {
            sink.lock().push(bytes.clone());
            true
        }));
        Ok(Self {
            doc,
            tree,
            peer,
            local_updates,
            _local_sub: sub,
        })
    }

    fn lamport_of(&self, id: TreeID) -> u64 {
        self.doc
            .get_change(ID::new(id.peer, id.counter))
            .map_or(0, |c| {
                let offset = u64::try_from(id.counter.saturating_sub(c.id.counter)).unwrap_or(0);
                u64::from(c.lamport) + offset
            })
    }

    fn tree_id(id: FileId) -> Result<TreeID, LiveError> {
        id.to_tree().ok_or_else(|| project_err("invalid file id"))
    }
}

impl TreeCrdt for LoroTreeEngine {
    fn new(peer: u64) -> Result<Self, LiveError> {
        Self::build(LoroDoc::new(), peer)
    }

    fn from_snapshot(peer: u64, bytes: &[u8]) -> Result<Self, LiveError> {
        let e = Self::build(LoroDoc::new(), peer)?;
        e.doc.import(bytes).map_err(decode_err)?;
        Ok(e)
    }

    fn peer(&self) -> u64 {
        self.peer
    }

    fn create(&mut self, parent: Option<FileId>, meta: &NodeMeta) -> Result<FileId, LiveError> {
        let id = self.tree.create(parent_id(parent)?).map_err(engine_err)?;
        let map = self.tree.get_meta(id).map_err(engine_err)?;
        write_meta(&map, meta)?;
        self.doc.commit();
        Ok(FileId::from_tree(id))
    }

    fn set_meta(&mut self, id: FileId, meta: &NodeMeta) -> Result<(), LiveError> {
        let map = self.tree.get_meta(Self::tree_id(id)?).map_err(engine_err)?;
        write_meta(&map, meta)?;
        self.doc.commit();
        Ok(())
    }

    fn move_to(&mut self, id: FileId, parent: Option<FileId>) -> Result<(), LiveError> {
        self.tree
            .mov(Self::tree_id(id)?, parent_id(parent)?)
            .map_err(engine_err)?;
        self.doc.commit();
        Ok(())
    }

    fn delete(&mut self, id: FileId) -> Result<(), LiveError> {
        self.tree.delete(Self::tree_id(id)?).map_err(engine_err)?;
        self.doc.commit();
        Ok(())
    }

    fn nodes(&self) -> Vec<RawNode> {
        self.tree
            .get_nodes(false)
            .into_iter()
            .filter_map(|n| {
                let parent = match n.parent {
                    TreeParentId::Root => None,
                    TreeParentId::Node(p) => Some(FileId::from_tree(p)),
                    _ => return None,
                };
                let map = self.tree.get_meta(n.id).ok()?;
                Some(RawNode {
                    id: FileId::from_tree(n.id),
                    parent,
                    meta: read_meta(&map),
                    born_lamport: self.lamport_of(n.id),
                })
            })
            .collect()
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

    fn apply_update(&mut self, bytes: &[u8]) -> Result<bool, LiveError> {
        let status = self.doc.import(bytes).map_err(decode_err)?;
        Ok(status.pending.is_some())
    }
}

// ---------------------------------------------------------------------------
// ProjectDoc
// ---------------------------------------------------------------------------

/// The project's file tree, replicated.
pub struct ProjectDoc<T: TreeCrdt = LoroTreeEngine> {
    tree: T,
}

impl<T: TreeCrdt> ProjectDoc<T> {
    /// An empty project authored by `peer`.
    pub fn new(peer: u64) -> Result<Self, LiveError> {
        Ok(Self {
            tree: T::new(peer)?,
        })
    }

    /// Restore from [`ProjectDoc::export_snapshot`] output.
    pub fn from_snapshot(peer: u64, bytes: &[u8]) -> Result<Self, LiveError> {
        Ok(Self {
            tree: T::from_snapshot(peer, bytes)?,
        })
    }

    /// This replica's peer id.
    #[must_use]
    pub fn peer(&self) -> u64 {
        self.tree.peer()
    }

    /// The resolved project as of now. O(n log n) in the number of nodes.
    #[must_use]
    pub fn view(&self) -> ProjectView {
        ProjectView::from_nodes(&self.tree.nodes())
    }

    /// Path of a node (see [`ProjectView::path_of`]; take a [`ProjectDoc::view`]
    /// to look up many).
    #[must_use]
    pub fn path_of(&self, id: FileId) -> Option<String> {
        self.view().path_of(id).map(str::to_owned)
    }

    /// Changes since an earlier [`ProjectDoc::view`].
    #[must_use]
    pub fn diff_since(&self, old: &ProjectView) -> Vec<TreeChange> {
        self.view().diff_from(old)
    }

    fn check_name(name: &str) -> Result<(), LiveError> {
        if valid_name(name) {
            Ok(())
        } else {
            Err(project_err(format!("invalid name {name:?}")))
        }
    }

    fn require_dir(view: &ProjectView, parent: Option<FileId>) -> Result<usize, LiveError> {
        let Some(p) = parent else { return Ok(0) };
        match view.get(p) {
            Some(e) if e.meta.kind == NodeKind::Dir => Ok(e.path.matches('/').count() + 1),
            Some(_) => Err(project_err("parent is not a directory")),
            None => Err(project_err("parent does not exist")),
        }
    }

    /// Create a node under `parent` (a live directory, or `None` for the top
    /// level). Returns its id.
    pub fn create(&mut self, parent: Option<FileId>, meta: &NodeMeta) -> Result<FileId, LiveError> {
        Self::check_name(&meta.name)?;
        let view = self.view();
        let depth = Self::require_dir(&view, parent)?;
        if depth >= MAX_DEPTH {
            return Err(project_err("directory nesting too deep"));
        }
        if view.entries().len() >= MAX_NODES {
            return Err(project_err("too many files"));
        }
        self.tree.create(parent, meta)
    }

    /// Rename a node (its last path component).
    pub fn rename(&mut self, id: FileId, name: &str) -> Result<(), LiveError> {
        Self::check_name(name)?;
        let mut meta = self.meta_of(id)?;
        meta.name = name.to_owned();
        self.tree.set_meta(id, &meta)
    }

    /// Move a node under `parent`. Moving a directory into itself or its own
    /// subtree is refused here and, for concurrent moves, ignored by the engine.
    pub fn move_to(&mut self, id: FileId, parent: Option<FileId>) -> Result<(), LiveError> {
        let view = self.view();
        if view.get(id).is_none() {
            return Err(project_err("node does not exist"));
        }
        let depth = Self::require_dir(&view, parent)?;
        // Walk up from the target by raw parents: merged same-named
        // directories share a path, so paths cannot answer this.
        let mut cur = parent;
        let mut hops = 0usize;
        while let Some(c) = cur {
            if c == id {
                return Err(project_err("cannot move a directory into itself"));
            }
            hops += 1;
            if hops > MAX_NODES {
                break;
            }
            cur = view.get(c).and_then(|e| e.parent);
        }
        if depth >= MAX_DEPTH {
            return Err(project_err("directory nesting too deep"));
        }
        self.tree.move_to(id, parent)
    }

    /// Record new content for a file: size, hash and seed fingerprint.
    pub fn set_content(
        &mut self,
        id: FileId,
        size: u64,
        sha256: [u8; 32],
        seed_fp: [u8; 16],
    ) -> Result<(), LiveError> {
        let mut meta = self.meta_of(id)?;
        if meta.kind == NodeKind::Dir {
            return Err(project_err("a directory has no content"));
        }
        if size > MAX_BLOB_BYTES {
            return Err(project_err("file exceeds the size cap"));
        }
        meta.size = size;
        meta.sha256 = sha256;
        meta.seed_fp = seed_fp;
        self.tree.set_meta(id, &meta)
    }

    /// Delete a node and everything under it.
    pub fn delete(&mut self, id: FileId) -> Result<(), LiveError> {
        if self.view().get(id).is_none() {
            return Err(project_err("node does not exist"));
        }
        self.tree.delete(id)
    }

    fn meta_of(&self, id: FileId) -> Result<NodeMeta, LiveError> {
        self.tree
            .nodes()
            .into_iter()
            .find(|n| n.id == id)
            .map(|n| n.meta)
            .ok_or_else(|| project_err("node does not exist"))
    }

    /// Update blobs of local operations since the last call.
    pub fn take_local_updates(&mut self) -> Result<Vec<Vec<u8>>, LiveError> {
        self.tree.take_local_updates()
    }

    /// Encoded version, for the sync exchange.
    #[must_use]
    pub fn version(&self) -> Vec<u8> {
        self.tree.version()
    }

    /// Operations a peer at `version` lacks.
    pub fn export_since(&self, version: &[u8]) -> Result<Vec<u8>, LiveError> {
        self.tree.export_since(version)
    }

    /// Full state, for a disposable sidecar.
    pub fn export_snapshot(&self) -> Result<Vec<u8>, LiveError> {
        self.tree.export_snapshot()
    }

    /// Import remote operations; returns the visible changes and whether some
    /// operations are still waiting for missing ones.
    pub fn apply_update(&mut self, bytes: &[u8]) -> Result<(Vec<TreeChange>, bool), LiveError> {
        let before = self.view();
        let pending = self.tree.apply_update(bytes)?;
        Ok((self.view().diff_from(&before), pending))
    }
}
