//! The Typst plugin driven through the kernel, as the shell will.

use std::sync::Arc;

use manang_kernel::{Hook, Kernel, Payload};
use manang_typst::{CompilerConfig, manifest, plugin};
use serde_json::{Value, json};

const DOC: &str = "#set page(width: 100pt, height: 60pt)\nhello";

fn kernel() -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(
            manifest().expect("manifest is valid"),
            Arc::new(plugin(CompilerConfig::default()).expect("plugin starts")),
        )
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
fn manifest_is_native_and_namespaced() {
    let m = manifest().expect("valid");
    assert_eq!(m.plugin.name, "typst");
    assert_eq!(m.plugin.tier, manang_kernel::Tier::Native);
    assert_eq!(m.contributes.hooks, vec![Hook::OnLint]);
    let ids: Vec<_> = kernel().commands().into_iter().map(|c| c.id).collect();
    for id in [
        "typst.compile",
        "typst.render",
        "typst.export-pdf",
        "typst.jump-from-click",
        "typst.jump-from-cursor",
        "typst.svg-to-pdf",
    ] {
        assert!(ids.contains(&id.to_owned()), "{id}");
    }
}

#[test]
fn compile_render_export_through_commands() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_str().expect("utf8");
    let k = kernel();
    let report = json_of(
        k.invoke(
            "typst.compile",
            Payload::Json(json!({"main": "main.typ", "root": root, "sources": {"main.typ": DOC}})),
        )
        .expect("compile"),
    );
    assert_eq!(report["ok"], true);
    assert_eq!(report["pages"], 1);
    assert!(report["duration_ms"].as_f64().is_some());

    let Payload::Binary(svg) = k
        .invoke("typst.render", Payload::Json(json!({"page": 0})))
        .expect("render")
    else {
        panic!("render is binary");
    };
    assert!(svg.starts_with(b"<svg"));
    let Payload::Binary(pdf) = k
        .invoke("typst.export-pdf", Payload::Empty)
        .expect("export")
    else {
        panic!("pdf is binary");
    };
    assert!(pdf.starts_with(b"%PDF-"));
}

#[test]
fn check_reports_problems_and_leaves_the_preview_alone() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_str().expect("utf8");
    let k = kernel();
    let report = json_of(
        k.invoke(
            "typst.compile",
            Payload::Json(json!({"main": "main.typ", "root": root, "sources": {"main.typ": DOC}})),
        )
        .expect("compile"),
    );
    assert_eq!(report["ok"], true);
    let generation = report["generation"].clone();
    let checked = json_of(
        k.invoke(
            "typst.check",
            Payload::Json(json!({
                "main": "main.typ", "root": root,
                "sources": {"main.typ": "#figur(\"x\")"}
            })),
        )
        .expect("check"),
    );
    let list = checked["diagnostics"].as_array().expect("list");
    assert!(
        list.iter()
            .any(|d| d["severity"] == "error" && d["message"].as_str().unwrap_or("").contains("figur")),
        "{checked}"
    );
    // The cached document is still the good one: it renders, and a recompile continues the count.
    assert!(matches!(
        k.invoke("typst.render", Payload::Json(json!({"page": 0}))),
        Ok(Payload::Binary(_))
    ));
    let again = json_of(
        k.invoke(
            "typst.compile",
            Payload::Json(json!({"main": "main.typ", "root": root, "sources": {"main.typ": DOC}})),
        )
        .expect("compile"),
    );
    assert_eq!(again["generation"].as_u64(), generation.as_u64().map(|g| g + 1));
}

#[test]
fn bad_arguments_are_errors_not_panics() {
    let k = kernel();
    for (cmd, args) in [
        ("typst.compile", json!({})),
        ("typst.compile", json!({"main": 3})),
        ("typst.compile", json!({"main": "a.typ", "sources": []})),
        (
            "typst.compile",
            json!({"main": "a.typ", "sources": {"a.typ": 1}}),
        ),
        ("typst.render", json!({"page": "zero"})),
        ("typst.render", json!({"page": 0})),
        ("typst.jump-from-click", json!({"page": 0, "x": 1, "y": 1})),
        ("typst.jump-from-click", json!({"page": -1, "x": 1, "y": 1})),
        (
            "typst.jump-from-click",
            json!({"page": 0, "x": "a", "y": 1}),
        ),
        ("typst.jump-from-click", json!({"page": 0, "y": 1})),
        ("typst.jump-from-cursor", json!({"line": 1, "column": 1})),
        (
            "typst.jump-from-cursor",
            json!({"path": "a.typ", "line": 1}),
        ),
        (
            "typst.jump-from-cursor",
            json!({"path": "a.typ", "line": 1, "column": 1}),
        ),
    ] {
        assert!(
            k.invoke(cmd, Payload::Json(args.clone())).is_err(),
            "{cmd} {args}"
        );
    }
    assert!(
        k.invoke("typst.compile", Payload::Binary(Arc::from(vec![1u8])))
            .is_err()
    );
}

#[test]
fn on_lint_exposes_diagnostics() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_str().expect("utf8");
    let k = kernel();
    let out = k
        .run_hook(
            Hook::OnLint,
            Payload::Json(
                json!({"main": "main.typ", "root": root, "sources": {"main.typ": "#nope"}}),
            ),
        )
        .expect("hook");
    assert_eq!(out.len(), 1);
    let (plugin, payload) = out.into_iter().next().expect("one subscriber");
    assert_eq!(plugin, "typst");
    let body = json_of(payload);
    assert_eq!(body["diagnostics"][0]["severity"], "error");
    assert_eq!(body["diagnostics"][0]["line"], 1);

    // Without a main it reports the last compile.
    let again = k.run_hook(Hook::OnLint, Payload::Empty).expect("hook");
    assert_eq!(
        json_of(again[0].1.clone())["diagnostics"],
        body["diagnostics"]
    );
}

#[test]
fn jumps_through_commands() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_str().expect("utf8");
    let k = kernel();
    let report = json_of(
        k.invoke(
            "typst.compile",
            Payload::Json(json!({"main": "main.typ", "root": root, "sources": {"main.typ": DOC}})),
        )
        .expect("compile"),
    );
    assert_eq!(report["ok"], true);

    let positions = json_of(
        k.invoke(
            "typst.jump-from-cursor",
            Payload::Json(json!({"path": "main.typ", "line": 2, "column": 2})),
        )
        .expect("cursor"),
    );
    let first = &positions[0];
    assert_eq!(first["page"], 0);
    let x = first["x"].as_f64().expect("x");
    let y = first["y"].as_f64().expect("y");

    let jump = json_of(
        k.invoke(
            "typst.jump-from-click",
            Payload::Json(json!({"page": 0, "x": x + 1.0, "y": y - 2.0})),
        )
        .expect("click"),
    );
    assert_eq!(jump["kind"], "file");
    assert_eq!(jump["path"], "main.typ");
    assert_eq!(jump["line"], 2);
    assert!(jump["column"].as_u64().is_some_and(|c| c >= 1));

    let nothing = json_of(
        k.invoke(
            "typst.jump-from-click",
            Payload::Json(json!({"page": 0, "x": 99.0, "y": 59.0})),
        )
        .expect("click"),
    );
    assert_eq!(nothing, Value::Null);
}
