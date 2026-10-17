//! Snapshot tests for the structured and unified diff output.

mod common;

use common::{Repo, harness};
use serde_json::json;

fn repo_with_edits() -> Repo {
    let r = Repo::with_commit();
    r.write(
        "doc.typ",
        "= Title\n\nFirst paragraph.\nSecond line.\nThird line.\n\nEnd.\n",
    );
    r.commit_all("add doc");
    r.write(
        "doc.typ",
        "= Title\n\nFirst paragraph, edited.\nSecond line.\nThird line.\nInserted ünïcode 😀.\n\nEnd.\n",
    );
    r.write("fresh.txt", "brand new\n");
    r
}

#[test]
fn unstaged_structured_diff() {
    let h = harness();
    let r = repo_with_edits();
    let v = h.ok(&r, "git.diff", json!({}));
    insta::assert_json_snapshot!("unstaged_structured", v);
}

#[test]
fn staged_structured_diff() {
    let h = harness();
    let r = repo_with_edits();
    h.ok(&r, "git.stage", json!({"paths": ["doc.typ"]}));
    let v = h.ok(&r, "git.diff", json!({"staged": true}));
    insta::assert_json_snapshot!("staged_structured", v);
    // The same file has nothing left unstaged.
    let un = h.ok(&r, "git.diff", json!({"path": "doc.typ"}));
    assert_eq!(un["files"].as_array().map(Vec::len), Some(0));
}

#[test]
fn unified_text() {
    let h = harness();
    let r = repo_with_edits();
    let v = h.ok(&r, "git.diff-text", json!({"path": "."}));
    insta::assert_snapshot!("unified", v["text"].as_str().unwrap_or_default());
}

#[test]
fn deleted_and_crlf_files() {
    let h = harness();
    let r = Repo::with_commit();
    r.write("crlf.txt", "a\r\nb\r\nc\r\n");
    r.commit_all("crlf");
    r.write("crlf.txt", "a\r\nB\r\nc\r\n");
    std::fs::remove_file(r.dir.join("a.txt")).expect("rm");
    let v = h.ok(&r, "git.diff", json!({}));
    insta::assert_json_snapshot!("deleted_and_crlf", v);
}
