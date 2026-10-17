//! `git.status` and `git.diff` on a 20,000-file repository with 100 edits.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use criterion::{Criterion, criterion_group, criterion_main};
use manang_kernel::{Kernel, Payload};
use serde_json::json;

const FILES: usize = 20_000;
const EDITED: usize = 100;

fn run_git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .is_ok_and(|s| s.success());
    assert!(ok, "git {args:?} failed");
}

fn build_repo(dir: &Path) {
    run_git(dir, &["init", "-q", "-b", "main"]);
    run_git(dir, &["config", "user.name", "Bench"]);
    run_git(dir, &["config", "user.email", "bench@example.com"]);
    for i in 0..FILES {
        let sub = dir.join(format!("d{:03}", i % 200));
        fs::create_dir_all(&sub).expect("mkdir");
        fs::write(
            sub.join(format!("f{i}.txt")),
            format!("line one\nline two {i}\n"),
        )
        .expect("write");
    }
    run_git(dir, &["add", "-A"]);
    run_git(dir, &["commit", "-q", "-m", "bulk"]);
    for i in 0..EDITED {
        let sub = dir.join(format!("d{:03}", i % 200));
        fs::write(
            sub.join(format!("f{i}.txt")),
            format!("line one\nedited {i}\nextra\n"),
        )
        .expect("write");
    }
    for i in 0..20 {
        fs::write(dir.join(format!("untracked{i}.txt")), "u\n").expect("write");
    }
}

fn bench(c: &mut Criterion) {
    let tmp = tempfile::tempdir().expect("tmp");
    build_repo(tmp.path());
    let kernel = Kernel::new();
    kernel
        .register(
            manang_git::manifest().expect("manifest"),
            Arc::new(manang_git::plugin(None)),
        )
        .expect("register");
    let root = tmp.path().to_string_lossy().into_owned();
    let call = |cmd: &str| {
        kernel
            .invoke(cmd, Payload::Json(json!({ "root": root })))
            .expect("command")
    };
    call("git.status");

    let mut group = c.benchmark_group("git_20k_files");
    group.sample_size(10);
    group.bench_function("status", |b| b.iter(|| call("git.status")));
    group.bench_function("diff_unstaged_100_edits", |b| b.iter(|| call("git.diff")));
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
