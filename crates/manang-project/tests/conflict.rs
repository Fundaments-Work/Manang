//! Conflict detection on save (`expected`), fingerprints and text diffs,
//! driven through the real kernel on temp dirs.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use manang_kernel::{EventBus, Kernel, Payload};
use serde_json::{Value, json};

struct Harness {
    _tmp: tempfile::TempDir,
    kernel: Kernel,
    dir: PathBuf,
}

fn harness() -> Harness {
    let tmp = tempfile::tempdir().expect("tmp");
    let config = tmp.path().join("config");
    let dir = tmp.path().join("paper");
    let kernel = Kernel::new();
    kernel
        .register(
            manang_project::manifest().expect("manifest"),
            Arc::new(manang_project::ProjectPlugin::new(&config, EventBus::new())),
        )
        .expect("register");
    let h = Harness {
        _tmp: tmp,
        kernel,
        dir,
    };
    h.ok(
        "project.create",
        json!({ "path": h.dir.to_string_lossy(), "name": "Paper", "engine": "typst" }),
    );
    h
}

impl Harness {
    fn call(&self, command: &str, args: Value) -> Result<Value, String> {
        match self.kernel.invoke(command, Payload::Json(args)) {
            Ok(Payload::Json(v)) => Ok(v),
            Ok(Payload::Empty) => Ok(Value::Null),
            Ok(Payload::Binary(_)) => Err("binary".to_owned()),
            Err(e) => Err(e.to_string()),
        }
    }

    fn ok(&self, command: &str, args: Value) -> Value {
        self.call(command, args)
            .unwrap_or_else(|e| panic!("{command} failed: {e}"))
    }

    fn file(&self, rel: &str) -> PathBuf {
        Path::new(&self.dir).join(rel)
    }
}

#[test]
fn write_file_with_an_expected_hash_detects_changes_made_outside() {
    let h = harness();
    let read = h.ok("project.read-text", json!({ "path": "main.typ" }));
    let hash = read["hash"].as_str().expect("hash").to_owned();
    assert!(hash.starts_with("b3:"));

    // Unchanged on disk: the write goes through and returns the new hash.
    let wrote = h.ok(
        "project.write-file",
        json!({ "path": "main.typ", "content": "= Mine\n", "expected": hash }),
    );
    let mine = wrote["hash"].as_str().expect("hash").to_owned();
    assert_ne!(mine, hash);
    assert_eq!(
        h.ok("project.read-text", json!({ "path": "main.typ" }))["hash"],
        mine
    );

    // Another program rewrites the file.
    fs::write(h.file("main.typ"), "= Theirs\n").expect("outside write");
    let err = h
        .call(
            "project.write-file",
            json!({ "path": "main.typ", "content": "= Mine again\n", "expected": mine }),
        )
        .expect_err("conflict");
    assert!(err.starts_with("[conflict]"), "{err}");
    assert_eq!(
        fs::read_to_string(h.file("main.typ")).expect("read"),
        "= Theirs\n",
        "a conflicting write changes nothing"
    );

    // Deleted outside: also a conflict, with its own wording.
    fs::remove_file(h.file("main.typ")).expect("rm");
    let err = h
        .call(
            "project.write-file",
            json!({ "path": "main.typ", "content": "x", "expected": mine }),
        )
        .expect_err("conflict");
    assert!(
        err.starts_with("[conflict]") && err.contains("deleted"),
        "{err}"
    );
}

#[test]
fn absent_only_writes_a_file_that_is_not_there_yet() {
    let h = harness();
    h.ok(
        "project.write-file",
        json!({ "path": "new.typ", "content": "a", "expected": "absent" }),
    );
    let err = h
        .call(
            "project.write-file",
            json!({ "path": "new.typ", "content": "b", "expected": "absent" }),
        )
        .expect_err("exists");
    assert!(err.starts_with("[conflict]"), "{err}");
    assert_eq!(fs::read_to_string(h.file("new.typ")).expect("read"), "a");
    // No `expected`: the unconditional write is unchanged.
    h.ok(
        "project.write-file",
        json!({ "path": "new.typ", "content": "c" }),
    );
    assert_eq!(fs::read_to_string(h.file("new.typ")).expect("read"), "c");
}

#[test]
fn the_hash_is_of_the_bytes_returned_and_stable() {
    let h = harness();
    h.ok(
        "project.write-file",
        json!({ "path": "a.typ", "content": "same\n" }),
    );
    let one = h.ok("project.read-text", json!({ "path": "a.typ" }));
    let two = h.ok("project.read-text", json!({ "path": "a.typ" }));
    assert_eq!(one["hash"], two["hash"]);
    assert_eq!(one["content"], "same\n");
    assert_eq!(one["size"], 5);
}

#[test]
fn diff_text_compares_the_disk_with_given_text() {
    let h = harness();
    h.ok(
        "project.write-file",
        json!({ "path": "a.typ", "content": "one\ntwo\n" }),
    );
    let d = h.ok(
        "project.diff-text",
        json!({ "path": "a.typ", "content": "one\nTWO\n" }),
    );
    assert_eq!(d["status"], "modified");
    assert_eq!(d["additions"], 1);
    assert_eq!(d["deletions"], 1);
    let missing = h.ok(
        "project.diff-text",
        json!({ "path": "nope.typ", "content": "x\n" }),
    );
    assert_eq!(missing["status"], "added");
}
