//! Tree listing on a 20,000-file project (200 directories x 100 files, with a
//! `.gitignore` that excludes a tenth of them).
//!
//! Budget: a full recursive listing must stay well under one frame-budget
//! multiple (target < 100 ms); the lazy per-directory listing the UI uses
//! on expand must stay < 5 ms (measured: ~41 ms / ~3.4 ms / ~0.28 ms).

use std::fs;
use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use manang_project::ProjectRoot;
use manang_project::tree::{self, TreeQuery};

fn build() -> (tempfile::TempDir, ProjectRoot) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let base = tmp.path();
    fs::write(base.join(".gitignore"), "*.log\n").expect("gitignore");
    for d in 0..200 {
        let dir = base.join(format!("section-{d:03}"));
        fs::create_dir_all(&dir).expect("dir");
        for f in 0..100 {
            let ext = if f % 10 == 0 { "log" } else { "typ" };
            fs::write(dir.join(format!("file-{f:03}.{ext}")), "").expect("file");
        }
    }
    let root = ProjectRoot::open(base).expect("root");
    (tmp, root)
}

fn bench(c: &mut Criterion) {
    let (_tmp, root) = build();
    let mut group = c.benchmark_group("tree_20k");
    group.sample_size(20);

    let recursive = TreeQuery {
        recursive: true,
        limit: 1000,
        ..TreeQuery::default()
    };
    group.bench_function("recursive_first_page", |b| {
        b.iter(|| tree::list(black_box(&root), black_box(&recursive)));
    });

    let top = TreeQuery::default();
    group.bench_function("lazy_root", |b| {
        b.iter(|| tree::list(black_box(&root), black_box(&top)));
    });

    let one = TreeQuery {
        path: "section-042".to_owned(),
        ..TreeQuery::default()
    };
    group.bench_function("lazy_one_directory", |b| {
        b.iter(|| tree::list(black_box(&root), black_box(&one)));
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
