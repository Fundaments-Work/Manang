//! File-tree listing that respects `.gitignore` and stays fast on huge trees.
//!
//! Two modes:
//!
//! * **Lazy** (default): one directory level per call. The UI expands folders
//!   on demand, so the cost is proportional to what is on screen.
//! * **Recursive**: a flat, path-sorted list of everything below a directory.
//!   The walk stops at [`WALK_CAP`] entries and flags `truncated`.
//!
//! Both are paged with `offset`/`limit` so a single IPC response stays small.

use std::path::Path;

use ignore::{DirEntry, WalkBuilder};
use serde::{Deserialize, Serialize};

use crate::error::ProjectError;
use crate::paths::{ProjectRoot, STATE_DIR, to_slash};

pub const DEFAULT_LIMIT: usize = 1_000;
pub const MAX_LIMIT: usize = 5_000;
/// Hard stop for a recursive walk.
pub const WALK_CAP: usize = 200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Dir,
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TreeEntry {
    /// Project-relative, `/`-separated.
    pub path: String,
    pub name: String,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TreeQuery {
    /// Directory to list, relative to the project root. Empty is the root.
    pub path: String,
    pub recursive: bool,
    pub offset: usize,
    pub limit: usize,
}

impl Default for TreeQuery {
    fn default() -> Self {
        Self {
            path: String::new(),
            recursive: false,
            offset: 0,
            limit: DEFAULT_LIMIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TreePage {
    pub entries: Vec<TreeEntry>,
    /// Entries found (before paging), at most [`WALK_CAP`].
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    /// More entries exist beyond this page.
    pub has_more: bool,
    /// A recursive walk hit [`WALK_CAP`]; `total` is a lower bound.
    pub truncated: bool,
}

fn builder(root: &Path, dir: &Path, recursive: bool) -> WalkBuilder {
    let root = root.to_path_buf();
    let mut b = WalkBuilder::new(dir);
    b.hidden(false) // show dotfiles such as .gitignore; the filter below drops .git/.manang
        .git_global(false)
        .require_git(false)
        .follow_links(false)
        .max_depth(if recursive { None } else { Some(1) })
        .filter_entry(move |e| {
            // `.git` is internal at any depth (nested repos); `.manang` only
            // at the project root.
            let name = e.file_name();
            name != ".git" && !(name == STATE_DIR && e.path().parent() == Some(root.as_path()))
        });
    b
}

fn kind_of(entry: &DirEntry) -> Option<EntryKind> {
    let ft = entry.file_type()?;
    Some(if ft.is_symlink() {
        EntryKind::Symlink
    } else if ft.is_dir() {
        EntryKind::Dir
    } else {
        EntryKind::File
    })
}

/// List `query.path` under `root`.
pub fn list(root: &ProjectRoot, query: &TreeQuery) -> Result<TreePage, ProjectError> {
    let dir = root.resolve(&query.path)?;
    if !dir.is_dir() {
        return Err(if dir.exists() {
            ProjectError::NotADirectory(query.path.clone())
        } else {
            ProjectError::NotFound(query.path.clone())
        });
    }
    let limit = query.limit.clamp(1, MAX_LIMIT);

    let mut entries: Vec<TreeEntry> = Vec::new();
    let mut truncated = false;
    for result in builder(root.path(), &dir, query.recursive).build() {
        // Unreadable entries are skipped rather than failing the listing.
        let Ok(entry) = result else { continue };
        if entry.depth() == 0 {
            continue;
        }
        let Some(kind) = kind_of(&entry) else {
            continue;
        };
        let Some(rel) = root.relative(entry.path()) else {
            continue;
        };
        let Some(path) = to_slash(&rel) else { continue };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if entries.len() >= WALK_CAP {
            truncated = true;
            break;
        }
        entries.push(TreeEntry { path, name, kind });
    }

    if query.recursive {
        entries.sort_by(|a, b| {
            a.path
                .to_lowercase()
                .cmp(&b.path.to_lowercase())
                .then_with(|| a.path.cmp(&b.path))
        });
    } else {
        // Folders first, then case-insensitive by name.
        entries.sort_by(|a, b| {
            (a.kind != EntryKind::Dir)
                .cmp(&(b.kind != EntryKind::Dir))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.name.cmp(&b.name))
        });
    }

    let total = entries.len();
    let start = query.offset.min(total);
    let end = (start + limit).min(total);
    let page: Vec<TreeEntry> = entries.drain(start..end).collect();
    Ok(TreePage {
        entries: page,
        total,
        offset: start,
        limit,
        has_more: end < total,
        truncated,
    })
}
