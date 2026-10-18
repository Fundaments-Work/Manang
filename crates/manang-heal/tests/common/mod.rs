//! Helpers shared by the integration tests.
#![allow(dead_code)] // each test binary uses a different subset

use manang_heal::{DiagnosticIn, Engine, Fix, Suggestion, apply_edits, suggest};
use serde_json::{Value, json};

pub fn latex_diag(message: &str, line: usize) -> DiagnosticIn {
    DiagnosticIn::at_line(message, line)
}

/// A Typst diagnostic with a scalar range into `text`, found by `needle`.
pub fn typst_diag(text: &str, message: &str, needle: &str) -> DiagnosticIn {
    let byte = text.find(needle).expect("needle in text");
    let start = text[..byte].chars().count();
    let line = text[..byte].matches('\n').count() + 1;
    DiagnosticIn {
        severity: Some("error".to_owned()),
        message: message.to_owned(),
        line: Some(line),
        range: Some(manang_heal::ScalarRange {
            start,
            end: start + needle.chars().count(),
        }),
        ..DiagnosticIn::default()
    }
}

pub fn run(engine: Engine, text: &str, diags: &[DiagnosticIn]) -> Suggestion {
    suggest(engine, text, diags)
}

pub fn applied(text: &str, fix: &Fix) -> String {
    apply_edits(text, &fix.edits)
}

/// The suggestion and each fix's result, for snapshots.
pub fn show(engine: Engine, text: &str, diags: &[DiagnosticIn]) -> Value {
    let s = run(engine, text, diags);
    let results: Vec<Value> = s
        .fixes
        .iter()
        .map(|f| json!({ "id": f.id, "after": applied(text, f) }))
        .collect();
    json!({
        "suggestion": serde_json::to_value(&s).expect("serialises"),
        "results": results,
        "fix_all": s.fix_all.as_ref().map(|a| apply_edits(text, &a.edits)),
    })
}
