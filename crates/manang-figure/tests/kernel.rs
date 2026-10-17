//! Figure plugin kernel-driven integration tests.

use std::fs;
use std::sync::Arc;

use manang_figure::{RectNode, format_node_id, manifest, plugin};
use manang_kernel::{Kernel, Payload};
use serde_json::{Value, json};

fn test_kernel() -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(manifest().expect("manifest is valid"), Arc::new(plugin()))
        .expect("registers");
    kernel
}

fn json_of(payload: Payload) -> Value {
    match payload {
        Payload::Json(v) => v,
        other => panic!("expected JSON, got {other:?}"),
    }
}

#[test]
fn manifest_is_native_and_commands_registered() {
    let m = manifest().expect("valid manifest");
    assert_eq!(m.plugin.name, "figure");
    assert_eq!(m.plugin.tier, manang_kernel::Tier::Native);

    let k = test_kernel();
    let cmd_ids: Vec<_> = k.commands().into_iter().map(|c| c.id).collect();
    for id in [
        "figure.create",
        "figure.open",
        "figure.save",
        "figure.list",
        "figure.apply",
        "figure.get",
        "figure.export-svg",
        "figure.hit-test",
    ] {
        assert!(cmd_ids.contains(&id.to_owned()), "missing command {id}");
    }
}

#[test]
fn create_and_get_scene() {
    let k = test_kernel();
    let res = json_of(
        k.invoke(
            "figure.create",
            Payload::Json(json!({ "width": 1000.0, "height": 800.0 })),
        )
        .expect("create"),
    );
    let id = res["id"].as_u64().expect("id is u64");

    let scene_val = json_of(
        k.invoke("figure.get", Payload::Json(json!({ "id": id })))
            .expect("get"),
    );
    assert_eq!(scene_val["width"], 1000.0);
    assert_eq!(scene_val["height"], 800.0);
    assert_eq!(scene_val["nodes"], json!([]));
}

#[test]
fn apply_edits_undo_redo_and_revision_counter() {
    let k = test_kernel();
    let created = json_of(k.invoke("figure.create", Payload::Empty).expect("create"));
    let id = created["id"].as_u64().expect("id");

    let rect_node = RectNode {
        id: format_node_id(1),
        x: 50.0,
        y: 50.0,
        width: 100.0,
        height: 80.0,
        stroke: "#c6f135".to_string(),
        fill: Some("#141416".to_string()),
    };

    // 1. Add node
    let op_add = json!({
        "id": id,
        "op": {
            "op": "add-node",
            "node": {
                "kind": "rect",
                "id": rect_node.id.clone(),
                "x": rect_node.x,
                "y": rect_node.y,
                "width": rect_node.width,
                "height": rect_node.height,
                "stroke": rect_node.stroke.clone(),
                "fill": rect_node.fill.clone(),
            }
        }
    });
    let add_res = json_of(
        k.invoke("figure.apply", Payload::Json(op_add))
            .expect("add"),
    );
    assert_eq!(add_res["revision"], 1);
    assert_eq!(add_res["applied"], true);

    let scene = json_of(
        k.invoke("figure.get", Payload::Json(json!({ "id": id })))
            .expect("get"),
    );
    assert_eq!(scene["nodes"].as_array().expect("array").len(), 1);

    // 2. Set style
    let op_style = json!({
        "id": id,
        "op": {
            "type": "set-style",
            "id": rect_node.id.clone(),
            "stroke": "#ffffff",
        }
    });
    let style_res = json_of(
        k.invoke("figure.apply", Payload::Json(op_style))
            .expect("style"),
    );
    assert_eq!(style_res["revision"], 2);

    let scene_styled = json_of(k.invoke("figure.get", Payload::Empty).expect("get active"));
    assert_eq!(scene_styled["nodes"][0]["stroke"], "#ffffff");

    // 3. Undo style
    let op_undo = json!({
        "op": "undo"
    });
    let undo_res = json_of(
        k.invoke("figure.apply", Payload::Json(op_undo))
            .expect("undo"),
    );
    assert_eq!(undo_res["revision"], 3);
    assert_eq!(undo_res["applied"], true);

    let scene_after_undo = json_of(k.invoke("figure.get", Payload::Empty).expect("get"));
    assert_eq!(scene_after_undo["nodes"][0]["stroke"], "#c6f135");

    // 4. Redo style
    let op_redo = json!({
        "op": "redo"
    });
    let redo_res = json_of(
        k.invoke("figure.apply", Payload::Json(op_redo))
            .expect("redo"),
    );
    assert_eq!(redo_res["revision"], 4);
    assert_eq!(redo_res["applied"], true);

    let scene_after_redo = json_of(k.invoke("figure.get", Payload::Empty).expect("get"));
    assert_eq!(scene_after_redo["nodes"][0]["stroke"], "#ffffff");

    // 5. Remove node
    let op_remove = json!({
        "id": id,
        "op": {
            "op": "remove-node",
            "id": rect_node.id.clone()
        }
    });
    let rem_res = json_of(
        k.invoke("figure.apply", Payload::Json(op_remove))
            .expect("remove"),
    );
    assert_eq!(rem_res["revision"], 5);

    let scene_empty = json_of(k.invoke("figure.get", Payload::Empty).expect("get"));
    assert_eq!(scene_empty["nodes"].as_array().expect("array").len(), 0);
}

#[test]
fn open_and_save_figure_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let fig_path = dir.path().join("circuit.figure.json");
    let svg_path = dir.path().join("circuit.svg");

    let k = test_kernel();
    let created = json_of(k.invoke("figure.create", Payload::Empty).expect("create"));
    let id = created["id"].as_u64().expect("id");

    // Add a rect node
    let op_add = json!({
        "id": id,
        "op": {
            "op": "add-node",
            "node": {
                "kind": "rect",
                "id": "r1",
                "x": 10.0,
                "y": 20.0,
                "width": 100.0,
                "height": 50.0,
                "stroke": "#c6f135"
            }
        }
    });
    k.invoke("figure.apply", Payload::Json(op_add))
        .expect("apply");

    // Save with explicit path
    k.invoke(
        "figure.save",
        Payload::Json(json!({
            "id": id,
            "path": fig_path.to_str().expect("utf8")
        })),
    )
    .expect("save");

    assert!(fig_path.exists(), "circuit.figure.json was not created");
    assert!(svg_path.exists(), "circuit.svg was not created beside it");

    let saved_json = fs::read_to_string(&fig_path).expect("read json");
    assert!(saved_json.contains("\"r1\""));

    let saved_svg = fs::read_to_string(&svg_path).expect("read svg");
    assert!(saved_svg.starts_with("<svg"));
    assert!(saved_svg.contains("id=\"r1\""));

    // Open saved figure in a new kernel
    let k2 = test_kernel();
    let opened = json_of(
        k2.invoke(
            "figure.open",
            Payload::Json(json!({
                "path": fig_path.to_str().expect("utf8")
            })),
        )
        .expect("open"),
    );
    let opened_id = opened["id"].as_u64().expect("opened id");

    let opened_scene = json_of(
        k2.invoke("figure.get", Payload::Json(json!({ "id": opened_id })))
            .expect("get"),
    );
    assert_eq!(opened_scene["nodes"].as_array().expect("array").len(), 1);
    assert_eq!(opened_scene["nodes"][0]["id"], "r1");
}

#[test]
fn export_svg_binary_payload() {
    let k = test_kernel();
    let created = json_of(k.invoke("figure.create", Payload::Empty).expect("create"));
    let id = created["id"].as_u64().expect("id");

    let payload = k
        .invoke("figure.export-svg", Payload::Json(json!({ "id": id })))
        .expect("export-svg");

    let Payload::Binary(bytes) = payload else {
        panic!("expected Payload::Binary, got {payload:?}");
    };

    let svg_str = std::str::from_utf8(&bytes).expect("valid utf8");
    assert!(svg_str.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg_str.ends_with("</svg>\n"));
}

#[test]
fn hit_test_through_command() {
    let k = test_kernel();
    let created = json_of(k.invoke("figure.create", Payload::Empty).expect("create"));
    let id = created["id"].as_u64().expect("id");

    let op_add = json!({
        "id": id,
        "op": {
            "op": "add-node",
            "node": {
                "kind": "rect",
                "id": "box_hit",
                "x": 100.0,
                "y": 100.0,
                "width": 200.0,
                "height": 150.0,
                "stroke": "#3d3d42"
            }
        }
    });
    k.invoke("figure.apply", Payload::Json(op_add))
        .expect("add");

    // Hit inside rect
    let hit_res = json_of(
        k.invoke(
            "figure.hit-test",
            Payload::Json(json!({ "id": id, "x": 150.0, "y": 150.0 })),
        )
        .expect("hit-test inside"),
    );
    assert_eq!(hit_res["hit"], true);
    assert_eq!(hit_res["node_id"], "box_hit");

    // Miss outside rect
    let miss_res = json_of(
        k.invoke(
            "figure.hit-test",
            Payload::Json(json!({ "id": id, "x": 20.0, "y": 20.0 })),
        )
        .expect("hit-test outside"),
    );
    assert_eq!(miss_res["hit"], false);
    assert!(miss_res["node"].is_null());
}

#[test]
fn list_figures_and_dirty_flag() {
    let k = test_kernel();
    let f1 = json_of(k.invoke("figure.create", Payload::Empty).expect("create 1"));
    let f2 = json_of(k.invoke("figure.create", Payload::Empty).expect("create 2"));

    // Modify f1
    k.invoke(
        "figure.apply",
        Payload::Json(json!({
            "id": f1["id"],
            "op": {
                "op": "add-node",
                "node": {
                    "kind": "rect",
                    "id": "n1",
                    "x": 0.0,
                    "y": 0.0,
                    "width": 10.0,
                    "height": 10.0,
                    "stroke": "#000000"
                }
            }
        })),
    )
    .expect("apply");

    let list = json_of(k.invoke("figure.list", Payload::Empty).expect("list"));
    let arr = list.as_array().expect("array");
    assert_eq!(arr.len(), 2);

    let doc1 = arr.iter().find(|d| d["id"] == f1["id"]).expect("doc 1");
    let doc2 = arr.iter().find(|d| d["id"] == f2["id"]).expect("doc 2");

    assert_eq!(doc1["dirty"], true);
    assert_eq!(doc1["nodes"], 1);
    assert_eq!(doc2["dirty"], false);
    assert_eq!(doc2["nodes"], 0);
}

#[test]
fn invalid_arguments_return_errors_not_panics() {
    let k = test_kernel();
    for (cmd, args) in [
        ("figure.open", json!({})),
        ("figure.open", json!({ "path": 123 })),
        ("figure.apply", json!({})),
        ("figure.apply", json!({ "op": "not-an-op" })),
        ("figure.hit-test", json!({ "x": "bad" })),
        ("figure.hit-test", json!({ "x": 10.0, "y": "bad" })),
    ] {
        assert!(
            k.invoke(cmd, Payload::Json(args.clone())).is_err(),
            "expected error for {cmd} with {args}"
        );
    }

    assert!(
        k.invoke("figure.create", Payload::Binary(Arc::from(vec![1u8])))
            .is_err(),
        "expected error for binary payload on create"
    );
}

#[test]
fn svg_snapshot_through_plugin() {
    let k = test_kernel();
    let created = json_of(
        k.invoke(
            "figure.create",
            Payload::Json(json!({ "width": 400.0, "height": 300.0 })),
        )
        .expect("create"),
    );
    let id = created["id"].as_u64().expect("id");

    let op_add = json!({
        "id": id,
        "op": {
            "op": "add-node",
            "node": {
                "kind": "rect",
                "id": "box_snap",
                "x": 20.0,
                "y": 30.0,
                "width": 120.0,
                "height": 80.0,
                "stroke": "#c6f135",
                "fill": "#141416"
            }
        }
    });
    k.invoke("figure.apply", Payload::Json(op_add))
        .expect("add");

    let payload = k
        .invoke("figure.export-svg", Payload::Json(json!({ "id": id })))
        .expect("export-svg");

    let Payload::Binary(bytes) = payload else {
        panic!("expected binary");
    };
    let svg_str = std::str::from_utf8(&bytes).expect("utf8");
    insta::assert_snapshot!(svg_str);
}
