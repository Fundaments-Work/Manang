//! The heal plugin driven through the kernel, as the shell will.

use std::sync::Arc;

use manang_heal::{manifest, plugin};
use manang_kernel::{Kernel, KernelError, Payload, PluginError, Tier};
use serde_json::{Value, json};

fn kernel() -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(manifest().expect("manifest is valid"), Arc::new(plugin()))
        .expect("registers");
    kernel
}

fn call(k: &Kernel, command: &str, args: Value) -> Result<Value, KernelError> {
    match k.invoke(command, Payload::Json(args))? {
        Payload::Json(v) => Ok(v),
        other => panic!("expected JSON, got {other:?}"),
    }
}

const DOC: &str =
    "\\documentclass{article}\n\\begin{document}\n\\includegraphics{a}\n\\end{document}\n";

fn request() -> Value {
    json!({
        "engine": "latex",
        "file": "main.tex",
        "text": DOC,
        "diagnostics": [{
            "severity": "error",
            "message": "Undefined control sequence: \\includegraphics",
            "file": "main.tex",
            "line": 3,
            "hint": "add it"
        }]
    })
}

#[test]
fn manifest_declares_a_pure_native_plugin() {
    let m = manifest().expect("valid");
    assert_eq!(m.plugin.name, "heal");
    assert_eq!(m.plugin.tier, Tier::Native);
    assert!(m.permissions.network.is_empty());
    assert!(m.permissions.filesystem.is_empty());
    let ids: Vec<&str> = m
        .contributes
        .settings
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(ids, ["heal.enabled", "heal.auto-apply"]);
}

#[test]
fn every_declared_command_is_handled() {
    let k = kernel();
    for command in k.commands() {
        let result = call(&k, &command.id, json!({}));
        if let Err(KernelError::Plugin(PluginError::UnknownCommand(c))) = &result {
            panic!("`{c}` is declared but not handled");
        }
    }
    assert!(matches!(
        call(&k, "heal.nope", json!({})),
        Err(KernelError::UnknownCommand(_))
    ));
}

#[test]
fn suggest_returns_fixes_with_utf16_edits() {
    let k = kernel();
    let out = call(&k, "heal.suggest", request()).expect("suggest");
    assert_eq!(out["enabled"], true);
    let fix = &out["fixes"][0];
    assert_eq!(fix["id"], "latex.package:graphicx");
    assert_eq!(fix["confidence"], "certain");
    assert_eq!(fix["edits"][0]["insert"], "\n\\usepackage{graphicx}");
    assert_eq!(fix["edits"][0]["from"], 23);
    assert_eq!(fix["diagnostics"], json!([0]));
}

#[test]
fn bad_arguments_are_named() {
    let k = kernel();
    for (args, needle) in [
        (json!({"text": "x"}), "engine"),
        (json!({"engine": "latex"}), "text"),
        (json!({"engine": "word", "text": "x"}), "unknown engine"),
        (
            json!({"engine": "latex", "text": "x", "diagnostics": 3}),
            "diagnostics",
        ),
    ] {
        let err = call(&k, "heal.suggest", args).expect_err("rejected");
        assert!(err.to_string().contains(needle), "{err}");
    }
}

#[test]
fn settings_turn_fixes_off_and_report_auto_apply() {
    let k = kernel();
    let applied = call(
        &k,
        "heal.configure",
        json!({"values": {"heal.enabled": false, "heal.auto-apply": true, "other.x": 1}}),
    )
    .expect("configure");
    assert_eq!(
        applied,
        json!({"heal.enabled": false, "heal.auto-apply": true})
    );
    let out = call(&k, "heal.suggest", request()).expect("suggest");
    assert_eq!(out["enabled"], false);
    assert_eq!(out["fixes"], json!([]));

    let err = call(
        &k,
        "heal.configure",
        json!({"values": {"heal.enabled": "yes"}}),
    )
    .expect_err("type checked");
    assert!(err.to_string().contains("boolean"), "{err}");
    // A rejected push changes nothing.
    let out = call(&k, "heal.suggest", request()).expect("suggest");
    assert_eq!(out["enabled"], false);
}

#[test]
fn diagnostics_may_be_absent() {
    let k = kernel();
    let out = call(&k, "heal.suggest", json!({"engine": "latex", "text": DOC})).expect("ok");
    assert_eq!(out["fixes"], json!([]));
}
