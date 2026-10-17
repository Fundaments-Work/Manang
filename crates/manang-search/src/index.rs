//! In-memory file index: built lazily per project root, cached, and
//! invalidated by `project.changed` events.
//!
//! The index lists regular files only (symlinks are never followed or
//! listed), honouring `.gitignore`/`.ignore` without requiring a git repo, and
//! always excluding `.git` and the root-level `.manang` state directory.
//!
//! The walk uses the `ignore` crate's own parallel walker (I/O-bound
//! directory reads on its threads); CPU-bound matching runs on the Rayon pool.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ignore::{WalkBuilder, WalkState};
use manang_project::{ProjectRoot, to_slash};
use parking_lot::Mutex;

/// Root-level directory holding Manang's own state.
const STATE_DIR: &str = ".manang";
/// Roots kept cached at once.
const MAX_CACHED_ROOTS: usize = 4;

/// One indexed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Project-relative, `/`-separated.
    pub path: String,
    /// Byte offset where the file name starts inside `path`.
    pub name_start: usize,
}

impl FileEntry {
    pub fn new(path: String) -> Self {
        let name_start = path.rfind('/').map_or(0, |i| i + 1);
        Self { path, name_start }
    }

    pub fn name(&self) -> &str {
        &self.path[self.name_start..]
    }
}

/// A snapshot of one root's files, sorted by path.
#[derive(Debug)]
pub struct FileIndex {
    pub entries: Vec<FileEntry>,
    /// The walk stopped at the entry cap.
    pub truncated: bool,
    built: Instant,
    pub build_time: Duration,
}

impl FileIndex {
    /// Build an index by walking `root`.
    pub fn build(root: &ProjectRoot, include_hidden: bool, max_entries: usize) -> Self {
        let started = Instant::now();
        let root_path = root.path().to_path_buf();
        let mut builder = WalkBuilder::new(&root_path);
        let filter_root = root_path.clone();
        builder
            .hidden(!include_hidden)
            .git_global(false)
            .require_git(false)
            .follow_links(false)
            .filter_entry(move |e| {
                let name = e.file_name();
                name != ".git"
                    && !(name == STATE_DIR && e.path().parent() == Some(filter_root.as_path()))
            });

        let sink: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let count = Arc::new(AtomicUsize::new(0));
        let truncated = Arc::new(AtomicBool::new(false));

        builder.build_parallel().run(|| {
            let mut local = Collector {
                sink: sink.clone(),
                buf: Vec::new(),
            };
            let root_path = root_path.clone();
            let count = count.clone();
            let truncated = truncated.clone();
            Box::new(move |result| {
                let Ok(entry) = result else {
                    return WalkState::Continue;
                };
                if !entry.file_type().is_some_and(|t| t.is_file()) {
                    return WalkState::Continue;
                }
                let Ok(rel) = entry.path().strip_prefix(&root_path) else {
                    return WalkState::Continue;
                };
                let Some(path) = to_slash(rel) else {
                    // Non-UTF-8 names cannot cross the JSON boundary.
                    return WalkState::Continue;
                };
                if count.fetch_add(1, Ordering::Relaxed) >= max_entries {
                    truncated.store(true, Ordering::Relaxed);
                    return WalkState::Quit;
                }
                local.buf.push(path);
                WalkState::Continue
            })
        });

        let mut paths = std::mem::take(&mut *sink.lock());
        paths.sort_unstable();
        paths.dedup();
        Self {
            entries: paths.into_iter().map(FileEntry::new).collect(),
            truncated: truncated.load(Ordering::Relaxed),
            built: started,
            build_time: started.elapsed(),
        }
    }

    /// Build from explicit paths (tests, benchmarks).
    pub fn from_paths(paths: impl IntoIterator<Item = String>) -> Self {
        let mut v: Vec<String> = paths.into_iter().collect();
        v.sort_unstable();
        let now = Instant::now();
        Self {
            entries: v.into_iter().map(FileEntry::new).collect(),
            truncated: false,
            built: now,
            build_time: Duration::ZERO,
        }
    }
}

/// Flushes a walker thread's batch into the shared list when the thread ends.
struct Collector {
    sink: Arc<Mutex<Vec<String>>>,
    buf: Vec<String>,
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.sink.lock().append(&mut self.buf);
    }
}

type Key = (PathBuf, bool);

/// Per-root cache of [`FileIndex`] snapshots.
pub struct IndexCache {
    map: Mutex<HashMap<Key, Arc<FileIndex>>>,
    /// Without an event bus nothing tells us the tree changed, so snapshots
    /// expire after this long. `None` means "trust the events".
    ttl: Option<Duration>,
    max_entries: usize,
}

impl IndexCache {
    pub fn new(ttl: Option<Duration>, max_entries: usize) -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            ttl,
            max_entries,
        }
    }

    /// Cached snapshot for `root`, building it if absent or expired.
    pub fn get(&self, root: &ProjectRoot, include_hidden: bool) -> Arc<FileIndex> {
        let key = (root.path().to_path_buf(), include_hidden);
        if let Some(hit) = self.map.lock().get(&key) {
            let fresh = self.ttl.is_none_or(|ttl| hit.built.elapsed() < ttl);
            if fresh {
                return hit.clone();
            }
        }
        // Build outside the lock: walks can take a while.
        let built = Arc::new(FileIndex::build(root, include_hidden, self.max_entries));
        let mut map = self.map.lock();
        if map.len() >= MAX_CACHED_ROOTS && !map.contains_key(&key) {
            let oldest = map
                .iter()
                .min_by_key(|(_, v)| v.built)
                .map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                map.remove(&k);
            }
        }
        map.insert(key, built.clone());
        built
    }

    /// Drop every snapshot of `root`.
    pub fn invalidate(&self, root: &Path) {
        self.map.lock().retain(|(r, _), _| r != root);
    }

    pub fn invalidate_all(&self) {
        self.map.lock().clear();
    }

    pub fn len(&self) -> usize {
        self.map.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_name_split() {
        let e = FileEntry::new("a/b/c.typ".to_owned());
        assert_eq!(e.name(), "c.typ");
        let e = FileEntry::new("main.typ".to_owned());
        assert_eq!(e.name(), "main.typ");
    }

    #[test]
    fn from_paths_sorts() {
        let i = FileIndex::from_paths(["b".to_owned(), "a".to_owned()]);
        assert_eq!(i.entries[0].path, "a");
    }
}
