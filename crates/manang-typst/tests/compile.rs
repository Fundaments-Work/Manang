//! End-to-end behaviour of the compiler against real (temporary) projects.
//! Only embedded fonts are used so snapshots are machine independent.

use std::collections::BTreeMap;
use std::fs;
use std::sync::Arc;

use manang_typst::{
    CompileRequest, Compiler, CompilerConfig, DiagSeverity, Environment, TypstError,
};
use tempfile::TempDir;

fn compiler() -> Compiler {
    Compiler::new(CompilerConfig {
        env: Environment::default(),
        root: None,
        threads: 2,
    })
    .expect("pool starts")
}

fn project(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (name, text) in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("mkdir");
        }
        fs::write(path, text).expect("write fixture");
    }
    dir
}

fn request(dir: &TempDir, main: &str, sources: &[(&str, &str)]) -> CompileRequest {
    CompileRequest {
        main: main.to_owned(),
        root: Some(dir.path().to_path_buf()),
        sources: sources
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect::<BTreeMap<_, _>>(),
    }
}

const HELLO: &str = "#set page(width: 200pt, height: 100pt, margin: 10pt)\n= Hello\nWorld.";

#[test]
fn compiles_a_project_from_disk() {
    let dir = project(&[("main.typ", HELLO)]);
    let report = compiler()
        .compile(&request(&dir, "main.typ", &[]))
        .expect("compile");
    assert!(report.ok, "{:?}", report.diagnostics);
    assert_eq!(report.pages, 1);
    assert_eq!(report.page_sizes, vec![[200.0, 100.0]]);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn in_memory_source_overrides_disk_and_may_not_exist_on_disk() {
    let dir = project(&[("main.typ", "#include \"part.typ\"")]);
    let c = compiler();
    let report = c
        .compile(&request(
            &dir,
            "main.typ",
            &[("part.typ", "#set page(width: 120pt, height: 80pt)\nunsaved")],
        ))
        .expect("compile");
    assert!(report.ok, "{:?}", report.diagnostics);
    assert_eq!(report.page_sizes, vec![[120.0, 80.0]]);

    // Dropping the override falls back to disk, where the file is missing.
    let report = c.compile(&request(&dir, "main.typ", &[])).expect("compile");
    assert!(!report.ok);
}

#[test]
fn unsaved_main_beats_disk_text() {
    let dir = project(&[("main.typ", "#panic(\"stale\")")]);
    let report = compiler()
        .compile(&request(&dir, "main.typ", &[("main.typ", HELLO)]))
        .expect("compile");
    assert!(report.ok, "{:?}", report.diagnostics);
}

#[test]
fn absolute_paths_inside_the_root_resolve() {
    let dir = project(&[("main.typ", HELLO)]);
    let abs = dir.path().join("main.typ");
    let mut req = request(&dir, abs.to_str().expect("utf8"), &[]);
    req.root = None;
    let report = compiler().compile(&req).expect("compile");
    assert!(report.ok, "{:?}", report.diagnostics);
}

#[test]
fn paths_outside_the_root_are_rejected() {
    let dir = project(&[("main.typ", HELLO)]);
    let other = project(&[("x.typ", "x")]);
    let outside = other.path().join("x.typ");
    let result = compiler().compile(&request(&dir, outside.to_str().expect("utf8"), &[]));
    assert!(
        matches!(result, Err(TypstError::OutsideRoot(_))),
        "{result:?}"
    );
    let escape = compiler().compile(&request(&dir, "../x.typ", &[]));
    assert!(escape.is_err() || !escape.expect("report").ok);
}

#[test]
fn relative_main_without_a_root_is_an_error() {
    let req = CompileRequest {
        main: "main.typ".to_owned(),
        ..CompileRequest::default()
    };
    assert!(matches!(compiler().compile(&req), Err(TypstError::NoRoot)));
}

#[test]
fn diagnostics_are_structured_with_character_offsets() {
    // The leading emoji and umlaut make byte and char offsets differ.
    let source = "😀 ü\n#let x = 1\n#undefined-thing";
    let dir = project(&[("main.typ", "")]);
    let report = compiler()
        .compile(&request(&dir, "main.typ", &[("main.typ", source)]))
        .expect("compile");
    assert!(!report.ok);
    let d = &report.diagnostics[0];
    assert_eq!(d.severity, DiagSeverity::Error);
    assert_eq!(d.file.as_deref(), Some("/main.typ"));
    assert_eq!(d.line, Some(3));
    assert_eq!(d.column, Some(2));
    let range = d.range.expect("range");
    let chars: Vec<char> = source.chars().collect();
    let slice: String = chars[range.start..range.end].iter().collect();
    assert_eq!(slice, "undefined-thing");
    insta::assert_json_snapshot!("unknown_variable", report.diagnostics);
}

#[test]
fn warnings_and_hints_are_reported_on_success() {
    let dir = project(&[("main.typ", "")]);
    let src = "#set page(width: 100pt, height: 60pt)\n#set text(font: \"No Such Font Family\")\nhi";
    let report = compiler()
        .compile(&request(&dir, "main.typ", &[("main.typ", src)]))
        .expect("compile");
    assert!(report.ok);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.severity == DiagSeverity::Warning),
        "{:?}",
        report.diagnostics
    );
    insta::assert_json_snapshot!("missing_font_warning", report.diagnostics);
}

#[test]
fn errors_in_imported_files_name_that_file() {
    let dir = project(&[
        ("main.typ", "#import \"lib.typ\": f\n#f()"),
        ("lib.typ", "#let f() = 1 + \"a\""),
    ]);
    let report = compiler()
        .compile(&request(&dir, "main.typ", &[]))
        .expect("compile");
    assert!(!report.ok);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.file.as_deref() == Some("/lib.typ")),
        "{:?}",
        report.diagnostics
    );
    insta::assert_json_snapshot!("error_in_import", report.diagnostics);
}

#[test]
fn render_and_export_need_a_compile_first() {
    let c = compiler();
    assert!(matches!(c.render(0), Err(TypstError::NoDocument)));
    assert!(matches!(c.export_pdf(), Err(TypstError::NoDocument)));
}

#[test]
fn render_returns_cached_svg_and_checks_the_page_range() {
    let dir = project(&[("main.typ", HELLO)]);
    let c = compiler();
    c.compile(&request(&dir, "main.typ", &[])).expect("compile");
    let first = c.render(0).expect("render");
    let second = c.render(0).expect("render");
    assert!(
        Arc::ptr_eq(&first, &second),
        "second render reads the cache"
    );
    let svg = std::str::from_utf8(&first).expect("svg is utf-8");
    assert!(svg.starts_with("<svg"));
    assert!(matches!(
        c.render(1),
        Err(TypstError::PageOutOfRange { page: 1, pages: 1 })
    ));
    insta::assert_snapshot!("hello_page_svg", svg);
}

#[test]
fn pdf_export_is_cached_and_well_formed() {
    let dir = project(&[("main.typ", HELLO)]);
    let c = compiler();
    c.compile(&request(&dir, "main.typ", &[])).expect("compile");
    let a = c.export_pdf().expect("pdf");
    assert!(a.starts_with(b"%PDF-"));
    let b = c.export_pdf().expect("pdf");
    assert!(Arc::ptr_eq(&a, &b));
}

#[test]
fn a_failed_compile_keeps_the_last_good_document() {
    let dir = project(&[("main.typ", HELLO)]);
    let c = compiler();
    let good = c.compile(&request(&dir, "main.typ", &[])).expect("compile");
    let bad = c
        .compile(&request(&dir, "main.typ", &[("main.typ", "#panic(\"x\")")]))
        .expect("compile");
    assert!(!bad.ok);
    assert_eq!(c.generation(), Some(good.generation));
    assert!(c.render(0).is_ok());
    assert_eq!(c.diagnostics().len(), bad.diagnostics.len());
}

#[test]
fn generations_increase_and_edits_recompile() {
    let dir = project(&[("main.typ", HELLO)]);
    let c = compiler();
    let a = c.compile(&request(&dir, "main.typ", &[])).expect("compile");
    let b = c
        .compile(&request(
            &dir,
            "main.typ",
            &[("main.typ", "#set page(width: 50pt, height: 50pt)\nx")],
        ))
        .expect("compile");
    assert!(b.generation > a.generation);
    assert_eq!(b.page_sizes, vec![[50.0, 50.0]]);
}

#[test]
fn today_is_available_to_documents() {
    let dir = project(&[("main.typ", "")]);
    let report = compiler()
        .compile(&request(
            &dir,
            "main.typ",
            &[("main.typ", "#assert(datetime.today().year() >= 2024)")],
        ))
        .expect("compile");
    assert!(report.ok, "{:?}", report.diagnostics);
}

#[test]
fn too_many_sources_are_refused() {
    let dir = project(&[("main.typ", HELLO)]);
    let mut req = request(&dir, "main.typ", &[]);
    for i in 0..=manang_typst::MAX_SOURCES {
        req.sources.insert(format!("f{i}.typ"), String::new());
    }
    assert!(matches!(
        compiler().compile(&req),
        Err(TypstError::TooManySources(..))
    ));
}
