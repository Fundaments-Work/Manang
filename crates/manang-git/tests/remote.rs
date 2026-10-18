//! Share and get updates, driven through the kernel with two clones of a
//! local bare remote.

mod common;

use common::{Harness, Repo, git, harness};
use serde_json::json;

struct Pair {
    remote: std::path::PathBuf,
    a: Repo,
    b: Repo,
}

/// A bare remote, clone A holding the first version (already shared) and
/// clone B cloned from it.
fn pair(h: &Harness) -> Pair {
    let a = Repo::with_commit();
    let remote = a.tmp.path().join("remote.git");
    git(
        a.tmp.path(),
        &["init", "-q", "--bare", "-b", "main", "remote.git"],
    );
    h.ok(
        &a,
        "git.remote-set",
        json!({"url": remote.to_string_lossy()}),
    );
    h.ok(&a, "git.share", json!({}));
    let b_dir = a.tmp.path().join("clone-b");
    git(
        a.tmp.path(),
        &["clone", "-q", remote.to_str().expect("utf8"), "clone-b"],
    );
    let b = Repo {
        tmp: tempfile::tempdir().expect("unused"),
        dir: b_dir.canonicalize().expect("canon"),
    };
    b.identity();
    Pair { remote, a, b }
}

fn status(h: &Harness, r: &Repo) -> serde_json::Value {
    h.ok(r, "git.sync-status", json!({}))
}

#[test]
fn no_remote_is_reported() {
    let h = harness();
    let r = Repo::with_commit();
    assert_eq!(status(&h, &r)["state"], "no-remote");
    assert_eq!(h.ok(&r, "git.remote-get", json!({}))["url"], json!(null));
    assert_eq!(h.code(&r, "git.share", json!({})), "no-remote");
    assert_eq!(h.code(&r, "git.get-updates", json!({})), "no-remote");
}

#[test]
fn rejects_odd_addresses() {
    let h = harness();
    let r = Repo::with_commit();
    for bad in ["", "--upload-pack=x", "ext::sh -c id", "a b"] {
        assert_eq!(
            h.code(&r, "git.remote-set", json!({"url": bad})),
            "bad-remote-url",
            "{bad:?}"
        );
    }
}

#[test]
fn share_sets_upstream_and_status_follows() {
    let h = harness();
    let a = Repo::with_commit();
    let remote = a.tmp.path().join("remote.git");
    git(
        a.tmp.path(),
        &["init", "-q", "--bare", "-b", "main", "remote.git"],
    );
    let url = remote.to_string_lossy().into_owned();
    let got = h.ok(&a, "git.remote-set", json!({"url": url}));
    assert_eq!(got["name"], "origin");
    assert_eq!(got["url"], json!(url));
    let st = status(&h, &a);
    assert_eq!(
        (st["state"].as_str(), st["never_shared"].as_bool()),
        (Some("ahead"), Some(true))
    );
    assert_eq!(st["ahead"], 1);
    let shared = h.ok(&a, "git.share", json!({}));
    assert_eq!(shared["branch"], "main");
    assert_eq!(shared["pushed"], 1);
    assert_eq!(
        git(&remote, &["rev-parse", "main"]),
        git(&a.dir, &["rev-parse", "HEAD"])
    );
    assert_eq!(status(&h, &a)["state"], "up-to-date");
    a.write("a.txt", "one\ntwo\n");
    a.commit_all("second");
    let st = status(&h, &a);
    assert_eq!(
        (st["state"].as_str(), st["ahead"].as_u64()),
        (Some("ahead"), Some(1))
    );
    a.write("scratch.txt", "x");
    assert_eq!(status(&h, &a)["uncommitted"], 1);
}

#[test]
fn fast_forward_brings_the_new_version() {
    let h = harness();
    let p = pair(&h);
    p.a.write("a.txt", "one\nfrom a\n");
    p.a.commit_all("from a");
    h.ok(&p.a, "git.share", json!({}));
    assert_eq!(status(&h, &p.b)["state"], "up-to-date", "no network yet");
    let refreshed = h.ok(&p.b, "git.sync-status", json!({"refresh": true}));
    assert_eq!(
        (refreshed["state"].as_str(), refreshed["behind"].as_u64()),
        (Some("behind"), Some(1))
    );
    let out = h.ok(&p.b, "git.get-updates", json!({"strategy": "ff-only"}));
    assert_eq!(out["result"], "fast-forwarded");
    assert_eq!(out["received"], 1);
    assert_eq!(p.b.read("a.txt"), "one\nfrom a\n");
    assert_eq!(status(&h, &p.b)["state"], "up-to-date");
    let again = h.ok(&p.b, "git.get-updates", json!({}));
    assert_eq!(again["result"], "up-to-date");
}

#[test]
fn diverged_clean_merge_combines_both_sides() {
    let h = harness();
    let p = pair(&h);
    p.a.write("only-a.txt", "a\n");
    p.a.commit_all("a adds");
    h.ok(&p.a, "git.share", json!({}));
    p.b.write("only-b.txt", "b\n");
    p.b.commit_all("b adds");

    let refreshed = h.ok(&p.b, "git.sync-status", json!({"refresh": true}));
    assert_eq!(refreshed["state"], "diverged");
    let ff = h.ok(&p.b, "git.get-updates", json!({"strategy": "ff-only"}));
    assert_eq!(ff["result"], "diverged");
    assert!(
        !p.b.dir.join("only-a.txt").exists(),
        "ff-only changed nothing"
    );

    let out = h.ok(&p.b, "git.get-updates", json!({"strategy": "merge"}));
    assert_eq!(out["result"], "merged");
    assert_eq!(p.b.read("only-a.txt"), "a\n");
    assert_eq!(p.b.read("only-b.txt"), "b\n");
    // B can now share the combined history.
    h.ok(&p.b, "git.share", json!({}));
    assert_eq!(
        git(&p.remote, &["rev-parse", "main"]),
        git(&p.b.dir, &["rev-parse", "HEAD"])
    );
}

#[test]
fn unsaved_work_is_saved_first_and_survives_an_update() {
    let h = harness();
    let p = pair(&h);
    p.a.write("only-a.txt", "a\n");
    p.a.commit_all("a adds");
    h.ok(&p.a, "git.share", json!({}));
    p.b.write("draft.txt", "unsaved\n");
    let out = h.ok(&p.b, "git.get-updates", json!({}));
    assert_eq!(out["result"], "merged");
    assert!(out["safety_version"].is_string());
    assert_eq!(p.b.read("draft.txt"), "unsaved\n");
    assert_eq!(p.b.read("only-a.txt"), "a\n");
    assert_eq!(status(&h, &p.b)["uncommitted"], 0);
}

#[test]
fn conflict_stops_before_touching_the_work_tree() {
    let h = harness();
    let p = pair(&h);
    p.a.write("a.txt", "one from a\n");
    p.a.write("other.txt", "new in a\n");
    p.a.commit_all("a edits");
    h.ok(&p.a, "git.share", json!({}));
    p.b.write("a.txt", "one from b\n");
    p.b.commit_all("b edits");
    let head = git(&p.b.dir, &["rev-parse", "HEAD"]);

    let out = h.ok(&p.b, "git.get-updates", json!({}));
    assert_eq!(out["result"], "conflicts");
    assert_eq!(out["files"], json!(["a.txt"]));
    assert_eq!(p.b.read("a.txt"), "one from b\n");
    assert!(!p.b.dir.join("other.txt").exists());
    assert_eq!(git(&p.b.dir, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&p.b.dir, &["status", "--porcelain"]), "");
    assert!(
        !p.b.dir.join(".git/MERGE_HEAD").exists(),
        "no merge in progress"
    );
}

#[test]
fn rejected_share_is_readable() {
    let h = harness();
    let p = pair(&h);
    p.a.write("a.txt", "from a\n");
    p.a.commit_all("a edits");
    h.ok(&p.a, "git.share", json!({}));
    p.b.write("b.txt", "from b\n");
    p.b.commit_all("b edits");
    let err = h
        .call(&p.b, "git.share", json!({}))
        .expect_err("push must be rejected");
    assert_eq!(err["code"], "share-rejected");
    let msg = err["message"].as_str().expect("message");
    assert!(msg.contains("get updates"), "{msg}");
    assert!(!msg.to_lowercase().contains("push"), "{msg}");
}

#[test]
fn unreachable_remote_is_readable() {
    let h = harness();
    let r = Repo::with_commit();
    let missing = r.tmp.path().join("nowhere.git");
    h.ok(
        &r,
        "git.remote-set",
        json!({"url": missing.to_string_lossy()}),
    );
    assert_eq!(h.code(&r, "git.share", json!({})), "remote-unreachable");
    assert_eq!(
        h.code(&r, "git.get-updates", json!({})),
        "remote-unreachable"
    );
}

#[test]
fn share_needs_a_version_first() {
    let h = harness();
    let r = Repo::unborn();
    h.ok(
        &r,
        "git.remote-set",
        json!({"url": "https://example.invalid/x.git"}),
    );
    assert_eq!(h.code(&r, "git.share", json!({})), "nothing-to-share");
}
