//! Arbitrary patterns, queries and texts must never panic, hang or exceed
//! the configured caps.

use std::fs;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use manang_kernel::{Kernel, Payload};
use manang_search::fuzzy::rank;
use manang_search::index::FileEntry;
use manang_search::intext::find;
use manang_search::pattern::PatternSpec;
use manang_search::{Limits, SearchPlugin};
use proptest::prelude::*;
use serde_json::{Value, json};

struct Fixture {
    _tmp: tempfile::TempDir,
    root: String,
    kernel: Kernel,
}

static FIXTURE: LazyLock<Fixture> = LazyLock::new(|| {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("p");
    fs::create_dir_all(root.join("sub")).expect("dirs");
    fs::write(
        root.join("a.txt"),
        "alpha beta\n😀 gamma e\u{301}\nfoo(bar)\n[x]\n",
    )
    .expect("a");
    fs::write(root.join("sub/b.txt"), "aaaa\nbbbb\n\\d+ $ ^\n").expect("b");
    let kernel = Kernel::new();
    let limits = Limits {
        max_results: 25,
        max_duration: Duration::from_secs(2),
        ..Limits::default()
    };
    kernel
        .register(
            manang_search::manifest().expect("manifest"),
            Arc::new(SearchPlugin::with_limits(None, limits)),
        )
        .expect("register");
    Fixture {
        root: root.to_string_lossy().into_owned(),
        _tmp: tmp,
        kernel,
    }
});

fn call(command: &str, args: Value) -> Result<Value, String> {
    match FIXTURE.kernel.invoke(command, Payload::Json(args)) {
        Ok(Payload::Json(v)) => Ok(v),
        Ok(_) => Ok(Value::Null),
        Err(e) => Err(e.to_string()),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn text_search_never_panics_or_exceeds_caps(
        query in "\\PC{0,40}",
        regex in any::<bool>(),
        case_sensitive in any::<bool>(),
        whole_word in any::<bool>(),
        ctx in 0usize..50,
        limit in proptest::option::of(0usize..100_000),
    ) {
        let started = Instant::now();
        let r = call("search.text", json!({
            "root": FIXTURE.root, "query": query, "regex": regex,
            "case_sensitive": case_sensitive, "whole_word": whole_word,
            "context_lines": ctx, "limit": limit,
        }));
        prop_assert!(started.elapsed() < Duration::from_secs(5));
        if let Ok(v) = r {
            prop_assert!(v["total_matches"].as_u64().unwrap_or(u64::MAX) <= 25);
            for f in v["files"].as_array().into_iter().flatten() {
                for m in f["matches"].as_array().into_iter().flatten() {
                    prop_assert!(m["before"].as_array().map_or(0, Vec::len) <= 10);
                    prop_assert!(m["after"].as_array().map_or(0, Vec::len) <= 10);
                    let text_len = m["text"].as_str().map_or(0, |t| t.chars().count());
                    for rg in m["ranges"].as_array().into_iter().flatten() {
                        let (s, e) = (rg["start"].as_u64().unwrap_or(0), rg["end"].as_u64().unwrap_or(0));
                        prop_assert!(s < e && e as usize <= text_len);
                    }
                }
            }
        }
    }

    #[test]
    fn replace_preview_never_panics_or_touches_disk(
        query in "\\PC{0,30}",
        replacement in "\\PC{0,30}",
        regex in any::<bool>(),
    ) {
        let before = fs::read(format!("{}/a.txt", FIXTURE.root)).expect("read");
        let _ = call("search.replace-preview", json!({
            "root": FIXTURE.root, "query": query, "regex": regex, "replacement": replacement,
        }));
        prop_assert_eq!(fs::read(format!("{}/a.txt", FIXTURE.root)).expect("read"), before);
    }

    #[test]
    fn in_text_offsets_are_valid_scalar_ranges(
        text in "\\PC{0,200}",
        query in "\\PC{0,12}",
        regex in any::<bool>(),
        case_sensitive in any::<bool>(),
        whole_word in any::<bool>(),
        limit in 1usize..50,
    ) {
        let spec = PatternSpec { query: &query, regex, case_sensitive, whole_word };
        if let Ok(out) = find(&text, &spec, limit, &Limits::default()) {
            prop_assert!(out.matches.len() <= limit);
            let total = text.chars().count();
            let mut prev_end = 0;
            for m in &out.matches {
                prop_assert!(m.start < m.end && m.end <= total);
                prop_assert!(m.start >= prev_end);
                prop_assert!(m.line >= 1 && m.column >= 1);
                prev_end = m.end;
            }
        }
    }

    #[test]
    fn literal_in_text_matches_naive_oracle(
        text in "[ab\u{301}😀 \n]{0,60}",
        query in "[ab😀]{1,3}",
    ) {
        let spec = PatternSpec { query: &query, regex: false, case_sensitive: true, whole_word: false };
        let out = find(&text, &spec, 1000, &Limits::default()).expect("find");
        let chars: Vec<char> = text.chars().collect();
        let q: Vec<char> = query.chars().collect();
        let mut expected = Vec::new();
        let mut i = 0;
        while i + q.len() <= chars.len() {
            if chars[i..i + q.len()] == q[..] {
                expected.push((i, i + q.len()));
                i += q.len();
            } else {
                i += 1;
            }
        }
        let got: Vec<(usize, usize)> = out.matches.iter().map(|m| (m.start, m.end)).collect();
        prop_assert_eq!(got, expected);
    }

    #[test]
    fn fuzzy_rank_never_panics_and_indices_are_in_bounds(
        paths in proptest::collection::vec("[a-zA-Z0-9_./é日]{1,30}", 0..40),
        query in "\\PC{0,15}",
        limit in 1usize..20,
    ) {
        let entries: Vec<FileEntry> = paths.into_iter().map(FileEntry::new).collect();
        let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build().expect("pool");
        let hits = rank(&pool, &entries, &query, limit);
        prop_assert!(hits.len() <= limit);
        for h in &hits {
            let n = entries[h.entry].path.chars().count();
            prop_assert!(h.indices.iter().all(|i| (*i as usize) < n));
            prop_assert!(h.indices.windows(2).all(|w| w[0] < w[1]));
        }
    }
}
