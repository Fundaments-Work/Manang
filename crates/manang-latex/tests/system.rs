//! The system TeX backend against the TeX installation on this machine.
//!
//! Every test skips (with a message) when no `pdflatex` is on `PATH`, so the
//! suite stays green on machines without TeX.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use manang_kernel::{Kernel, Payload};
use manang_latex::{BackendChoice, LatexConfig, LatexPlugin, TexTools, manifest};
use serde_json::{Value, json};

fn kernel_with(tools: TexTools, cache: &Path) -> Kernel {
    let config = LatexConfig {
        backend: BackendChoice::System,
        cache_dir: Some(cache.to_path_buf()),
        timeout: Duration::from_secs(60),
        ..LatexConfig::default()
    };
    let kernel = Kernel::new();
    kernel
        .register(
            manifest().expect("manifest"),
            Arc::new(LatexPlugin::with_tools(config, tools)),
        )
        .expect("register");
    kernel
}

/// The installed tools, or `None` (and a skip message) without TeX.
fn system_tex() -> Option<TexTools> {
    let tools = TexTools::detect();
    if tools.is_available() && tools.engines.contains_key(&manang_latex::Engine::Pdflatex) {
        Some(tools)
    } else {
        eprintln!("SKIP: no pdflatex on PATH");
        None
    }
}

fn json_of(payload: Payload) -> Value {
    match payload {
        Payload::Json(v) => v,
        other => panic!("expected JSON, got {other:?}"),
    }
}

fn compile(kernel: &Kernel, args: Value) -> Value {
    json_of(
        kernel
            .invoke("latex.compile", Payload::Json(args))
            .expect("compile runs"),
    )
}

const MINIMAL: &str =
    "\\documentclass{article}\n\\begin{document}\nHello, \\LaTeX.\n\\end{document}\n";

#[test]
fn status_reports_the_installed_distribution() {
    let Some(tools) = system_tex() else { return };
    let cache = tempfile::tempdir().expect("tmp");
    let k = kernel_with(tools, cache.path());
    let status = json_of(k.invoke("latex.status", Payload::Empty).expect("status"));
    assert_eq!(status["default"], "system");
    let system = &status["backends"][0];
    assert_eq!(system["available"], true);
    assert!(
        system["engines"]
            .as_array()
            .is_some_and(|e| e.iter().any(|v| v == "pdflatex"))
    );
    assert!(
        system["version"]
            .as_str()
            .is_some_and(|v| v.contains("TeX"))
    );
}

#[test]
fn minimal_document_compiles_with_both_pipelines() {
    let Some(tools) = system_tex() else { return };
    let cache = tempfile::tempdir().expect("tmp");
    let k = kernel_with(tools, cache.path());
    for latexmk in [true, false] {
        let report = compile(
            &k,
            json!({"main": "main.tex", "sources": {"main.tex": MINIMAL}, "latexmk": latexmk}),
        );
        assert_eq!(report["ok"], true, "latexmk={latexmk}: {report}");
        assert_eq!(report["pages"], 1);
        assert_eq!(report["backend"], "system");
        assert_eq!(report["engine"], "pdflatex");
        assert_eq!(report["synctex"], true);
        let pdf = k
            .invoke(
                "latex.export-pdf",
                Payload::Json(json!({"main": "main.tex"})),
            )
            .expect("pdf");
        let Payload::Binary(bytes) = pdf else {
            panic!("expected PDF bytes")
        };
        assert!(bytes.starts_with(b"%PDF"));
    }
}

#[test]
fn errors_become_diagnostics_with_lines() {
    let Some(tools) = system_tex() else { return };
    let cache = tempfile::tempdir().expect("tmp");
    let k = kernel_with(tools, cache.path());
    let broken =
        "\\documentclass{article}\n\\begin{document}\nfine\n\\undefinedmacro\n\\end{document}\n";
    let report = compile(
        &k,
        json!({"main": "main.tex", "sources": {"main.tex": broken}}),
    );
    assert_eq!(report["ok"], false, "{report}");
    let diagnostics = report["diagnostics"].as_array().expect("diagnostics");
    let error = diagnostics
        .iter()
        .find(|d| d["severity"] == "error")
        .expect("an error diagnostic");
    assert!(
        error["message"]
            .as_str()
            .is_some_and(|m| m.contains("Undefined control sequence")),
        "{error}"
    );
    assert_eq!(error["line"], 4, "{error}");
    assert!(
        k.invoke(
            "latex.export-pdf",
            Payload::Json(json!({"main": "main.tex"}))
        )
        .is_err(),
        "a failed build must not export a stale PDF"
    );
}

#[test]
fn on_disk_project_with_chapters_and_bibtex_leaves_the_project_clean() {
    let Some(tools) = system_tex() else { return };
    let cache = tempfile::tempdir().expect("tmp");
    let project = tempfile::tempdir().expect("tmp");
    let root = project.path();
    fs::create_dir_all(root.join("chapters")).expect("mkdir");
    fs::write(
        root.join("main.tex"),
        "\\documentclass{article}\n\\begin{document}\n\\include{chapters/one}\n\
         \\bibliographystyle{plain}\n\\bibliography{refs}\n\\end{document}\n",
    )
    .expect("write");
    fs::write(
        root.join("chapters/one.tex"),
        "\\section{One}\\label{sec:one}\nSee \\cite{landauer1961} and Section~\\ref{sec:one}.\n",
    )
    .expect("write");
    fs::write(
        root.join("refs.bib"),
        "@article{landauer1961,\n  author = {Landauer, Rolf},\n  title = {Irreversibility and Heat Generation},\n  journal = {IBM J. Res. Dev.},\n  year = {1961},\n  volume = {5},\n  pages = {183--191}\n}\n",
    )
    .expect("write");
    let before: Vec<_> = list(root);

    let k = kernel_with(tools, cache.path());
    let main = root.join("main.tex");
    for latexmk in [true, false] {
        let report = compile(
            &k,
            json!({"main": main.to_string_lossy(), "latexmk": latexmk}),
        );
        assert_eq!(report["ok"], true, "latexmk={latexmk}: {report}");
        let unresolved = report["diagnostics"].as_array().is_some_and(|d| {
            d.iter().any(|x| {
                x["message"]
                    .as_str()
                    .is_some_and(|m| m.contains("undefined") || m.contains("Citation"))
            })
        });
        assert!(!unresolved, "citations and references resolve: {report}");
    }
    assert_eq!(list(root), before, "no aux files written into the project");
}

#[test]
fn shell_escape_is_disabled() {
    let Some(tools) = system_tex() else { return };
    let cache = tempfile::tempdir().expect("tmp");
    let marker_dir = tempfile::tempdir().expect("tmp");
    let marker = marker_dir.path().join("pwned");
    let k = kernel_with(tools, cache.path());
    let doc = format!(
        "\\documentclass{{article}}\n\\begin{{document}}\n\\immediate\\write18{{touch {}}}\nx\n\\end{{document}}\n",
        marker.display()
    );
    let _ = compile(
        &k,
        json!({"main": "main.tex", "sources": {"main.tex": doc}}),
    );
    assert!(!marker.exists(), "\\write18 must not run commands");
}

#[test]
fn runaway_documents_are_killed_at_the_deadline() {
    let Some(tools) = system_tex() else { return };
    let cache = tempfile::tempdir().expect("tmp");
    let k = kernel_with(tools, cache.path());
    let looping =
        "\\documentclass{article}\n\\begin{document}\n\\def\\x{\\x}\\x\n\\end{document}\n";
    let started = std::time::Instant::now();
    let err = k
        .invoke(
            "latex.compile",
            Payload::Json(json!({
                "main": "main.tex",
                "sources": {"main.tex": looping},
                "timeout_ms": 3000,
                "latexmk": false
            })),
        )
        .expect_err("must time out");
    assert!(err.to_string().contains("[timeout]"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(15));
}

#[test]
fn unknown_engine_names_are_rejected() {
    let Some(tools) = system_tex() else { return };
    let cache = tempfile::tempdir().expect("tmp");
    let k = kernel_with(tools, cache.path());
    let err = k
        .invoke(
            "latex.compile",
            Payload::Json(
                json!({"main": "main.tex", "sources": {"main.tex": MINIMAL}, "engine": "tex"}),
            ),
        )
        .expect_err("bad engine");
    assert!(err.to_string().contains("[bad_argument]"), "{err}");
}

#[test]
fn without_tex_the_error_says_how_to_fix_it() {
    let cache = tempfile::tempdir().expect("tmp");
    let k = kernel_with(TexTools::default(), cache.path());
    let err = k
        .invoke(
            "latex.compile",
            Payload::Json(json!({"main": "main.tex", "sources": {"main.tex": MINIMAL}})),
        )
        .expect_err("no TeX");
    let text = err.to_string();
    assert!(
        text.contains("[no_tex]") && text.contains("TeX Live"),
        "{text}"
    );
}

fn list(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read_dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path.clone());
            }
            out.push(
                path.strip_prefix(root)
                    .expect("prefix")
                    .display()
                    .to_string(),
            );
        }
    }
    out.sort();
    out
}
