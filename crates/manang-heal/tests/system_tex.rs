//! The closed loop against a real TeX: compile a broken document, ask the
//! heal plugin for fixes, apply the first, and compile again until it passes.
//!
//! Every test skips (with a message) when no `pdflatex` is on `PATH`, like the
//! `manang-latex` system tests, so CI without TeX stays green.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use manang_heal::{Edit, apply_edits};
use manang_kernel::{Kernel, Payload};
use manang_latex::{BackendChoice, Engine, LatexConfig, LatexPlugin, TexTools};
use serde_json::{Value, json};

fn kernel(tools: TexTools, cache: &Path) -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(
            manang_latex::manifest().expect("latex manifest"),
            Arc::new(LatexPlugin::with_tools(
                LatexConfig {
                    backend: BackendChoice::System,
                    cache_dir: Some(cache.to_path_buf()),
                    timeout: Duration::from_secs(90),
                    ..LatexConfig::default()
                },
                tools,
            )),
        )
        .expect("register latex");
    kernel
        .register(
            manang_heal::manifest().expect("heal manifest"),
            Arc::new(manang_heal::plugin()),
        )
        .expect("register heal");
    kernel
}

fn system_tex() -> Option<TexTools> {
    let tools = TexTools::detect();
    if tools.is_available() && tools.engines.contains_key(&Engine::Pdflatex) {
        Some(tools)
    } else {
        eprintln!("SKIP: no pdflatex on PATH");
        None
    }
}

fn call(k: &Kernel, command: &str, args: Value) -> Value {
    match k
        .invoke(command, Payload::Json(args))
        .unwrap_or_else(|e| panic!("{command}: {e}"))
    {
        Payload::Json(v) => v,
        other => panic!("expected JSON, got {other:?}"),
    }
}

fn compile(k: &Kernel, text: &str) -> Value {
    call(
        k,
        "latex.compile",
        json!({"main": "main.tex", "sources": {"main.tex": text}}),
    )
}

fn error_count(report: &Value) -> usize {
    report["diagnostics"]
        .as_array()
        .map_or(0, |d| d.iter().filter(|d| d["severity"] == "error").count())
}

/// Heal `text` one fix at a time (the shell's Fix button), at most `rounds`
/// times. Returns the fixed text and the ids applied.
fn heal(k: &Kernel, text: &str, rounds: usize) -> (String, Vec<String>, Value) {
    let mut text = text.to_owned();
    let mut applied = Vec::new();
    let mut report = compile(k, &text);
    for _ in 0..rounds {
        if report["ok"] == true {
            break;
        }
        let out = call(
            k,
            "heal.suggest",
            json!({
                "engine": "latex",
                "file": "main.tex",
                "text": text,
                "diagnostics": report["diagnostics"],
            }),
        );
        let Some(fix) = out["fixes"].get(0) else {
            break;
        };
        let edits: Vec<Edit> = serde_json::from_value(fix["edits"].clone()).expect("edits");
        let before = error_count(&report);
        let targeted: Vec<String> = fix["diagnostics"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|i| report["diagnostics"][i.as_u64()? as usize]["message"].as_str())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        text = apply_edits(&text, &edits);
        applied.push(fix["id"].as_str().expect("id").to_owned());
        report = compile(k, &text);
        // The verification loop of the shell: the fix worked when the error
        // it targeted is gone or the error count dropped; otherwise the shell
        // offers Undo. (TeX stops at the first error, so a fix often reveals
        // the next one without lowering the count.)
        let still_there = report["diagnostics"].as_array().is_some_and(|d| {
            d.iter()
                .any(|d| targeted.iter().any(|t| d["message"].as_str() == Some(t)))
        });
        assert!(
            !still_there || error_count(&report) < before,
            "{} did not work ({before} -> {})\n{text}\n{report}",
            applied.last().expect("applied"),
            error_count(&report)
        );
    }
    (text, applied, report)
}

fn corpus(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus/latex")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn assert_heals(name: &str, expect_ids: &[&str]) {
    let Some(tools) = system_tex() else { return };
    let cache = tempfile::tempdir().expect("tmp");
    let k = kernel(tools, cache.path());
    let broken = corpus(name);
    assert_eq!(compile(&k, &broken)["ok"], false, "{name} must be broken");
    let (fixed, applied, report) = heal(&k, &broken, 6);
    assert_eq!(
        report["ok"], true,
        "{name}: still broken\n{fixed}\n{report}"
    );
    assert_eq!(applied, expect_ids, "{name}");
}

#[test]
fn missing_graphicx() {
    assert_heals("graphicx.tex", &["latex.package:graphicx"]);
}

#[test]
fn missing_xcolor() {
    assert_heals("xcolor.tex", &["latex.package:xcolor"]);
}

#[test]
fn missing_amsmath_environment() {
    assert_heals("align.tex", &["latex.package:amsmath"]);
}

#[test]
fn missing_booktabs() {
    assert_heals("booktabs.tex", &["latex.package:booktabs"]);
}

#[test]
fn tikz_and_its_libraries() {
    assert_heals(
        "tikz_libraries.tex",
        &[
            "latex.package:tikz",
            "latex.tikzlibrary:arrows.meta,positioning",
        ],
    );
}

#[test]
fn undeclared_theorem() {
    assert_heals("theorem.tex", &["latex.theorem:theorem"]);
}

#[test]
fn missing_end_document() {
    assert_heals("missing_end_document.tex", &["latex.end:document:eof"]);
}

#[test]
fn unclosed_brace_in_a_paragraph() {
    let Some(tools) = system_tex() else { return };
    let cache = tempfile::tempdir().expect("tmp");
    let k = kernel(tools, cache.path());
    let (fixed, applied, report) = heal(&k, &corpus("unclosed_brace.tex"), 3);
    assert_eq!(report["ok"], true, "{fixed}\n{report}");
    assert_eq!(applied.len(), 1);
    assert!(applied[0].starts_with("latex.close-brace"), "{applied:?}");
    assert!(fixed.contains("Some \\textbf{bold text}\n"));
}

#[test]
fn mismatched_end_environment() {
    let Some(tools) = system_tex() else { return };
    let cache = tempfile::tempdir().expect("tmp");
    let k = kernel(tools, cache.path());
    let (fixed, applied, report) = heal(&k, &corpus("wrong_end.tex"), 3);
    assert_eq!(report["ok"], true, "{fixed}\n{report}");
    assert!(applied[0].starts_with("latex.end-rename"), "{applied:?}");
}

#[test]
fn several_errors_one_fix_all_pass() {
    let Some(tools) = system_tex() else { return };
    let cache = tempfile::tempdir().expect("tmp");
    let k = kernel(tools, cache.path());
    let broken = corpus("several.tex");
    // TeX stops at the first error of a document, so a few rounds are needed
    // even with Fix all; each round must still make progress.
    let mut text = broken.clone();
    let mut report = compile(&k, &text);
    let mut rounds = 0;
    while report["ok"] != true && rounds < 6 {
        let out = call(
            &k,
            "heal.suggest",
            json!({"engine": "latex", "text": text, "diagnostics": report["diagnostics"]}),
        );
        let edits_value = if out["fix_all"].is_object() {
            out["fix_all"]["edits"].clone()
        } else {
            out["fixes"][0]["edits"].clone()
        };
        let edits: Vec<Edit> = serde_json::from_value(edits_value).expect("edits");
        text = apply_edits(&text, &edits);
        report = compile(&k, &text);
        rounds += 1;
    }
    assert_eq!(report["ok"], true, "{text}\n{report}");
    for package in ["graphicx", "xcolor", "booktabs"] {
        assert_eq!(
            text.matches(&format!("\\usepackage{{{package}}}")).count(),
            1,
            "{package} exactly once in\n{text}"
        );
    }
}
