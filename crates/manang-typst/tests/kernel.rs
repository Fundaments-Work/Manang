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
    for id in ["typst.compile", "typst.render", "typst.export-pdf"] {
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
