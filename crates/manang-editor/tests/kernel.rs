//! The editor driven end to end through the kernel, as the shell will.

use std::sync::Arc;

use manang_editor::{Editor, EditorPlugin, manifest};
use manang_kernel::{Kernel, KernelError, Payload, PluginError};
use serde_json::json;

fn kernel() -> (Kernel, Arc<Editor>) {
    let editor = Arc::new(Editor::new());
    let kernel = Kernel::new();
    kernel
        .register(
            manifest().expect("manifest is valid"),
            Arc::new(EditorPlugin::new(Arc::clone(&editor))),
        )
        .expect("registers");
    (kernel, editor)
}

fn call(k: &Kernel, command: &str, args: serde_json::Value) -> Result<Payload, KernelError> {
    k.invoke(command, Payload::Json(args))
}

#[test]
fn command_palette_lists_editor_commands() {
    let (k, _) = kernel();
    let ids: Vec<_> = k.commands().into_iter().map(|c| c.id).collect();
    for cmd in [
        "editor.new",
        "editor.open",
        "editor.save",
        "editor.activate",
        "editor.insert",
        "editor.delete",
        "editor.text",
        "editor.info",
        "editor.undo",
        "editor.redo",
        "editor.replace",
        "editor.slice",
        "editor.line-col",
    ] {
        assert!(ids.contains(&cmd.to_owned()), "missing {cmd}");
    }
    assert!(ids.iter().all(|id| id.starts_with("editor.")));
}

#[test]
fn edit_session_through_commands() {
    let (k, editor) = kernel();
    call(&k, "editor.new", json!({})).expect("new");
    call(&k, "editor.insert", json!({"at": 0, "text": "= Hello"})).expect("insert");
    call(&k, "editor.insert", json!({"at": 7, "text": ", world"})).expect("insert");
    call(&k, "editor.delete", json!({"from": 0, "to": 2})).expect("delete");

    assert_eq!(
        call(&k, "editor.text", json!({})).expect("text"),
        Payload::Json(json!("Hello, world"))
    );
    let Payload::Json(info) = call(&k, "editor.info", json!({})).expect("info") else {
        panic!("info is JSON");
    };
    assert_eq!(info["chars"], 12);
    assert_eq!(info["dirty"], true);
    assert_eq!(editor.list().len(), 1);
}

#[test]
fn undo_redo_replace_slice_line_col_through_commands() {
    let (k, _) = kernel();
    call(&k, "editor.new", json!({})).expect("new");
    call(
        &k,
        "editor.insert",
        json!({"at": 0, "text": "first line\nsecond line"}),
    )
    .expect("insert");

    // Slice
    let Payload::Json(slice_val) =
        call(&k, "editor.slice", json!({"from": 0, "to": 5})).expect("slice")
    else {
        panic!("expected json");
    };
    assert_eq!(slice_val, "first");

    // Line-col
    let Payload::Json(pos) = call(&k, "editor.line-col", json!({"char": 11})).expect("line-col")
    else {
        panic!("expected json");
    };
    assert_eq!(pos["line"], 1);
    assert_eq!(pos["column"], 0);

    // Replace
    call(
        &k,
        "editor.replace",
        json!({"from": 0, "to": 5, "text": "initial"}),
    )
    .expect("replace");
    assert_eq!(
        call(&k, "editor.text", json!({})).expect("text"),
        Payload::Json(json!("initial line\nsecond line"))
    );

    // Undo replace
    call(&k, "editor.undo", json!({})).expect("undo");
    assert_eq!(
        call(&k, "editor.text", json!({})).expect("text"),
        Payload::Json(json!("first line\nsecond line"))
    );

    // Redo replace
    call(&k, "editor.redo", json!({})).expect("redo");
    assert_eq!(
        call(&k, "editor.text", json!({})).expect("text"),
        Payload::Json(json!("initial line\nsecond line"))
    );
}

#[test]
fn bad_arguments_and_ranges_are_reported_not_panicked() {
    let (k, _) = kernel();
    call(&k, "editor.new", json!({})).expect("new");
    for (cmd, args) in [
        ("editor.insert", json!({"at": "zero", "text": "x"})),
        ("editor.insert", json!({"at": 99, "text": "x"})),
        ("editor.delete", json!({"from": 3, "to": 1})),
        ("editor.activate", json!({})),
        ("editor.text", json!({"id": 42})),
        ("editor.replace", json!({"from": 5, "to": 1, "text": "a"})),
        ("editor.slice", json!({"from": 10, "to": 2})),
        ("editor.line-col", json!({"char": 999})),
    ] {
        assert!(
            matches!(
                call(&k, cmd, args),
                Err(KernelError::Plugin(PluginError::Failed(_)))
            ),
            "{cmd}"
        );
    }
}
