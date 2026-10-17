//! Shared parallel file pipeline used by text search and replace preview.
//!
//! Files are processed in path order, in fixed-size chunks. Inside a chunk the
//! work is data-parallel on the Rayon pool; between chunks the budget is
//! re-checked, so a hit on the result cap, the time limit, the byte limit or a
//! cancellation stops scheduling promptly, and early chunks (which hold the
//! alphabetically first files) are always finished first.

use std::fs::File;
use std::sync::atomic::{AtomicU64, Ordering};

use manang_project::ProjectRoot;
use rayon::ThreadPool;
use rayon::prelude::*;
use serde::Serialize;

use crate::budget::Budget;
use crate::index::FileEntry;
use crate::limits::Limits;
use crate::pattern::PathFilter;

/// Files handled per scheduling round.
const CHUNK: usize = 256;

/// Per-search tallies reported back to the UI.
#[derive(Debug, Default)]
pub struct Counters {
    pub searched: AtomicU64,
    pub matched_files: AtomicU64,
    pub binary: AtomicU64,
    pub large: AtomicU64,
    pub unreadable: AtomicU64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub files_searched: u64,
    pub files_with_matches: u64,
    pub files_skipped_binary: u64,
    pub files_skipped_large: u64,
    pub files_unreadable: u64,
    pub bytes_scanned: u64,
    pub elapsed_ms: u64,
}

impl Counters {
    pub fn bump(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self, bytes_scanned: u64, elapsed_ms: u64) -> Stats {
        Stats {
            files_searched: self.searched.load(Ordering::Relaxed),
            files_with_matches: self.matched_files.load(Ordering::Relaxed),
            files_skipped_binary: self.binary.load(Ordering::Relaxed),
            files_skipped_large: self.large.load(Ordering::Relaxed),
            files_unreadable: self.unreadable.load(Ordering::Relaxed),
            bytes_scanned,
            elapsed_ms,
        }
    }
}

/// Outcome of opening an indexed file for reading.
pub enum Opened {
    Ready(File),
    TooLarge,
    /// The byte budget ran out; stop scheduling.
    OverBudget,
    Unreadable,
}

/// Open `entry` for reading, enforcing the per-file size cap and charging its
/// size to the byte budget.
///
/// The path comes from the index, which was built without following symlinks;
/// see the module docs of `manang_project::ProjectRoot` for the (accepted)
/// check-then-use limitation. Anything that is not a regular file is
/// reported as unreadable.
pub fn open(root: &ProjectRoot, entry: &FileEntry, budget: &Budget, limits: &Limits) -> Opened {
    let path = root.path().join(&entry.path);
    let Ok(file) = File::open(path) else {
        return Opened::Unreadable;
    };
    let Ok(meta) = file.metadata() else {
        return Opened::Unreadable;
    };
    if !meta.is_file() {
        return Opened::Unreadable;
    }
    if meta.len() > limits.max_file_bytes {
        return Opened::TooLarge;
    }
    if !budget.claim_bytes(meta.len()) {
        return Opened::OverBudget;
    }
    Opened::Ready(file)
}

/// Run `per_file` over every entry passing `filter`, in parallel and in path
/// order, honouring `budget`. `init` builds per-worker scratch state.
pub fn scan<S, T, I, F>(
    pool: &ThreadPool,
    entries: &[FileEntry],
    filter: &PathFilter,
    budget: &Budget,
    init: I,
    per_file: F,
) -> Vec<T>
where
    T: Send,
    I: Fn() -> S + Sync + Send,
    F: Fn(&mut S, &FileEntry) -> Option<T> + Sync + Send,
{
    let mut out: Vec<T> = Vec::new();
    for chunk in entries.chunks(CHUNK) {
        if budget.should_stop() {
            break;
        }
        let mut part: Vec<T> = pool.install(|| {
            chunk
                .par_iter()
                .map_init(&init, |state, entry| {
                    if budget.should_stop() || !filter.allows(&entry.path) {
                        return None;
                    }
                    per_file(state, entry)
                })
                .flatten_iter()
                .collect()
        });
        out.append(&mut part);
    }
    out
}
