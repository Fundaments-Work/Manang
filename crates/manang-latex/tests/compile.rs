//! End-to-end tests through the kernel, with the real engine.
//!
//! Tests that need TeX support files skip (with a message) when the bundle is
//! not cached: set `MANANG_LATEX_CACHE` to a populated cache, or compile once
//! with network access (`MANANG_LATEX_TEST_NETWORK=1`) to fill the default one.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use manang_kernel::{Hook, Kernel, KernelError, Payload, PluginError};
use manang_latex::{BackendChoice, LatexConfig, manifest, plugin, resolve_cache_dir};
use serde_json::{Value, json};

const WORKER: &str = env!("CARGO_BIN_EXE_manang-latex-worker");

const ARTICLE: &str = "\\documentclass{article}\n\\begin{document}\nHello, \\textbf{world}.\n\
                       \n\\input{chapter}\n\\end{document}\n";
const CHAPTER: &str = "A second paragraph, $E=mc^2$.\n";

fn config(cache: Option<PathBuf>) -> LatexConfig {
    LatexConfig {
        worker: Some(PathBuf::from(WORKER)),
        // These tests exercise the optional Tectonic worker specifically;
        // the system backend has its own suite (tests/system.rs).
        backend: BackendChoice::Tectonic,
        cache_dir: cache,
        timeout: Duration::from_secs(120),
        allow_network: std::env::var_os("MANANG_LATEX_TEST_NETWORK").is_some(),
        ..LatexConfig::default()
    }
}

/// Whether the Tectonic worker exists. It builds only with the `engine`
/// feature (`cargo build -p manang-latex --features engine`), which the
/// workspace gate leaves off; tests that spawn it skip (saying why) without it.
fn worker_built() -> bool {
    let built = std::path::Path::new(WORKER).is_file();
    if !built {
        eprintln!("SKIP: {WORKER} is not built; build manang-latex with --features engine");
    }
    built
}

fn kernel_with(config: LatexConfig) -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(manifest().expect("manifest"), Arc::new(plugin(config)))
        .expect("register");
    kernel
}

/// A kernel on the user's cache, or `None` (after saying why) when the
/// worker is not built or the bundle is not available offline.
fn engine_kernel() -> Option<Kernel> {
    if !worker_built() {
        return None;
    }
    let cfg = config(None);
    let cache = resolve_cache_dir(&cfg).ok()?;
    let populated = std::fs::read_dir(cache.join("bundles"))
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);
    if !populated && !cfg.allow_network {
        eprintln!(
            "SKIP: no cached TeX bundle in {}; set MANANG_LATEX_CACHE or MANANG_LATEX_TEST_NETWORK=1",
            cache.display()
        );
        return None;
    }
    Some(kernel_with(cfg))
}

fn json_of(payload: Payload) -> Value {
    match payload {
        Payload::Json(v) => v,
        other => panic!("expected JSON, got {other:?}"),
    }
}

fn sources() -> Value {
    json!({"main.tex": ARTICLE, "chapter.tex": CHAPTER})
}

#[test]
fn compiles_a_tiny_article_and_exports_the_pdf() {
    let Some(k) = engine_kernel() else { return };
    let out = json_of(
        k.invoke(
            "latex.compile",
            Payload::Json(json!({"main": "main.tex", "sources": sources()})),
        )
        .expect("compile"),
    );
    assert_eq!(out["ok"], true, "{out}");
    assert_eq!(out["pages"], 1);
    assert_eq!(out["synctex"], true);
    assert!(
        out["diagnostics"].as_array().is_some_and(Vec::is_empty),
        "{out}"
    );
    assert!(out["duration_ms"].as_u64().is_some());

    let Payload::Binary(pdf) = k
        .invoke(
            "latex.export-pdf",
            Payload::Json(json!({"main": "main.tex"})),
        )
        .expect("export")
    else {
        panic!("export-pdf must return binary");
    };
    assert!(pdf.starts_with(b"%PDF-"), "not a PDF");
}

#[test]
fn synctex_maps_both_ways() {
    let Some(k) = engine_kernel() else { return };
    k.invoke(
        "latex.compile",
        Payload::Json(json!({"main": "main.tex", "sources": sources()})),
    )
    .expect("compile");

    let fwd = json_of(
        k.invoke(
            "latex.synctex",
            Payload::Json(json!({"file": "chapter.tex", "line": 1})),
        )
        .expect("forward"),
    );
    let pos = &fwd["positions"][0];
    assert_eq!(pos["page"], 1, "{fwd}");
    let (x, y) = (
        pos["x"].as_f64().expect("x") + 2.0,
        pos["y"].as_f64().expect("y") + pos["height"].as_f64().expect("h") / 2.0,
    );

    let rev = json_of(
        k.invoke(
            "latex.synctex",
            Payload::Json(json!({"page": 1, "x": x, "y": y})),
        )
        .expect("reverse"),
    );
    assert_eq!(rev["file"], "chapter.tex", "{rev}");
    assert_eq!(rev["line"], 1);
}

#[test]
fn tex_errors_become_diagnostics() {
    let Some(k) = engine_kernel() else { return };
    let src = json!({"main.tex": "\\documentclass{article}\n\\begin{document}\nHi \\nosuchmacro\n\\end{document}\n"});
    let out = json_of(
        k.invoke(
            "latex.compile",
            Payload::Json(json!({"main": "main.tex", "sources": src})),
        )
        .expect("compile returns diagnostics, not Err"),
    );
    assert_eq!(out["ok"], false);
    let d = &out["diagnostics"][0];
    assert_eq!(d["severity"], "error");
    assert_eq!(d["file"], "main.tex");
    assert_eq!(d["line"], 3);
    assert!(
        d["hint"]
            .as_str()
            .is_some_and(|h| h.contains("nosuchmacro"))
    );
    // export-pdf of a failed build is a structured error.
    let err = k
        .invoke(
            "latex.export-pdf",
            Payload::Json(json!({"main": "main.tex"})),
        )
        .expect_err("no pdf");
    assert!(err.to_string().contains("[compile_failed]"), "{err}");
}

#[test]
fn on_disk_projects_are_not_polluted_and_builds_are_deterministic() {
    let Some(k) = engine_kernel() else { return };
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("main.tex"), ARTICLE).expect("write");
    std::fs::write(dir.path().join("chapter.tex"), CHAPTER).expect("write");
    let main = dir.path().join("main.tex");
    let list = || {
        let mut names: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .map(|e| e.expect("entry").file_name())
            .collect();
        names.sort();
        names
    };
    let before = list();
    let args = json!({"main": main, "recompile": true});
    let mut pdfs = Vec::new();
    for _ in 0..2 {
        let out = json_of(
            k.invoke("latex.compile", Payload::Json(args.clone()))
                .expect("compile"),
        );
        assert_eq!(out["ok"], true, "{out}");
        let Payload::Binary(pdf) = k
            .invoke("latex.export-pdf", Payload::Json(args.clone()))
            .expect("export")
        else {
            panic!("binary expected");
        };
        pdfs.push(pdf);
    }
    assert_eq!(before, list(), "aux files leaked into the project");
    assert_eq!(pdfs[0], pdfs[1], "same input must give the same PDF");
}

#[test]
fn runaway_documents_are_killed_at_the_timeout() {
    let Some(k) = engine_kernel() else { return };
    let src = json!({"main.tex": "\\documentclass{article}\n\\begin{document}\n\
        \\def\\x{\\ifnum1=1 \\expandafter\\x\\fi}\\x\n\\end{document}\n"});
    let started = Instant::now();
    let err = k
        .invoke(
            "latex.compile",
            Payload::Json(json!({"main": "main.tex", "sources": src, "timeout_ms": 2500})),
        )
        .expect_err("must time out");
    assert!(err.to_string().contains("[timeout]"), "{err}");
    assert!(started.elapsed() < Duration::from_secs(30));
}

#[test]
fn missing_bundle_without_network_fails_fast_and_clearly() {
    if !worker_built() {
        return;
    }
    let empty = tempfile::tempdir().expect("tempdir");
    let k = kernel_with(LatexConfig {
        allow_network: false,
        timeout: Duration::from_secs(20),
        ..config(Some(empty.path().to_path_buf()))
    });
    let started = Instant::now();
    let err = k
        .invoke(
            "latex.compile",
            Payload::Json(json!({"main": "main.tex", "sources": sources()})),
        )
        .expect_err("no bundle");
    assert!(started.elapsed() < Duration::from_secs(15), "hung");
    let message = err.to_string();
    assert!(
        message.contains("[bundle_unavailable]") || message.contains("[compile_failed]"),
        "{message}"
    );
}

#[test]
fn bad_arguments_are_structured_errors() {
    let k = kernel_with(config(Some(std::env::temp_dir())));
    for args in [
        json!({}),
        json!({"main": "m.tex", "sources": {"../evil.tex": "x", "m.tex": "y"}}),
        json!({"main": "m.tex", "sources": {"/abs.tex": "x", "m.tex": "y"}}),
        json!({"main": "m.tex", "sources": {"other.tex": "x"}}),
        json!({"main": "m.tex", "timeout_ms": 0}),
    ] {
        let err = k
            .invoke("latex.compile", Payload::Json(args.clone()))
            .expect_err("rejected");
        assert!(
            matches!(&err, KernelError::Plugin(PluginError::Failed(m)) if m.starts_with('[')),
            "{args}: {err}"
        );
    }
}

#[test]
fn synctex_without_a_build_is_a_structured_error() {
    let k = kernel_with(config(Some(std::env::temp_dir())));
    let err = k
        .invoke(
            "latex.synctex",
            Payload::Json(json!({"file": "a.tex", "line": 1})),
        )
        .expect_err("no build");
    assert!(err.to_string().contains("[no_build]"), "{err}");
}

#[test]
fn on_lint_hook_parses_logs_without_the_engine() {
    let k = kernel_with(config(Some(std::env::temp_dir())));
    let log = "(./main.tex\n! Undefined control sequence.\nl.3 \\foo\n\n)";
    let out = k
        .run_hook(Hook::OnLint, Payload::Json(json!({"log": log})))
        .expect("hook");
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].0, "latex");
    let v = json_of(out[0].1.clone());
    assert_eq!(v["diagnostics"][0]["line"], 3);
    // Unrelated hooks pass through untouched.
    let pre = k
        .run_hook(Hook::OnCompilePre, Payload::Json(json!(1)))
        .expect("pre");
    assert!(pre.is_empty() || pre[0].1 == Payload::Json(json!(1)));
}
