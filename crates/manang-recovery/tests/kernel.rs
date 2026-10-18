//! Every `recovery.*` command driven through the real kernel on temp dirs.

use std::fs;
use std::sync::Arc;

use manang_kernel::{Kernel, Payload};
use serde_json::{Value, json};

struct Harness {
    _tmp: tempfile::TempDir,
    kernel: Kernel,
    project: String,
}

fn harness() -> Harness {
    let tmp = tempfile::tempdir().expect("tmp");
    let project = tmp.path().join("proj");
    fs::create_dir_all(&project).expect("project");
    let kernel = Kernel::new();
    kernel
        .register(
            manang_recovery::manifest().expect("manifest"),
            Arc::new(manang_recovery::plugin(tmp.path().join("recovery"))),
        )
        .expect("register");
    Harness {
        project: project.to_string_lossy().into_owned(),
        _tmp: tmp,
        kernel,
    }
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
}

#[test]
fn write_list_read_discard_through_the_kernel() {
    let h = harness();
    let out = h.ok(
        "recovery.write",
        json!({ "project": h.project, "path": "main.typ", "kind": "text",
                "content": "= Draft\nunsaved", "saved_hash": null,
                "view": { "anchor": 4, "head": 4 } }),
    );
    assert_eq!(out["stored"], true);

    let list = h.ok("recovery.list", json!({ "project": h.project }));
    assert_eq!(list["entries"][0]["path"], "main.typ");
    assert_eq!(list["entries"][0]["disk"], "missing");
    assert!(
        list["entries"][0].get("text").is_none(),
        "listing carries no text"
    );

    let read = h.ok(
        "recovery.read",
        json!({ "project": h.project, "path": "main.typ" }),
    );
    assert_eq!(read["text"], "= Draft\nunsaved");
    assert_eq!(read["view"]["anchor"], 4);

    let gone = h.ok(
        "recovery.discard",
        json!({ "project": h.project, "path": "main.typ" }),
    );
    assert_eq!(gone["removed"], 1);
    assert_eq!(
        h.ok(
            "recovery.read",
            json!({ "project": h.project, "path": "main.typ" })
        ),
        Value::Null
    );
}

#[test]
fn bad_arguments_and_escaping_paths_are_errors_not_panics() {
    let h = harness();
    assert!(h.call("recovery.write", json!({ "nope": 1 })).is_err());
    assert!(
        h.call(
            "recovery.write",
            json!({ "project": h.project, "path": "../x", "kind": "text", "content": "" })
        )
        .is_err()
    );
    assert!(h.call("recovery.list", json!({})).is_err());
}

#[test]
fn a_snapshot_of_a_figure_keeps_its_scene_json() {
    let h = harness();
    let scene = r#"{"version":2,"width":960,"height":540,"nodes":[]}"#;
    h.ok(
        "recovery.write",
        json!({ "project": h.project, "path": "a.figure.json", "kind": "figure",
                "content": scene, "saved_hash": "b3:00" }),
    );
    let list = h.ok("recovery.list", json!({ "project": h.project }));
    assert_eq!(list["entries"][0]["kind"], "figure");
    let read = h.ok(
        "recovery.read",
        json!({ "project": h.project, "path": "a.figure.json" }),
    );
    assert_eq!(read["text"], scene);
}
