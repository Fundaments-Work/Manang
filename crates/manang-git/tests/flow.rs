//! End-to-end command behaviour on real temp repositories.

mod common;

use std::time::Duration;

use common::{Repo, git, harness};
use manang_kernel::Payload;
use serde_json::json;

#[test]
fn non_repo_is_a_structured_error_for_every_command() {
    let h = harness();
    let r = Repo::plain();
    for (cmd, args) in [
        ("git.status", json!({})),
        ("git.diff", json!({})),
        ("git.log", json!({})),
        ("git.branches", json!({})),
        ("git.stage", json!({"paths": ["a"]})),
        ("git.commit", json!({"message": "x"})),
    ] {
        assert_eq!(h.code(&r, cmd, args), "not-a-repo", "{cmd}");
    }
}

#[test]
fn init_then_status_on_an_empty_unborn_repo() {
    let h = harness();
    let r = Repo::plain();
    let sub = h.subscribe();
    let st = h.ok(&r, "git.init", json!({}));
    assert_eq!(st["unborn"], true);
    assert_eq!(st["head"], json!(null));
    assert!(st["files"].as_array().is_some_and(Vec::is_empty));
    assert_eq!(h.code(&r, "git.init", json!({})), "already-a-repo");
    assert_eq!(sub.drain().len(), 1, "git.changed after init");
}

#[test]
fn unborn_branch_stage_commit_roundtrip() {
    let h = harness();
    let r = Repo::unborn();
    r.write("a.txt", "hello\n");
    let st = h.ok(&r, "git.status", json!({}));
    assert_eq!(st["branch"], "main");
    assert_eq!(st["files"][0]["untracked"], true);

    let sub = h.subscribe();
    h.ok(&r, "git.stage", json!({"paths": ["a.txt"]}));
    let st = h.ok(&r, "git.status", json!({}));
    assert_eq!(st["files"][0]["staged"], "added");

    h.ok(&r, "git.unstage", json!({"paths": ["a.txt"]}));
    let st = h.ok(&r, "git.status", json!({}));
    assert_eq!(st["files"][0]["untracked"], true);

    h.ok(&r, "git.stage", json!({"paths": ["."]}));
    let c = h.ok(&r, "git.commit", json!({"message": "initial\n\nbody"}));
    assert_eq!(c["subject"], "initial");
    assert_eq!(c["hash"].as_str().map(str::len), Some(40));
    assert_eq!(sub.drain().len(), 4, "one git.changed per write");

    let st = h.ok(&r, "git.status", json!({}));
    assert_eq!(st["unborn"], false);
    assert!(st["files"].as_array().is_some_and(Vec::is_empty));
    let log = h.ok(&r, "git.log", json!({}));
    assert_eq!(log["commits"][0]["subject"], "initial");
    assert_eq!(log["commits"][0]["author_name"], "Test User");
    assert_eq!(log["commits"][0]["short"].as_str().map(str::len), Some(7));
}

#[test]
fn commit_validations() {
    let h = harness();
    let r = Repo::with_commit();
    assert_eq!(
        h.code(&r, "git.commit", json!({"message": "  "})),
        "empty-message"
    );
    assert_eq!(
        h.code(&r, "git.commit", json!({"message": "x"})),
        "nothing-to-commit"
    );
    r.write("a.txt", "two\n");
    h.ok(&r, "git.stage", json!({"paths": ["a.txt"]}));
    h.ok(&r, "git.commit", json!({"message": "second"}));
    // Amend rewrites the unpushed last commit.
    r.write("a.txt", "three\n");
    h.ok(&r, "git.stage", json!({"paths": ["a.txt"]}));
    let c = h.ok(
        &r,
        "git.commit",
        json!({"message": "second, fixed", "amend": true}),
    );
    assert_eq!(c["amended"], true);
    let log = h.ok(&r, "git.log", json!({}));
    assert_eq!(log["commits"].as_array().map(Vec::len), Some(2));
    assert_eq!(log["commits"][0]["subject"], "second, fixed");
}

#[test]
fn amend_is_refused_once_the_commit_is_on_a_remote() {
    let h = harness();
    let r = Repo::with_commit();
    git(&r.dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    assert_eq!(
        h.code(&r, "git.commit", json!({"message": "x", "amend": true})),
        "amend-published"
    );
    let u = Repo::unborn();
    assert_eq!(
        h.code(&u, "git.commit", json!({"message": "x", "amend": true})),
        "amend-unborn"
    );
}

#[test]
fn discard_requires_confirmation_and_keeps_a_recoverable_copy() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("a.txt", "precious edit\n");
    r.write("new.txt", "untracked work\n");
    assert_eq!(
        h.code(&r, "git.discard", json!({"paths": ["."]})),
        "confirm-required"
    );
    assert_eq!(r.read("a.txt"), "precious edit\n");

    let out = h.ok(&r, "git.discard", json!({"paths": ["."], "confirm": true}));
    assert_eq!(out["discarded"], 2);
    assert_eq!(r.read("a.txt"), "one\n");
    assert!(!r.dir.join("new.txt").exists());
    let trash = std::path::PathBuf::from(out["trash"].as_str().expect("trash dir"));
    assert_eq!(
        std::fs::read_to_string(trash.join("a.txt")).expect("saved"),
        "precious edit\n"
    );
    assert_eq!(
        std::fs::read_to_string(trash.join("new.txt")).expect("saved"),
        "untracked work\n"
    );
    // The trash lives in .git, so it is not itself reported as a change.
    let st = h.ok(&r, "git.status", json!({}));
    assert!(st["files"].as_array().is_some_and(Vec::is_empty));
}

#[test]
fn discard_restores_a_deleted_file() {
    let h = harness();
    let r = Repo::with_commit();
    std::fs::remove_file(r.dir.join("a.txt")).expect("rm");
    let out = h.ok(
        &r,
        "git.discard",
        json!({"paths": ["a.txt"], "confirm": true}),
    );
    assert_eq!(out["trash"], json!(null));
    assert_eq!(r.read("a.txt"), "one\n");
}

#[test]
fn checkout_branch_create_switch_and_dirty_refusal() {
    let h = harness();
    let r = Repo::with_commit();
    h.ok(
        &r,
        "git.checkout-branch",
        json!({"name": "feature", "create": true}),
    );
    assert_eq!(h.ok(&r, "git.status", json!({}))["branch"], "feature");
    r.write("a.txt", "feature version\n");
    r.commit_all("feature change");
    h.ok(&r, "git.checkout-branch", json!({"name": "main"}));
    assert_eq!(r.read("a.txt"), "one\n");

    // A dirty file that the target branch would overwrite blocks the switch.
    r.write("a.txt", "local edit\n");
    assert_eq!(
        h.code(&r, "git.checkout-branch", json!({"name": "feature"})),
        "dirty-tree"
    );
    assert_eq!(r.read("a.txt"), "local edit\n");
    assert_eq!(h.ok(&r, "git.status", json!({}))["branch"], "main");

    assert_eq!(
        h.code(
            &r,
            "git.checkout-branch",
            json!({"name": "bad name", "create": true})
        ),
        "bad-branch-name"
    );
    assert_eq!(
        h.code(&r, "git.checkout-branch", json!({"name": "--detach"})),
        "bad-branch-name"
    );
    assert_eq!(
        h.code(&r, "git.checkout-branch", json!({"name": "nope"})),
        "command-failed"
    );

    let names: Vec<String> = h
        .ok(&r, "git.branches", json!({}))
        .as_array()
        .expect("array")
        .iter()
        .map(|b| {
            format!(
                "{}{}",
                b["name"].as_str().unwrap_or(""),
                if b["current"] == true { "*" } else { "" }
            )
        })
        .collect();
    assert_eq!(names, ["feature", "main*"]);
}

#[test]
fn checkout_on_an_unborn_branch_can_create() {
    let h = harness();
    let r = Repo::unborn();
    h.ok(
        &r,
        "git.checkout-branch",
        json!({"name": "dev", "create": true}),
    );
    let st = h.ok(&r, "git.status", json!({}));
    assert_eq!(st["branch"], "dev");
    assert_eq!(st["unborn"], true);
    let b = h.ok(&r, "git.branches", json!({}));
    assert_eq!(b[0]["name"], "dev");
}

#[test]
fn merge_conflict_is_reported_per_file() {
    let h = harness();
    let r = Repo::with_commit();
    git(&r.dir, &["switch", "-q", "-c", "other"]);
    r.write("a.txt", "other\n");
    r.commit_all("other side");
    git(&r.dir, &["switch", "-q", "main"]);
    r.write("a.txt", "main side\n");
    r.commit_all("main side");
    let merge = std::process::Command::new("git")
        .args(["merge", "other"])
        .current_dir(&r.dir)
        .output()
        .expect("merge");
    assert!(!merge.status.success());

    let st = h.ok(&r, "git.status", json!({}));
    let f = &st["files"][0];
    assert_eq!(f["path"], "a.txt");
    assert_eq!(f["conflicted"], true);
    // Diff of a conflicted file does not crash and compares against "ours".
    h.ok(&r, "git.diff", json!({"path": "a.txt"}));
    // Committing during an unresolved merge is refused by git, not swallowed.
    assert_eq!(
        h.code(&r, "git.commit", json!({"message": "m"})),
        "command-failed"
    );
}

#[test]
fn detached_head_status_and_log() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("a.txt", "two\n");
    r.commit_all("second");
    git(&r.dir, &["checkout", "-q", "--detach", "HEAD~1"]);
    let st = h.ok(&r, "git.status", json!({}));
    assert_eq!(st["detached"], true);
    assert_eq!(st["branch"], json!(null));
    assert_eq!(st["upstream"], json!(null));
    let log = h.ok(&r, "git.log", json!({}));
    assert_eq!(log["commits"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        h.code(&r, "git.sync", json!({"action": "push"})),
        "bad-arguments"
    );
}

#[test]
fn upstream_ahead_behind() {
    let h = harness();
    let r = Repo::with_commit();
    let remote = tempfile::tempdir().expect("remote");
    git(remote.path(), &["init", "-q", "--bare", "-b", "main"]);
    let url = remote.path().to_string_lossy().into_owned();
    git(&r.dir, &["remote", "add", "origin", &url]);

    // No network is touched until the user asks for it.
    let st = h.ok(&r, "git.status", json!({}));
    assert_eq!(st["upstream"], json!(null));

    let pushed = h.ok(&r, "git.sync", json!({"action": "push"}));
    assert_eq!(pushed["remote"], "origin");
    let st = h.ok(&r, "git.status", json!({}));
    assert_eq!(st["upstream"]["name"], "origin/main");
    assert_eq!(
        (
            st["upstream"]["ahead"].as_u64(),
            st["upstream"]["behind"].as_u64()
        ),
        (Some(0), Some(0))
    );

    r.write("a.txt", "two\n");
    r.commit_all("local only");
    let st = h.ok(&r, "git.status", json!({}));
    assert_eq!(st["upstream"]["ahead"], 1);

    // A second clone pushes; fetch shows us behind, pull fast-forwards.
    let other = tempfile::tempdir().expect("other");
    git(other.path(), &["clone", "-q", &url, "c"]);
    let c = other.path().join("c");
    git(&c, &["config", "user.name", "O"]);
    git(&c, &["config", "user.email", "o@example.com"]);
    std::fs::write(c.join("b.txt"), "b\n").expect("write");
    git(&c, &["add", "."]);
    git(&c, &["commit", "-q", "-m", "remote work"]);
    // The local-only commit must be pushed first so the clone's push is a
    // fast-forward; do it from our side, then pull into the clone.
    h.ok(&r, "git.sync", json!({"action": "push"}));
    git(
        &c,
        &["pull", "-q", "--no-rebase", "--no-edit", "origin", "main"],
    );
    git(&c, &["push", "-q", "origin", "main"]);

    h.ok(&r, "git.sync", json!({"action": "fetch"}));
    let st = h.ok(&r, "git.status", json!({}));
    assert_eq!(
        (
            st["upstream"]["ahead"].as_u64(),
            st["upstream"]["behind"].as_u64()
        ),
        (Some(0), Some(2))
    );
    h.ok(&r, "git.sync", json!({"action": "pull"}));
    assert_eq!(r.read("b.txt"), "b\n");
}

#[test]
fn sync_refuses_unknown_remotes_urls_and_bad_actions() {
    let h = harness();
    let r = Repo::with_commit();
    for remote in [
        "origin",
        "--upload-pack=evil",
        "ext::sh -c evil",
        "https://example.com/x.git",
    ] {
        assert_eq!(
            h.code(&r, "git.sync", json!({"action": "fetch", "remote": remote})),
            "bad-arguments",
            "{remote}"
        );
    }
    assert_eq!(
        h.code(&r, "git.sync", json!({"action": "force-push"})),
        "bad-arguments"
    );
    assert_eq!(
        h.code(&r, "git.sync", json!({"action": "fetch"})),
        "bad-arguments",
        "no remote"
    );
}

#[test]
fn hooks_are_off_unless_opted_in() {
    let h = harness();
    let r = Repo::with_commit();
    let hook = r.dir.join(".git/hooks/pre-commit");
    std::fs::write(&hook, "#!/bin/sh\ntouch hook-ran\nexit 1\n").expect("hook");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    r.write("a.txt", "two\n");
    h.ok(&r, "git.stage", json!({"paths": ["a.txt"]}));
    h.ok(&r, "git.commit", json!({"message": "no hooks"}));
    assert!(!r.dir.join("hook-ran").exists());

    r.write("a.txt", "three\n");
    h.ok(&r, "git.stage", json!({"paths": ["a.txt"]}));
    assert_eq!(
        h.code(
            &r,
            "git.commit",
            json!({"message": "with hooks", "hooks": true})
        ),
        "command-failed"
    );
    assert!(r.dir.join("hook-ran").exists());
}

#[test]
fn unicode_paths_and_crlf_and_binary() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("図/日本語 ファイル.txt", "こんにちは\r\nworld\r\n");
    r.write_bytes("img.bin", &[0, 159, 146, 150, 0, 1, 2]);
    h.ok(
        &r,
        "git.stage",
        json!({"paths": ["図/日本語 ファイル.txt", "img.bin"]}),
    );
    let st = h.ok(&r, "git.status", json!({}));
    let paths: Vec<&str> = st["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|f| f["path"].as_str())
        .collect();
    assert_eq!(paths, ["img.bin", "図/日本語 ファイル.txt"]);
    h.ok(&r, "git.commit", json!({"message": "unicode + binary"}));

    // CRLF content round-trips through `show` byte for byte.
    let shown = h.ok(
        &r,
        "git.show",
        json!({"rev": "HEAD", "path": "図/日本語 ファイル.txt"}),
    );
    assert_eq!(shown["text"], "こんにちは\r\nworld\r\n");
    // Non-UTF-8 content comes back as a binary payload.
    let bin = h
        .kernel
        .invoke(
            "git.show",
            Payload::Json(
                json!({"root": r.dir.to_string_lossy(), "rev": "HEAD", "path": "img.bin"}),
            ),
        )
        .expect("show");
    assert!(matches!(&bin, Payload::Binary(b) if b.as_ref() == [0, 159, 146, 150, 0, 1, 2]));

    r.write("図/日本語 ファイル.txt", "こんにちは\r\nWORLD\r\n");
    r.write_bytes("img.bin", &[0, 1, 2, 3]);
    let d = h.ok(&r, "git.diff", json!({}));
    let files = d["files"].as_array().expect("files");
    let bin = files.iter().find(|f| f["path"] == "img.bin").expect("bin");
    assert_eq!(bin["binary"], true);
    assert_eq!(bin["hunks"].as_array().map(Vec::len), Some(0));
    let text = files.iter().find(|f| f["path"] != "img.bin").expect("text");
    assert_eq!(
        (text["additions"].as_u64(), text["deletions"].as_u64()),
        (Some(1), Some(1))
    );
}

#[test]
fn show_errors() {
    let h = harness();
    let r = Repo::with_commit();
    assert_eq!(
        h.code(&r, "git.show", json!({"rev": "nope", "path": "a.txt"})),
        "rev-not-found"
    );
    assert_eq!(
        h.code(&r, "git.show", json!({"rev": "HEAD", "path": "missing"})),
        "path-not-in-rev"
    );
    assert_eq!(
        h.code(&r, "git.show", json!({"rev": "--help", "path": "a.txt"})),
        "bad-arguments"
    );
    assert_eq!(
        h.ok(&r, "git.show", json!({"rev": "HEAD", "path": "a.txt"}))["text"],
        "one\n"
    );
}

#[test]
fn blame_and_path_log() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("a.txt", "one\ntwo\nthree\n");
    r.commit_all("add lines");
    r.write("b.txt", "other\n");
    r.commit_all("unrelated");
    let b = h.ok(&r, "git.blame", json!({"path": "a.txt"}));
    assert_eq!(b["total_lines"], 3);
    assert_eq!(b["ranges"][0]["commit"]["subject"], "first");
    assert_eq!(b["ranges"][0]["lines"], 1);
    assert_eq!(b["ranges"][1]["commit"]["subject"], "add lines");
    assert_eq!(b["ranges"][1]["start"], 2);

    let log = h.ok(&r, "git.log", json!({"path": "a.txt"}));
    let subjects: Vec<&str> = log["commits"]
        .as_array()
        .expect("c")
        .iter()
        .filter_map(|c| c["subject"].as_str())
        .collect();
    assert_eq!(subjects, ["add lines", "first"]);
    let page = h.ok(&r, "git.log", json!({"limit": 1, "offset": 1}));
    assert_eq!(page["commits"][0]["subject"], "add lines");
    assert_eq!(page["has_more"], true);
}

#[test]
fn project_in_a_subdirectory_of_the_repo() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("papers/p1/main.typ", "= Hi\n");
    r.write("outside.txt", "x\n");
    r.commit_all("add paper");
    r.write("papers/p1/main.typ", "= Hello\n");
    r.write("outside.txt", "y\n");
    let sub = Repo {
        tmp: tempfile::tempdir().expect("t"),
        dir: r.dir.join("papers/p1"),
    };
    let st = h.ok(&sub, "git.status", json!({}));
    let files = st["files"].as_array().expect("files");
    assert_eq!(files.len(), 1, "only project files: {st}");
    assert_eq!(files[0]["path"], "main.typ");
    let d = h.ok(&sub, "git.diff", json!({}));
    assert_eq!(d["files"][0]["hunks"][0]["lines"][0]["text"], "= Hi");
    assert_eq!(
        h.ok(&sub, "git.show", json!({"rev": "HEAD", "path": "main.typ"}))["text"],
        "= Hi\n"
    );
    h.ok(&sub, "git.stage", json!({"paths": ["main.typ"]}));
    let staged = h.ok(&sub, "git.diff", json!({"staged": true}));
    assert_eq!(staged["files"][0]["path"], "main.typ");
    assert_eq!(
        h.ok(&r, "git.status", json!({}))["files"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
}

trait Sub {
    fn subscribe(&self) -> manang_kernel::Subscription;
}

impl Sub for common::Harness {
    fn subscribe(&self) -> manang_kernel::Subscription {
        self.bus.subscribe("git.changed")
    }
}

#[test]
fn events_carry_the_root_and_reads_emit_nothing() {
    let h = harness();
    let r = Repo::with_commit();
    let sub = h.subscribe();
    h.ok(&r, "git.status", json!({}));
    h.ok(&r, "git.log", json!({}));
    assert!(sub.try_recv().is_none());
    r.write("x.txt", "x\n");
    h.ok(&r, "git.stage", json!({"paths": ["x.txt"]}));
    let ev = sub.recv_timeout(Duration::from_secs(2)).expect("event");
    assert_eq!(ev.topic, "git.changed");
    let Payload::Json(v) = ev.payload else {
        panic!("json")
    };
    assert_eq!(v["root"], r.dir.to_string_lossy().as_ref());
}
