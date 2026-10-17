//! Path confinement: nothing a caller can type reaches outside the root.

mod common;

use common::{Repo, harness};
use proptest::prelude::*;
use serde_json::json;

#[test]
fn escapes_are_refused_by_name() {
    let h = harness();
    let r = Repo::with_commit();
    for bad in [
        "../x",
        "a/../../x",
        "/etc/passwd",
        "a\\b",
        ".git/config",
        ".git",
        "a\0b",
    ] {
        for cmd in ["git.stage", "git.unstage"] {
            let code = h.code(&r, cmd, json!({"paths": [bad]}));
            assert_eq!(code, "bad-path", "{cmd} {bad:?}");
        }
        assert_eq!(
            h.code(&r, "git.discard", json!({"paths": [bad], "confirm": true})),
            "bad-path",
            "{bad:?}"
        );
    }
    for cmd in ["git.diff", "git.blame", "git.diff-text"] {
        assert_eq!(
            h.code(&r, cmd, json!({"path": "../x"})),
            "bad-path",
            "{cmd}"
        );
    }
    assert_eq!(
        h.code(
            &r,
            "git.show",
            json!({"rev": "HEAD", "path": "/etc/passwd"})
        ),
        "bad-path"
    );
}

#[cfg(unix)]
#[test]
fn symlinks_pointing_outside_are_refused() {
    let h = harness();
    let r = Repo::with_commit();
    let outside = tempfile::tempdir().expect("outside");
    std::fs::write(outside.path().join("secret.txt"), "s\n").expect("write");
    std::os::unix::fs::symlink(outside.path(), r.dir.join("link")).expect("symlink");
    std::os::unix::fs::symlink(outside.path().join("secret.txt"), r.dir.join("file-link"))
        .expect("symlink");
    for p in ["link/secret.txt", "link", "file-link"] {
        assert_eq!(
            h.code(&r, "git.stage", json!({"paths": [p]})),
            "bad-path",
            "{p}"
        );
        assert_eq!(
            h.code(&r, "git.discard", json!({"paths": [p], "confirm": true})),
            "bad-path",
            "{p}"
        );
    }
    assert_eq!(
        h.code(&r, "git.diff", json!({"path": "link/secret.txt"})),
        "bad-path"
    );
    assert!(outside.path().join("secret.txt").exists());
}

#[test]
fn pathspec_magic_is_literal() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("keep.txt", "k\n");
    r.write(":(glob)*.txt", "weird\n");
    // `*` and `:(...)` must not be interpreted as pathspec magic.
    h.ok(&r, "git.stage", json!({"paths": [":(glob)*.txt"]}));
    let st = h.ok(&r, "git.status", json!({}));
    let staged: Vec<&str> = st["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter(|f| f["staged"].is_string())
        .filter_map(|f| f["path"].as_str())
        .collect();
    assert_eq!(staged, [":(glob)*.txt"]);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Arbitrary strings never panic and never get past the lexical gate
    /// unless they name something inside the root.
    #[test]
    fn arbitrary_paths_never_escape_or_panic(raw in "\\PC{0,40}") {
        let h = harness();
        let r = common::shared_repo();
        let outcome = h.call(r, "git.stage", json!({"paths": [raw.clone()]}));
        if let Ok(v) = &outcome {
            prop_assert!(v["staged"].is_u64());
            // Whatever was accepted resolved inside the root.
            let lexical = manang_project::lexical_relative(&raw);
            prop_assert!(lexical.is_ok(), "accepted {raw:?}");
        }
        let _ = h.call(r, "git.diff", json!({"path": raw.clone()}));
        let _ = h.call(r, "git.show", json!({"rev": "HEAD", "path": raw}));
    }

    #[test]
    fn dotdot_heavy_paths_are_always_refused(parts in proptest::collection::vec(prop_oneof![Just(".."), Just("."), Just("a"), Just("")], 1..8)) {
        let raw = parts.join("/");
        let h = harness();
        let r = common::shared_repo();
        if parts.contains(&"..") {
            let code = h.call(r, "git.stage", json!({"paths": [raw]}));
            prop_assert!(code.is_err());
        }
    }
}
