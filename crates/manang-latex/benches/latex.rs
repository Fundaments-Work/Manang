//! Log parsing throughput, and (when a TeX bundle is cached) the end-to-end
//! warm compile through the worker process.
//!
//! The worker binary is looked up via `MANANG_LATEX_WORKER`, else next to the
//! bench executable's `target/<profile>` directory; build it first with
//! `cargo build --release -p manang-latex`.

use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use manang_latex::diagnostics::parse_tex_log;
use manang_latex::{CompileSpec, LatexConfig, compile, resolve_cache_dir};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(path).unwrap_or_default()
}

fn parser(c: &mut Criterion) {
    // A realistic 10k-line log: the real fixtures repeated.
    let mut log = String::new();
    while log.lines().count() < 10_000 {
        for name in [
            "pdftex_nonstop.log",
            "boxes.log",
            "refs.log",
            "multi_file.log",
        ] {
            log.push_str(&fixture(name));
        }
    }
    let mut group = c.benchmark_group("log_parser");
    group.throughput(Throughput::Bytes(log.len() as u64));
    group.bench_function("parse_tex_log_10k_lines", |b| {
        b.iter(|| black_box(parse_tex_log(black_box(&log), "main.tex")))
    });
    group.finish();
}

fn warm_compile(c: &mut Criterion) {
    let config = LatexConfig::default();
    let Ok(cache) = resolve_cache_dir(&config) else {
        return;
    };
    if !cache.join("bundles").is_dir() {
        eprintln!("SKIP warm_compile: no cached bundle in {}", cache.display());
        return;
    }
    let mut sources = BTreeMap::new();
    sources.insert(
        "main.tex".to_owned(),
        "\\documentclass{article}\n\\begin{document}\nHello $E=mc^2$.\n\\end{document}\n"
            .to_owned(),
    );
    let mut spec = CompileSpec::new("main.tex", &config);
    spec.sources = Some(sources);
    // Prime the format cache so the measurement is the warm path.
    if compile(&config, &spec).is_err() {
        eprintln!("SKIP warm_compile: engine unavailable");
        return;
    }
    let mut group = c.benchmark_group("compile");
    group
        .sample_size(10)
        .measurement_time(Duration::from_secs(20));
    group.bench_function("warm_tiny_article", |b| {
        b.iter(|| black_box(compile(&config, &spec)))
    });
    group.finish();
}

criterion_group!(benches, parser, warm_compile);
criterion_main!(benches);
