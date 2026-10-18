//! Integration tests verifying compilation, diagnostics, and timing
//! against the realistic projects in `testdata/corpus/`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use manang_typst::{CompileRequest, Compiler, CompilerConfig, DiagSeverity, Environment};

#[derive(Debug, Clone)]
struct ExpectedConfig {
    engine: String,
    main: String,
    compile: bool,
    pages: Option<usize>,
    diagnostic_lines: Option<Vec<usize>>,
}

fn parse_expected_toml(source: &str) -> ExpectedConfig {
    let mut engine = String::new();
    let mut main = String::new();
    let mut compile = true;
    let mut pages = None;
    let mut diagnostic_lines = None;

    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((key, val)) = line.split_once('=') {
            let key = key.trim();
            let val = val.trim();

            match key {
                "engine" => {
                    engine = val.trim_matches('"').trim_matches('\'').to_string();
                }
                "main" => {
                    main = val.trim_matches('"').trim_matches('\'').to_string();
                }
                "compile" => {
                    compile = val.parse::<bool>().unwrap_or(true);
                }
                "pages" => {
                    if let Ok(p) = val.parse::<usize>() {
                        pages = Some(p);
                    }
                }
                "diagnostic_lines" => {
                    let inner = val.trim_matches(|c| c == '[' || c == ']');
                    let lines: Vec<usize> = inner
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .filter_map(|s| s.parse::<usize>().ok())
                        .collect();
                    diagnostic_lines = Some(lines);
                }
                _ => {}
            }
        }
    }

    ExpectedConfig {
        engine,
        main,
        compile,
        pages,
        diagnostic_lines,
    }
}

fn parse_manang_toml_engine(source: &str) -> Option<String> {
    let mut in_project = false;
    for line in source.lines() {
        let line = line.trim();
        if line == "[project]" {
            in_project = true;
            continue;
        }
        if line.starts_with('[') {
            in_project = false;
            continue;
        }
        if in_project
            && line.starts_with("engine")
            && let Some((_, val)) = line.split_once('=')
        {
            return Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
        }
    }
    None
}

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/corpus")
        .canonicalize()
        .expect("canonicalize testdata/corpus path")
}

fn create_compiler() -> Compiler {
    Compiler::new(CompilerConfig {
        env: Environment::default(),
        root: None,
        threads: 2,
    })
    .expect("typst compiler starts")
}

#[test]
fn test_typst_corpus_compilation_and_diagnostics() {
    let corpus = corpus_dir();
    let compiler = create_compiler();

    let typst_projects = [
        "typst-article",
        "typst-thesis",
        "typst-slides",
        "typst-poster",
        "typst-letter",
        "typst-heavy-math",
        "typst-cjk-accented",
        "typst-broken",
    ];

    println!("\n=== Typst Corpus Compile & Timing Report ===");

    for project_name in typst_projects {
        let project_path = corpus.join(project_name);
        assert!(
            project_path.is_dir(),
            "Corpus project folder missing: {:?}",
            project_path
        );

        let expected_path = project_path.join("expected.toml");
        assert!(
            expected_path.is_file(),
            "expected.toml missing for {:?}",
            project_path
        );

        let expected_raw = fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("failed to read {:?}: {}", expected_path, e));
        let expected = parse_expected_toml(&expected_raw);

        assert_eq!(
            expected.engine, "typst",
            "Project {:?} expected.toml engine must be 'typst'",
            project_name
        );

        let main_file = project_path.join(&expected.main);
        assert!(
            main_file.is_file(),
            "Main file {:?} missing for {:?}",
            main_file,
            project_name
        );

        let req = CompileRequest {
            main: expected.main.clone(),
            root: Some(project_path.clone()),
            sources: BTreeMap::new(),
        };

        let start = Instant::now();
        let report = compiler
            .compile(&req)
            .unwrap_or_else(|e| panic!("compiler error on {:?}: {}", project_name, e));
        let elapsed = start.elapsed();

        println!(
            "  [{:18}] compile={:5} pages={:2} elapsed={:6.2} ms",
            project_name,
            report.ok,
            report.pages,
            elapsed.as_secs_f64() * 1000.0,
        );

        // Enforce performance SLA: no project takes over 5 seconds
        assert!(
            elapsed.as_secs() < 5,
            "Compilation of {:?} exceeded 5 second budget: {:?}",
            project_name,
            elapsed
        );

        if expected.compile {
            assert!(
                report.ok,
                "Compilation failed for {:?}. Diagnostics: {:#?}",
                project_name, report.diagnostics
            );

            if let Some(expected_pages) = expected.pages {
                assert_eq!(
                    report.pages, expected_pages,
                    "Page count mismatch for {:?}",
                    project_name
                );
            }
        } else {
            // Broken projects must fail gracefully with diagnostics, never panic
            assert!(
                !report.ok,
                "Compilation unexpectedly succeeded for broken project {:?}",
                project_name
            );

            assert!(
                !report.diagnostics.is_empty(),
                "Expected diagnostics for broken project {:?}, but none produced",
                project_name
            );

            for diag in &report.diagnostics {
                assert!(
                    diag.line.is_some() && diag.column.is_some(),
                    "Diagnostic must have line and column: {:?}",
                    diag
                );
                assert_eq!(
                    diag.severity,
                    DiagSeverity::Error,
                    "Expected error severity: {:?}",
                    diag
                );
            }

            if let Some(ref expected_lines) = expected.diagnostic_lines {
                let actual_lines: Vec<usize> =
                    report.diagnostics.iter().filter_map(|d| d.line).collect();

                for line in expected_lines {
                    assert!(
                        actual_lines.contains(line),
                        "Expected diagnostic at line {} for {:?}, found lines: {:?}",
                        line,
                        project_name,
                        actual_lines
                    );
                }
            }
        }
    }

    println!("============================================\n");
}

#[test]
fn test_latex_and_markdown_corpus_projects_exist_and_validate() {
    let corpus = corpus_dir();

    let non_typst_projects = [
        ("latex-article", "latex"),
        ("latex-report", "latex"),
        ("latex-broken", "latex"),
        ("markdown-notes", "markdown"),
    ];

    for (project_name, expected_engine) in non_typst_projects {
        let project_path = corpus.join(project_name);
        assert!(
            project_path.is_dir(),
            "Corpus project folder missing: {:?}",
            project_path
        );

        let manang_toml_path = project_path.join("manang.toml");
        assert!(
            manang_toml_path.is_file(),
            "manang.toml missing for {:?}",
            project_name
        );
        let manang_raw = fs::read_to_string(&manang_toml_path)
            .unwrap_or_else(|e| panic!("failed to read {:?}: {}", manang_toml_path, e));
        let engine =
            parse_manang_toml_engine(&manang_raw).expect("manang.toml must have [project].engine");
        assert_eq!(
            engine, expected_engine,
            "Engine in manang.toml mismatch for {:?}",
            project_name
        );

        let expected_path = project_path.join("expected.toml");
        assert!(
            expected_path.is_file(),
            "expected.toml missing for {:?}",
            project_path
        );

        let expected_raw = fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("failed to read {:?}: {}", expected_path, e));
        let expected = parse_expected_toml(&expected_raw);

        assert_eq!(
            expected.engine, expected_engine,
            "expected.toml engine mismatch for {:?}",
            project_name
        );

        let main_file = project_path.join(&expected.main);
        assert!(
            main_file.is_file(),
            "Main file {:?} referenced in expected.toml does not exist for {:?}",
            main_file,
            project_name
        );
    }
}
