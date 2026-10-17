//! Targets: first text results over a 20k-file / ~200 MB tree in < 100 ms and
//! a fuzzy file query over 100k paths in < 5 ms.
//!
//! `MANANG_BENCH_FILES` (default 20000) scales the text tree;
//! `TMPDIR` chooses where it is generated.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};
use manang_kernel::{Kernel, Payload};
use manang_search::SearchPlugin;
use manang_search::fuzzy::rank;
use manang_search::index::FileEntry;
use serde_json::{Value, json};

/// Tiny deterministic PRNG so the tree is identical on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

const WORDS: &[&str] = &[
    "alpha",
    "beta",
    "gamma",
    "delta",
    "lemma",
    "theorem",
    "proof",
    "figure",
    "section",
    "equation",
    "citation",
    "matrix",
    "tensor",
    "vector",
    "kernel",
    "buffer",
    "render",
    "compile",
    "layout",
    "typeset",
    "bibliography",
    "abstract",
    "margin",
    "caption",
    "label",
    "résumé",
    "日本語",
    "naïve",
];

fn make_tree(root: &Path, files: usize) -> u64 {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut total = 0u64;
    for i in 0..files {
        let dir = root.join(format!("d{:03}/s{:02}", i % 200, (i / 200) % 50));
        if fs::create_dir_all(&dir).is_err() {
            continue;
        }
        let mut body = String::with_capacity(10_500);
        let mut line = 0;
        while body.len() < 10_000 {
            for _ in 0..(6 + rng.next() % 8) {
                let w = WORDS[(rng.next() % WORDS.len() as u64) as usize];
                let _ = write!(body, "{w} ");
            }
            if i % 40 == 0 && line == 17 {
                body.push_str("the rare_marker_token appears here");
            }
            body.push('\n');
            line += 1;
        }
        total += body.len() as u64;
        let _ = fs::write(dir.join(format!("f{i}.typ")), body);
    }
    total
}

fn call(kernel: &Kernel, command: &str, args: Value) -> Value {
    match kernel.invoke(command, Payload::Json(args)) {
        Ok(Payload::Json(v)) => v,
        _ => Value::Null,
    }
}

fn synthetic_paths(n: usize) -> Vec<FileEntry> {
    let mut rng = Rng(42);
    let dirs = [
        "src",
        "crates",
        "docs",
        "tests",
        "lib",
        "components",
        "assets",
    ];
    let names = [
        "main", "index", "util", "config", "parser", "buffer", "render", "editor", "project",
        "search", "kernel", "plugin",
    ];
    let exts = ["rs", "ts", "svelte", "typ", "md", "toml", "json"];
    let mut v: Vec<String> = (0..n)
        .map(|i| {
            let depth = 2 + (rng.next() % 4) as usize;
            let mut p = String::new();
            for _ in 0..depth {
                let _ = write!(
                    p,
                    "{}{}/",
                    dirs[(rng.next() % dirs.len() as u64) as usize],
                    rng.next() % 40
                );
            }
            let _ = write!(
                p,
                "{}_{}.{}",
                names[(rng.next() % names.len() as u64) as usize],
                i,
                exts[(rng.next() % exts.len() as u64) as usize]
            );
            p
        })
        .collect();
    v.sort_unstable();
    v.into_iter().map(FileEntry::new).collect()
}

fn bench_fuzzy(c: &mut Criterion) {
    let entries = synthetic_paths(100_000);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(
            std::thread::available_parallelism()
                .map_or(2, std::num::NonZeroUsize::get)
                .min(8),
        )
        .build()
        .expect("pool");
    let mut g = c.benchmark_group("fuzzy_100k_paths");
    g.sample_size(50);
    for q in ["main", "srcutil", "comp parser", "x"] {
        g.bench_function(q, |b| b.iter(|| rank(&pool, &entries, q, 50)));
    }
    g.finish();
}

fn bench_text(c: &mut Criterion) {
    let files: usize = std::env::var("MANANG_BENCH_FILES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000);
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().join("tree");
    let started = std::time::Instant::now();
    let bytes = make_tree(&root, files);
    eprintln!(
        "generated {files} files, {:.0} MB in {:.1}s",
        bytes as f64 / 1e6,
        started.elapsed().as_secs_f64()
    );
    let root_s = root.to_string_lossy().into_owned();

    let kernel = Kernel::new();
    kernel
        .register(
            manang_search::manifest().expect("manifest"),
            Arc::new(SearchPlugin::new(None)),
        )
        .expect("register");
    // Cold index build, timed once.
    let t = std::time::Instant::now();
    let r = call(
        &kernel,
        "search.files",
        json!({ "root": root_s, "query": "f19999", "limit": 5 }),
    );
    eprintln!(
        "cold index build + first fuzzy query: {:.1} ms, {} files",
        t.elapsed().as_secs_f64() * 1e3,
        r["total_indexed"]
    );

    // With no event bus the index expires after 2 s; lift that for the run by
    // re-warming inside each iteration only when it has lapsed. Criterion's
    // measurement windows are short, so warm it right before each group.
    let warm = || {
        let _ = call(
            &kernel,
            "search.files",
            json!({ "root": root_s, "query": "x", "limit": 1 }),
        );
    };

    warm();
    let probe = call(
        &kernel,
        "search.text",
        json!({ "root": root_s, "query": "zzzz_never_present", "limit": 100 }),
    );
    eprintln!("full-scan stats: {}", probe["stats"]);

    let mut g = c.benchmark_group("text_search");
    g.sample_size(20).measurement_time(Duration::from_secs(8));
    warm();
    g.bench_function("first_100_results", |b| {
        b.iter(|| {
            call(
                &kernel,
                "search.text",
                json!({ "root": root_s, "query": "rare_marker_token", "limit": 100 }),
            )
        });
    });
    g.sample_size(10);
    g.bench_function("full_scan_literal_rare", |b| {
        b.iter(|| {
            call(
                &kernel,
                "search.text",
                json!({ "root": root_s, "query": "zzzz_never_present", "limit": 100 }),
            )
        });
    });
    g.bench_function("full_scan_regex", |b| {
        b.iter(|| {
            call(
                &kernel,
                "search.text",
                json!({ "root": root_s, "query": "rare_\\w+_token|zz+q", "regex": true, "limit": 100 }),
            )
        });
    });
    g.finish();
}

criterion_group!(benches, bench_fuzzy, bench_text);
criterion_main!(benches);
