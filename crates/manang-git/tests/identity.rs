//! Missing author identity is a clear, actionable error (own process so the
//! empty global config is guaranteed).

mod common;

use common::{Repo, git, harness};
use serde_json::json;

#[test]
fn commit_without_identity_explains_how_to_fix_it() {
    let h = harness();
    let r = Repo::plain();
    git(&r.dir, &["init", "-q", "-b", "main"]);
    r.write("a.txt", "x\n");
    h.ok(&r, "git.stage", json!({"paths": ["a.txt"]}));
    let err = h
        .call(&r, "git.commit", json!({"message": "m"}))
        .expect_err("no identity");
    assert_eq!(err["code"], "missing-identity");
    let msg = err["message"].as_str().unwrap_or_default();
    assert!(msg.contains("git config --global user.name"), "{msg}");
    assert!(msg.contains("user.email"), "{msg}");

    // With a local identity the same commit works.
    r.identity();
    h.ok(&r, "git.commit", json!({"message": "m"}));
}
