//! Save a version, the version timeline, per-version diffs, and the
//! `trailers` / `all` options of `git.commit`.

mod common;

use common::{Repo, git, harness};
use serde_json::json;

#[test]
fn nothing_to_save_is_an_error_not_an_empty_commit() {
    let h = harness();
    let r = Repo::with_commit();
    let before = git(&r.dir, &["rev-parse", "HEAD"]);
    assert_eq!(h.code(&r, "git.save-version", json!({})), "nothing-to-save");
    assert_eq!(git(&r.dir, &["rev-parse", "HEAD"]), before);
}

#[test]
fn saving_stages_everything_and_labels_the_commit() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("a.txt", "one\ntwo\n");
    r.write("new/b.txt", "bee\n");
    let out = h.ok(
        &r,
        "git.save-version",
        json!({"name": "First draft", "message": "Intro written."}),
    );
    assert_eq!(out["files_changed"], 2);
    assert_eq!(out["subject"], "First draft");
    assert_eq!(out["short"].as_str().map(str::len), Some(7));
    let body = git(&r.dir, &["log", "-1", "--format=%B"]);
    assert!(body.starts_with("First draft\n\nIntro written."), "{body}");
    assert!(body.trim_end().ends_with("Manang-Version: 1"), "{body}");
    assert_eq!(git(&r.dir, &["status", "--porcelain"]), "");
}

#[test]
fn a_blank_name_gets_a_default() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("a.txt", "changed\n");
    let out = h.ok(&r, "git.save-version", json!({"name": "   "}));
    let subject = out["subject"].as_str().expect("subject");
    assert!(subject.starts_with("Version of 20"), "{subject}");
    assert!(subject.ends_with(" UTC"), "{subject}");
}

#[test]
fn ignored_files_are_never_staged() {
    let h = harness();
    let r = Repo::with_commit();
    r.write(".gitignore", "build/\n*.log\n");
    r.write("build/out.pdf", "pdf\n");
    r.write("run.log", "log\n");
    r.write("keep.txt", "keep\n");
    let out = h.ok(&r, "git.save-version", json!({"name": "v"}));
    assert_eq!(out["files_changed"], 2); // .gitignore and keep.txt
    let tracked = git(&r.dir, &["ls-files"]);
    assert!(
        !tracked.contains("out.pdf") && !tracked.contains("run.log"),
        "{tracked}"
    );
    assert!(tracked.contains("keep.txt"));
    // Only ignored changes: nothing to save.
    r.write("build/out.pdf", "pdf 2\n");
    assert_eq!(h.code(&r, "git.save-version", json!({})), "nothing-to-save");
}

#[test]
fn bad_names_are_refused() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("a.txt", "x\n");
    assert_eq!(
        h.code(&r, "git.save-version", json!({"name": "two\nlines"})),
        "bad-arguments"
    );
}

#[test]
fn commit_trailers_are_appended_and_validated() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("a.txt", "x\n");
    h.ok(&r, "git.stage", json!({"paths": ["a.txt"]}));
    h.ok(
        &r,
        "git.commit",
        json!({"message": "Edit", "trailers": ["Co-authored-by: Ada <ada@example.org>"]}),
    );
    assert_eq!(
        git(
            &r.dir,
            &[
                "log",
                "-1",
                "--format=%(trailers:key=Co-authored-by,valueonly)"
            ]
        ),
        "Ada <ada@example.org>"
    );
    r.write("a.txt", "y\n");
    for bad in [
        "no colon",
        "Key:novalue",
        "Bad Key: v",
        "-x: v",
        "K: a\nInjected: b",
        "K: \u{7}",
        "K: ",
    ] {
        let code = h.code(
            &r,
            "git.commit",
            json!({"message": "m", "all": true, "trailers": [bad]}),
        );
        assert_eq!(code, "bad-arguments", "{bad:?}");
    }
}

#[test]
fn commit_all_stages_new_and_changed_but_not_ignored() {
    let h = harness();
    let r = Repo::with_commit();
    r.write(".gitignore", "*.aux\n");
    r.write("a.txt", "changed\n");
    r.write("n.txt", "new\n");
    r.write("x.aux", "aux\n");
    h.ok(&r, "git.commit", json!({"message": "all", "all": true}));
    let files = git(&r.dir, &["show", "--name-only", "--format=", "HEAD"]);
    assert!(
        files.contains("a.txt") && files.contains("n.txt") && !files.contains("x.aux"),
        "{files}"
    );
}

#[test]
fn versions_lists_newest_first_with_labels_and_counts() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("a.txt", "one\ntwo\n");
    r.write("b.txt", "bee\n");
    h.ok(&r, "git.save-version", json!({"name": "Second"}));
    r.write("a.txt", "three\n");
    h.ok(&r, "git.save-version", json!({"name": "Third"}));
    let page = h.ok(&r, "git.versions", json!({}));
    let list = page["versions"].as_array().expect("array");
    let subjects: Vec<_> = list
        .iter()
        .map(|v| v["subject"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(subjects, ["Third", "Second", "first"]);
    assert_eq!(list[0]["labelled"], true);
    assert_eq!(list[1]["labelled"], true);
    assert_eq!(list[2]["labelled"], false, "plain commits are not labelled");
    assert_eq!(list[0]["files_changed"], 1);
    assert_eq!(list[1]["files_changed"], 2);
    assert_eq!(list[2]["files_changed"], 1, "root commit counts its files");
    assert_eq!(list[0]["author_name"], "Test User");
    assert!(page["next_cursor"].is_null());
    // Paging and per-file filtering.
    let p1 = h.ok(&r, "git.versions", json!({"limit": 2}));
    assert_eq!(p1["versions"].as_array().map(Vec::len), Some(2));
    let cursor = p1["next_cursor"].as_str().expect("cursor").to_owned();
    let p2 = h.ok(&r, "git.versions", json!({"limit": 2, "cursor": cursor}));
    assert_eq!(p2["versions"][0]["subject"], "first");
    let only_b = h.ok(&r, "git.versions", json!({"path": "b.txt"}));
    assert_eq!(only_b["versions"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        h.code(&r, "git.versions", json!({"cursor": "x"})),
        "bad-arguments"
    );
}

#[test]
fn an_unborn_repository_has_no_versions() {
    let h = harness();
    let r = Repo::unborn();
    let page = h.ok(&r, "git.versions", json!({}));
    assert_eq!(page["versions"], json!([]));
    r.write("a.txt", "x\n");
    let out = h.ok(&r, "git.save-version", json!({"name": "Very first"}));
    assert_eq!(out["files_changed"], 1);
}

#[test]
fn version_files_and_diffs_show_what_changed() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("a.txt", "one\ntwo\n");
    r.write("b.txt", "bee\n");
    let saved = h.ok(&r, "git.save-version", json!({"name": "Second"}));
    let sha = saved["sha"].as_str().expect("sha").to_owned();
    let files = h.ok(&r, "git.version-files", json!({"rev": sha}));
    let list = files["files"].as_array().expect("files");
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["path"], "a.txt");
    assert_eq!(list[0]["status"], "modified");
    assert_eq!(list[0]["additions"], 1);
    assert_eq!(list[1]["status"], "added");
    assert_eq!(list[0]["hunks"], json!([]));
    let diff = h.ok(&r, "git.version-diff", json!({"rev": sha, "path": "a.txt"}));
    assert_eq!(diff["additions"], 1);
    assert_eq!(diff["hunks"][0]["lines"][1]["text"], "two");
    assert_eq!(
        h.code(
            &r,
            "git.version-diff",
            json!({"rev": sha, "path": "zzz.txt"})
        ),
        "path-not-in-rev"
    );
    assert_eq!(
        h.code(&r, "git.version-files", json!({"rev": "--exec=x"})),
        "bad-arguments"
    );
}

#[test]
fn a_version_can_carry_co_author_trailers() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("a.txt", "edited together\n");
    h.ok(
        &r,
        "git.save-version",
        json!({
            "name": "Together",
            "trailers": ["Co-authored-by: Ada <ada@example.org>"],
        }),
    );
    let body = git(&r.dir, &["log", "-1", "--format=%B"]);
    assert!(body.contains("Manang-Version: 1"), "{body}");
    assert!(
        body.trim_end()
            .ends_with("Co-authored-by: Ada <ada@example.org>"),
        "{body}"
    );
    // Still recognised as a version, and a bad trailer is refused.
    r.write("a.txt", "again\n");
    assert_eq!(
        h.code(
            &r,
            "git.save-version",
            json!({"trailers": ["not a trailer\nInjected: yes"]})
        ),
        "bad-arguments"
    );
}
