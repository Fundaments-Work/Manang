//! Fuzzy file-name ranking on top of `nucleo-matcher`.
//!
//! Each candidate is scored twice: against the whole relative path (nucleo's
//! path mode rewards matches right after `/` and at word starts) and against
//! the bare file name. A hit on the file name is boosted so that `main` ranks
//! `src/main.rs` above `domain/anything/mine.rs`, and exact / prefix name
//! matches are boosted further. Ties fall back to shorter, then
//! alphabetical, paths for stable output.
//!
//! Scoring is data-parallel on the caller-supplied Rayon pool, with one
//! `Matcher` per worker thread (a matcher owns a sizeable scratch buffer, so
//! it must not be created per item). Indices are only computed for the final
//! `limit` hits.

use std::cell::RefCell;
use std::cmp::Reverse;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use rayon::ThreadPool;
use rayon::prelude::*;

use crate::index::FileEntry;

/// Added to a file-name score so name hits outrank deep-path hits.
const NAME_BONUS: u32 = 128;
/// Added when the file name starts with the query (case-insensitive).
const PREFIX_BONUS: u32 = 256;
/// Added when the file name, or its stem, equals the query.
const EXACT_BONUS: u32 = 512;

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::new());
}

struct Scratch {
    matcher: Matcher,
    buf: Vec<char>,
}

impl Scratch {
    fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
            buf: Vec::new(),
        }
    }
}

/// One ranked file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Index into the entry slice that was searched.
    pub entry: usize,
    pub score: u32,
    /// Unicode-scalar indices into the path, ascending, for highlighting.
    pub indices: Vec<u32>,
}

fn name_bonus(name: &str, query: &str) -> u32 {
    // Only meaningful for a single-atom query.
    if query.is_empty() || query.contains(char::is_whitespace) {
        return 0;
    }
    let name = name.to_lowercase();
    let query = query.to_lowercase();
    let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
    if name == query || stem == query {
        EXACT_BONUS
    } else if name.starts_with(&query) {
        PREFIX_BONUS
    } else {
        0
    }
}

fn score_entry(pattern: &Pattern, query: &str, entry: &FileEntry) -> Option<u32> {
    SCRATCH.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        let Scratch { matcher, buf } = &mut *guard;
        let path = pattern.score(Utf32Str::new(&entry.path, buf), matcher);
        let name = pattern.score(Utf32Str::new(entry.name(), buf), matcher);
        let boosted = name.map(|n| {
            n.saturating_mul(2)
                .saturating_add(NAME_BONUS)
                .saturating_add(name_bonus(entry.name(), query))
        });
        match (path, boosted) {
            (None, None) => None,
            (p, b) => Some(p.unwrap_or(0).max(b.unwrap_or(0))),
        }
    })
}

fn indices_for(pattern: &Pattern, entry: &FileEntry) -> Vec<u32> {
    SCRATCH.with(|cell| {
        let Ok(mut guard) = cell.try_borrow_mut() else {
            return Vec::new();
        };
        let Scratch { matcher, buf } = &mut *guard;
        let mut out: Vec<u32> = Vec::new();
        // Prefer highlighting inside the file name when it matches on its own.
        let in_name = pattern
            .indices(Utf32Str::new(entry.name(), buf), matcher, &mut out)
            .is_some();
        if in_name {
            let prefix = entry.path[..entry.name_start].chars().count();
            let shift = u32::try_from(prefix).unwrap_or(u32::MAX);
            for i in &mut out {
                *i = i.saturating_add(shift);
            }
        } else {
            out.clear();
            let _ = pattern.indices(Utf32Str::new(&entry.path, buf), matcher, &mut out);
        }
        out.sort_unstable();
        out.dedup();
        out
    })
}

/// Rank `entries` against `query`, best first, at most `limit` hits.
///
/// An empty (or whitespace-only) query returns the first `limit` entries in
/// path order with no highlight indices.
pub fn rank(pool: &ThreadPool, entries: &[FileEntry], query: &str, limit: usize) -> Vec<Hit> {
    let query = query.trim();
    if query.is_empty() {
        return entries
            .iter()
            .enumerate()
            .take(limit)
            .map(|(entry, _)| Hit {
                entry,
                score: 0,
                indices: Vec::new(),
            })
            .collect();
    }
    let pattern = Pattern::new(
        query,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );

    let mut scored: Vec<(u32, usize)> = pool.install(|| {
        entries
            .par_iter()
            .enumerate()
            .filter_map(|(i, e)| score_entry(&pattern, query, e).map(|s| (s, i)))
            .collect()
    });

    let cmp = |a: &(u32, usize), b: &(u32, usize)| {
        let (ea, eb) = (&entries[a.1], &entries[b.1]);
        Reverse(a.0)
            .cmp(&Reverse(b.0))
            .then_with(|| ea.path.len().cmp(&eb.path.len()))
            .then_with(|| ea.path.cmp(&eb.path))
    };
    if scored.len() > limit {
        scored.select_nth_unstable_by(limit, cmp);
        scored.truncate(limit);
    }
    scored.sort_unstable_by(cmp);

    scored
        .into_iter()
        .map(|(score, entry)| Hit {
            entry,
            score,
            indices: indices_for(&pattern, &entries[entry]),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rayon::ThreadPoolBuilder;

    fn pool() -> ThreadPool {
        ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("pool")
    }

    fn entries(paths: &[&str]) -> Vec<FileEntry> {
        paths
            .iter()
            .map(|p| FileEntry::new((*p).to_owned()))
            .collect()
    }

    fn top(paths: &[&str], q: &str) -> Vec<String> {
        let e = entries(paths);
        rank(&pool(), &e, q, 10)
            .into_iter()
            .map(|h| e[h.entry].path.clone())
            .collect()
    }

    #[test]
    fn basename_beats_deep_path() {
        let got = top(
            &[
                "domain/anything/mine.rs",
                "src/main.rs",
                "docs/maintenance/x.md",
            ],
            "main",
        );
        assert_eq!(got.first().map(String::as_str), Some("src/main.rs"));
    }

    #[test]
    fn exact_name_wins_over_longer() {
        let got = top(
            &["a/readme-old.md", "readme.md", "x/readme.md.bak"],
            "readme.md",
        );
        assert_eq!(got.first().map(String::as_str), Some("readme.md"));
    }

    #[test]
    fn path_segments_match() {
        let got = top(
            &["src/lib/util.rs", "tests/util.rs", "other.rs"],
            "src util",
        );
        assert_eq!(got, vec!["src/lib/util.rs".to_owned()]);
    }

    #[test]
    fn indices_point_at_matched_chars() {
        let e = entries(&["src/main.rs"]);
        let hits = rank(&pool(), &e, "main", 5);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].indices, vec![4, 5, 6, 7]);
    }

    #[test]
    fn indices_are_scalar_not_byte_offsets() {
        let e = entries(&["図/é/日本語.typ"]);
        let hits = rank(&pool(), &e, "日本", 5);
        assert_eq!(hits.len(), 1);
        // "図/é/" is 4 scalars.
        assert_eq!(hits[0].indices, vec![4, 5]);
    }

    #[test]
    fn empty_query_lists_in_order() {
        let e = entries(&["a", "b", "c"]);
        let hits = rank(&pool(), &e, "  ", 2);
        assert_eq!(hits.iter().map(|h| h.entry).collect::<Vec<_>>(), vec![0, 1]);
    }

    #[test]
    fn no_match_is_empty() {
        assert!(top(&["a.rs", "b.rs"], "zzz").is_empty());
    }

    #[test]
    fn limit_is_respected() {
        let paths: Vec<String> = (0..50).map(|i| format!("dir/file{i}.rs")).collect();
        let e: Vec<FileEntry> = paths.into_iter().map(FileEntry::new).collect();
        assert_eq!(rank(&pool(), &e, "file", 7).len(), 7);
    }
}
