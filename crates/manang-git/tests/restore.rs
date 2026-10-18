//! `git.restore`: bring files back from a version without rewriting history
//! and without deleting anything.

mod common;

use common::{Repo, git, harness};
use serde_json::json;

/// Three versions: v1 {a: one, b: bee, sub/c: sea}, v2 changes a and b,
/// v3 adds d and changes sub/c. Returns the repo and the sha of v1.
fn history() -> (Repo, String) {
    let r = Repo::unborn();
    r.write("a.txt", "one\n");
    r.write("b.txt", "bee\n");
    r.write("sub/c.txt", "sea\n");
    r.commit_all("v1");
    let v1 = git(&r.dir, &["rev-parse", "HEAD"]);
    r.write("a.txt", "two\n");
    r.write("b.txt", "bee two\n");
    r.commit_all("v2");
    r.write("d.txt", "dee\n");
    r.write("sub/c.txt", "sea three\n");
    r.commit_all("v3");
    (r, v1)
}

#[test]
fn restores_one_file_and_keeps_history() {
    let h = harness();
    let (r, v1) = history();
    let head = git(&r.dir, &["rev-parse", "HEAD"]);
    let out = h.ok(&r, "git.restore", json!({"rev": v1, "paths": ["a.txt"]}));
    assert_eq!(out["restored"], json!(["a.txt"]));
    assert_eq!(out["untouched_new"], json!([]));
    assert_eq!(r.read("a.txt"), "one\n");
    assert_eq!(r.read("b.txt"), "bee two\n");
    assert_eq!(
        git(&r.dir, &["rev-parse", "HEAD"]),
        head,
        "history untouched"
    );
}

#[test]
fn restores_several_files_and_folders() {
    let h = harness();
    let (r, v1) = history();
    let out = h.ok(
        &r,
        "git.restore",
        json!({"rev": v1, "paths": ["a.txt", "sub"]}),
    );
    assert_eq!(out["restored"], json!(["a.txt", "sub/c.txt"]));
    assert_eq!(r.read("a.txt"), "one\n");
    assert_eq!(r.read("sub/c.txt"), "sea\n");
    assert_eq!(r.read("b.txt"), "bee two\n");
}

#[test]
fn whole_project_never_deletes_new_files() {
    let h = harness();
    let (r, v1) = history();
    let out = h.ok(&r, "git.restore", json!({"rev": v1, "whole": true}));
    assert_eq!(out["restored"], json!(["a.txt", "b.txt", "sub/c.txt"]));
    assert_eq!(out["untouched_new"], json!(["d.txt"]));
    assert_eq!(r.read("a.txt"), "one\n");
    assert_eq!(r.read("b.txt"), "bee\n");
    assert_eq!(
        r.read("d.txt"),
        "dee\n",
        "a file newer than the version stays"
    );
}

#[test]
fn restores_a_deleted_file() {
    let h = harness();
    let (r, v1) = history();
    std::fs::remove_file(r.dir.join("a.txt")).expect("rm");
    let out = h.ok(&r, "git.restore", json!({"rev": v1, "paths": ["a.txt"]}));
    assert_eq!(out["restored"], json!(["a.txt"]));
    assert_eq!(r.read("a.txt"), "one\n");
}

#[test]
fn dirty_files_are_refused_with_their_names() {
    let h = harness();
    let (r, v1) = history();
    r.write("a.txt", "unsaved work\n");
    let err = h
        .call(
            &r,
            "git.restore",
            json!({"rev": v1, "paths": ["a.txt", "b.txt"]}),
        )
        .expect_err("dirty");
    assert_eq!(err["code"], "dirty-tree");
    assert_eq!(err["paths"], json!(["a.txt"]));
    assert_eq!(r.read("a.txt"), "unsaved work\n", "nothing was touched");
    // A clean file elsewhere restores fine while a.txt stays dirty.
    h.ok(&r, "git.restore", json!({"rev": v1, "paths": ["b.txt"]}));
    assert_eq!(r.read("b.txt"), "bee\n");
}

#[test]
fn save_first_makes_a_safety_version_and_a_trash_copy() {
    let h = harness();
    let (r, v1) = history();
    r.write("a.txt", "unsaved work\n");
    let out = h.ok(
        &r,
        "git.restore",
        json!({"rev": v1, "paths": ["a.txt"], "save_first": true}),
    );
    let safety = out["safety_version"]
        .as_str()
        .expect("safety sha")
        .to_owned();
    assert_eq!(r.read("a.txt"), "one\n");
    assert_eq!(git(&r.dir, &["rev-parse", "HEAD"]), safety);
    let short = &v1[..7];
    assert_eq!(
        git(&r.dir, &["log", "-1", "--format=%s"]),
        format!("Before restoring {short}")
    );
    assert!(git(&r.dir, &["log", "-1", "--format=%B"]).contains("Manang-Version: 1"));
    // The unsaved text is both in the safety commit and in the trash.
    assert_eq!(
        git(&r.dir, &["show", &format!("{safety}:a.txt")]),
        "unsaved work"
    );
    let trash = out["trash"].as_str().expect("trash dir");
    assert!(trash.contains("manang-trash"), "{trash}");
    let copy = std::fs::read_to_string(std::path::Path::new(trash).join("a.txt")).expect("copy");
    assert_eq!(copy, "unsaved work\n");
}

#[test]
fn trash_copy_is_made_even_for_clean_overwrites() {
    let h = harness();
    let (r, v1) = history();
    let out = h.ok(&r, "git.restore", json!({"rev": v1, "paths": ["a.txt"]}));
    let trash = out["trash"].as_str().expect("trash dir");
    let copy = std::fs::read_to_string(std::path::Path::new(trash).join("a.txt")).expect("copy");
    assert_eq!(copy, "two\n");
}

#[test]
fn a_file_that_is_not_in_the_version_is_an_error() {
    let h = harness();
    let (r, v1) = history();
    assert_eq!(
        h.code(&r, "git.restore", json!({"rev": v1, "paths": ["d.txt"]})),
        "path-not-in-rev"
    );
}

#[test]
fn identical_files_are_not_rewritten() {
    let h = harness();
    let (r, _) = history();
    let out = h.ok(&r, "git.restore", json!({"rev": "HEAD", "whole": true}));
    assert_eq!(out["restored"], json!([]));
    assert!(out["trash"].is_null());
}

#[test]
fn revisions_and_paths_are_validated() {
    let h = harness();
    let (r, v1) = history();
    for rev in [
        "--exec=touch pwned",
        "-h",
        "",
        "HEAD..main",
        "a b",
        "x;y",
        "$(id)",
    ] {
        assert_eq!(
            h.code(&r, "git.restore", json!({"rev": rev, "paths": ["a.txt"]})),
            "bad-arguments",
            "{rev:?}"
        );
    }
    assert_eq!(
        h.code(
            &r,
            "git.restore",
            json!({"rev": "nosuchrev", "whole": true})
        ),
        "rev-not-found"
    );
    for bad in ["../x", "a/../../x", "/etc/passwd", ".git/config"] {
        assert_eq!(
            h.code(&r, "git.restore", json!({"rev": v1, "paths": [bad]})),
            "bad-path",
            "{bad:?}"
        );
    }
    assert_eq!(
        h.code(&r, "git.restore", json!({"rev": v1, "paths": ["--exec=x"]})),
        "path-not-in-rev",
        "an option-looking path is only ever a literal name"
    );
    assert!(!r.dir.join("pwned").exists());
    assert_eq!(
        h.code(&r, "git.restore", json!({"rev": v1})),
        "bad-arguments",
        "needs paths or whole"
    );
}

#[test]
fn a_project_inside_a_larger_repository_restores_only_its_files() {
    let h = harness();
    let r = Repo::unborn();
    r.write("proj/a.txt", "one\n");
    r.write("other.txt", "other one\n");
    r.commit_all("v1");
    let v1 = git(&r.dir, &["rev-parse", "HEAD"]);
    r.write("proj/a.txt", "two\n");
    r.write("other.txt", "other two\n");
    r.commit_all("v2");
    let inner = Repo {
        dir: r.dir.join("proj"),
        tmp: tempfile::tempdir().expect("tmp"),
    };
    let out = h.ok(&inner, "git.restore", json!({"rev": v1, "whole": true}));
    assert_eq!(out["restored"], json!(["a.txt"]));
    assert_eq!(r.read("proj/a.txt"), "one\n");
    assert_eq!(r.read("other.txt"), "other two\n");
}
