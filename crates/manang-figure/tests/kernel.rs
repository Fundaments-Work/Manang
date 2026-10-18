//! The figure plugin driven through the kernel, as the shell drives it.

use std::fs;
use std::sync::Arc;

use manang_figure::{manifest, plugin};
use manang_kernel::{Kernel, Payload};
use serde_json::{Value, json};

fn kernel() -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(manifest().expect("manifest is valid"), Arc::new(plugin()))
        .expect("registers");
    kernel
}

fn run(k: &Kernel, command: &str, args: Value) -> Value {
    match k.invoke(command, Payload::Json(args)) {
        Ok(Payload::Json(v)) => v,
        other => panic!("{command}: expected JSON, got {other:?}"),
    }
}

fn binary(k: &Kernel, command: &str, args: Value) -> String {
    match k.invoke(command, Payload::Json(args)) {
        Ok(Payload::Binary(bytes)) => String::from_utf8(bytes.to_vec()).expect("utf-8"),
        other => panic!("{command}: expected bytes, got {other:?}"),
    }
}

fn rect(id: &str, x: f64, y: f64) -> Value {
    json!({ "kind": "rect", "id": id, "x": x, "y": y, "width": 100, "height": 60 })
}

#[test]
fn manifest_is_native_and_lists_the_commands() {
    let m = manifest().expect("valid manifest");
    assert_eq!(m.plugin.tier, manang_kernel::Tier::Native);
    let ids: Vec<String> = kernel().commands().into_iter().map(|c| c.id).collect();
    for id in [
        "figure.create",
        "figure.open",
        "figure.save",
        "figure.list",
        "figure.apply",
        "figure.get",
        "figure.render-svg",
        "figure.export-svg",
        "figure.hit-test",
        "figure.close",
    ] {
        assert!(ids.contains(&id.to_owned()), "missing {id}");
    }
}

#[test]
fn edits_undo_redo_and_revisions() {
    let k = kernel();
    let id = run(&k, "figure.create", json!({ "width": 1000, "height": 800 }))["id"].clone();
    let added = run(
        &k,
        "figure.apply",
        json!({ "id": id, "op": { "op": "add", "nodes": [rect("n1", 50.0, 50.0)] } }),
    );
    assert_eq!(added["revision"], 1);
    assert_eq!(added["applied"], true);
    assert_eq!(added["changed"], json!(["n1"]));

    let styled = run(
        &k,
        "figure.apply",
        json!({ "id": id, "op": { "type": "set-style", "id": "n1", "stroke": "#ffffff" } }),
    );
    assert_eq!(styled["revision"], 2);
    let scene = run(&k, "figure.get", json!({}));
    assert_eq!(scene["nodes"][0]["style"]["stroke"], "#ffffff");

    run(&k, "figure.apply", json!({ "op": "undo" }));
    let scene = run(&k, "figure.get", json!({}));
    assert!(scene["nodes"][0].get("style").is_none());
    let redone = run(&k, "figure.apply", json!({ "op": { "op": "redo" } }));
    assert_eq!(redone["can_redo"], false);
    assert_eq!(
        run(&k, "figure.get", json!({}))["nodes"][0]["style"]["stroke"],
        "#ffffff"
    );
}

#[test]
fn failed_operations_change_nothing() {
    let k = kernel();
    run(&k, "figure.create", json!({}));
    run(
        &k,
        "figure.apply",
        json!({ "op": { "op": "add", "nodes": [rect("n1", 0.0, 0.0)] } }),
    );
    for bad in [
        json!({ "op": "add", "nodes": [rect("n1", 0.0, 0.0)] }),
        json!({ "op": "move", "ids": ["ghost"], "dx": 1, "dy": 1 }),
        json!({ "op": "update", "id": "n1", "set": { "style": { "stroke": "red" } } }),
        json!({ "op": "batch", "ops": [{ "op": "move", "ids": ["n1"], "dx": 5, "dy": 5 }, { "op": "remove", "ids": ["ghost"] }] }),
        json!({ "op": "not-an-op" }),
    ] {
        assert!(
            k.invoke("figure.apply", Payload::Json(json!({ "op": bad })))
                .is_err(),
            "{bad}"
        );
    }
    let list = run(&k, "figure.list", json!({}));
    assert_eq!(list[0]["revision"], 1);
    assert_eq!(run(&k, "figure.get", json!({}))["nodes"][0]["x"], 0.0);
}

#[test]
fn save_and_open_round_trip_and_v1_files_upgrade() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("lattice.figure.json");
    fs::write(
        &path,
        r##"{ "version": 1, "width": 960, "height": 540, "nodes": [
            { "kind": "rect", "id": "n1", "x": 10, "y": 10, "width": 50, "height": 20, "stroke": "#e8e6e1" },
            { "kind": "arrow", "id": "n2", "from": { "x": 0, "y": 0 }, "to": { "x": 9, "y": 9 }, "color": "#e8e6e1" }
        ] }"##,
    )
    .expect("writes");
    let k = kernel();
    let id = run(&k, "figure.open", json!({ "path": path.to_string_lossy() }))["id"].clone();
    run(
        &k,
        "figure.apply",
        json!({ "id": id, "op": { "op": "move", "ids": ["n1"], "dx": 5, "dy": 0 } }),
    );
    let saved = run(&k, "figure.save", json!({ "id": id }));
    assert_eq!(saved["revision"], 1);
    let text = fs::read_to_string(&path).expect("reads");
    assert!(text.starts_with("{\n  \"version\": 2,"), "{text}");
    assert!(
        text.contains("\"x\": 15,"),
        "whole numbers are written without a fraction"
    );
    assert!(text.ends_with("}\n"));
    let svg = fs::read_to_string(dir.path().join("lattice.svg")).expect("svg");
    assert!(svg.starts_with("<svg"));

    let reopened = run(&k, "figure.open", json!({ "path": path.to_string_lossy() }))["id"].clone();
    let scene = run(&k, "figure.get", json!({ "id": reopened }));
    assert_eq!(scene["nodes"][1]["kind"], "arrow");
    assert_eq!(scene["nodes"][1]["points"][1]["x"], 9.0);
}

#[test]
fn export_render_and_hit_test() {
    let k = kernel();
    run(&k, "figure.create", json!({ "width": 400, "height": 300 }));
    run(
        &k,
        "figure.apply",
        json!({ "op": { "op": "add", "nodes": [rect("n1", 50.0, 50.0)] } }),
    );
    let svg = binary(&k, "figure.export-svg", json!({}));
    assert!(svg.contains("<g id=\"n1\">"));
    let scene = run(&k, "figure.get", json!({}));
    assert_eq!(
        binary(&k, "figure.render-svg", json!({ "scene": scene })),
        svg
    );
    let hit = run(&k, "figure.hit-test", json!({ "x": 60, "y": 60 }));
    assert_eq!(hit["node_id"], "n1");
    let miss = run(&k, "figure.hit-test", json!({ "x": 5, "y": 5 }));
    assert_eq!(miss["hit"], false);
    // render-svg is stateless: it opened nothing.
    assert_eq!(
        run(&k, "figure.list", json!({})).as_array().map(Vec::len),
        Some(1)
    );
}

#[test]
fn invalid_arguments_are_errors_not_panics() {
    let k = kernel();
    for (command, args) in [
        ("figure.open", json!({})),
        ("figure.open", json!({ "path": 123 })),
        ("figure.apply", json!({})),
        ("figure.hit-test", json!({ "x": "bad" })),
        ("figure.render-svg", json!({ "scene": { "version": 2 } })),
        ("figure.close", json!({})),
    ] {
        assert!(
            k.invoke(command, Payload::Json(args.clone())).is_err(),
            "{command} {args}"
        );
    }
    assert!(
        k.invoke("figure.create", Payload::Binary(Arc::from(vec![1u8])))
            .is_err()
    );
}
