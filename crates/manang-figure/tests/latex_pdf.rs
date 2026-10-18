//! In a LaTeX project, saving a figure also writes the PDF that
//! `\includegraphics` takes, through the project and Typst plugins.

use std::fs;
use std::sync::Arc;

use manang_kernel::{EventBus, Kernel, Payload};
use serde_json::{Value, json};

fn kernel(dir: &std::path::Path, engine: &str) -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(
            manang_figure::manifest().expect("manifest"),
            Arc::new(manang_figure::plugin()),
        )
        .expect("figure");
    kernel
        .register(
            manang_project::manifest().expect("manifest"),
            Arc::new(manang_project::ProjectPlugin::new(
                dir.join("config"),
                EventBus::new(),
            )),
        )
        .expect("project");
    kernel
        .register(
            manang_typst::manifest().expect("manifest"),
            Arc::new(manang_typst::plugin(manang_typst::CompilerConfig::default()).expect("typst")),
        )
        .expect("typst");
    let root = dir.join("paper");
    kernel
        .invoke(
            "project.create",
            Payload::Json(json!({ "path": root.to_string_lossy(), "name": "P", "engine": engine })),
        )
        .expect("project");
    kernel
}

fn json_of(payload: Payload) -> Value {
    match payload {
        Payload::Json(v) => v,
        other => panic!("expected JSON, got {other:?}"),
    }
}

#[test]
fn latex_projects_get_a_pdf_next_to_the_svg() {
    let dir = tempfile::tempdir().expect("tmp");
    let k = kernel(dir.path(), "latex");
    let figure = dir.path().join("paper/figures/sketch.figure.json");
    let created = json_of(
        k.invoke(
            "figure.create",
            Payload::Json(json!({ "path": figure.to_string_lossy(), "width": 200, "height": 100 })),
        )
        .expect("create"),
    );
    assert_eq!(created["pdf"], "figures/sketch.pdf");
    let pdf = fs::read(dir.path().join("paper/figures/sketch.pdf")).expect("pdf");
    assert!(pdf.starts_with(b"%PDF-"));

    let saved = json_of(
        k.invoke("figure.save", Payload::Json(json!({ "id": created["id"] })))
            .expect("save"),
    );
    assert_eq!(saved["pdf"], "figures/sketch.pdf");
}

#[test]
fn typst_projects_get_no_pdf() {
    let dir = tempfile::tempdir().expect("tmp");
    let k = kernel(dir.path(), "typst");
    let figure = dir.path().join("paper/figures/sketch.figure.json");
    let created = json_of(
        k.invoke(
            "figure.create",
            Payload::Json(json!({ "path": figure.to_string_lossy() })),
        )
        .expect("create"),
    );
    assert_eq!(created["pdf"], Value::Null);
    assert!(!dir.path().join("paper/figures/sketch.pdf").exists());
}
