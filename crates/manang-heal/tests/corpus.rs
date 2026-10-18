//! The corpus of broken Typst and Markdown documents: each is healed one fix
//! at a time, and the sequence of fixes is snapshotted.
//!
//! Typst documents are compiled for real after every fix. The one that needs
//! a package from the Universe (`needs_cetz.typ`) is not compiled (tests have
//! no network); its fix is checked against the compiler's diagnostics only.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use common::{applied, run};
use manang_heal::{DiagnosticIn, Engine};
use manang_typst::{CompileRequest, Compiler, CompilerConfig, Environment};
use serde_json::{Value, json};

fn files(dir: &str, ext: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(dir);
    let mut out: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == ext))
        .collect();
    out.sort();
    out
}

fn compile(compiler: &Compiler, text: &str) -> Vec<DiagnosticIn> {
    let dir = tempfile::tempdir().expect("tempdir");
    let report = compiler
        .compile(&CompileRequest {
            main: "main.typ".to_owned(),
            root: Some(dir.path().to_path_buf()),
            sources: BTreeMap::from([("main.typ".to_owned(), text.to_owned())]),
        })
        .expect("compiles");
    report
        .diagnostics
        .iter()
        .map(|d| {
            serde_json::from_value(serde_json::to_value(d).expect("serialises")).expect("shape")
        })
        .filter(DiagnosticIn::is_error)
        .collect()
}

#[test]
fn typst_corpus_heals() {
    let compiler = Compiler::new(CompilerConfig {
        env: Environment::default(),
        root: None,
        threads: 1,
    })
    .expect("pool");
    let mut report: BTreeMap<String, Value> = BTreeMap::new();
    for path in files("typst", "typ") {
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        let mut text = fs::read_to_string(&path).expect("read");
        let mut diags = compile(&compiler, &text);
        assert!(!diags.is_empty(), "{name} must be broken");
        let mut steps: Vec<String> = Vec::new();
        for _ in 0..5 {
            if diags.is_empty() {
                break;
            }
            let s = run(Engine::Typst, &text, &diags);
            let Some(fix) = s.fixes.first() else { break };
            steps.push(format!("{} [{:?}]", fix.id, fix.confidence));
            if name == "needs_cetz.typ" {
                // No network for the package: only check the edit.
                assert_eq!(
                    applied(&text, fix).lines().next(),
                    Some("#import \"@preview/cetz:0.5.2\"")
                );
                break;
            }
            let before = diags.len();
            text = applied(&text, fix);
            diags = compile(&compiler, &text);
            assert!(
                diags.len() < before,
                "{name}: {} did not help\n{text}",
                fix.id
            );
        }
        if name != "needs_cetz.typ" {
            assert!(diags.is_empty(), "{name} still broken:\n{text}\n{diags:?}");
        }
        report.insert(name, json!({ "steps": steps, "result": text }));
    }
    insta::assert_json_snapshot!("typst_corpus", report);
}

#[test]
fn markdown_corpus_heals() {
    let mut report: BTreeMap<String, Value> = BTreeMap::new();
    for path in files("markdown", "md") {
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        let text = fs::read_to_string(&path).expect("read");
        let s = run(Engine::Markdown, &text, &[]);
        assert_eq!(s.fixes.len(), 1, "{name}");
        let fixed = applied(&text, &s.fixes[0]);
        assert!(
            run(Engine::Markdown, &fixed, &[]).fixes.is_empty(),
            "{name}: the fix leaves a problem\n{fixed}"
        );
        report.insert(
            name,
            json!({ "fix": s.fixes[0].id, "finding": s.findings[0].message, "result": fixed }),
        );
    }
    insta::assert_json_snapshot!("markdown_corpus", report);
}
