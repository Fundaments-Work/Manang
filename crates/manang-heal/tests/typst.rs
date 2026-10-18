//! Typst rules, with diagnostics from the real compiler.

mod common;

use std::collections::BTreeMap;

use common::{applied, run, show};
use manang_heal::{Confidence, DiagnosticIn, Engine};
use manang_typst::{CompileRequest, Compiler, CompilerConfig, Environment};
use typst::LibraryExt as _;

/// Compile `text` as `main.typ` in an empty project and return the diagnostics
/// in the heal request shape, plus the error count.
fn compile(text: &str) -> (Vec<DiagnosticIn>, usize) {
    let compiler = Compiler::new(CompilerConfig {
        env: Environment::default(),
        root: None,
        threads: 1,
    })
    .expect("pool starts");
    let dir = tempfile::tempdir().expect("tempdir");
    let report = compiler
        .compile(&CompileRequest {
            main: "main.typ".to_owned(),
            root: Some(dir.path().to_path_buf()),
            sources: BTreeMap::from([("main.typ".to_owned(), text.to_owned())]),
        })
        .expect("compiles");
    let diags: Vec<DiagnosticIn> = report
        .diagnostics
        .iter()
        .map(|d| {
            serde_json::from_value(serde_json::to_value(d).expect("serialises")).expect("shape")
        })
        .collect();
    let errors = diags.iter().filter(|d| d.is_error()).count();
    (diags, errors)
}

/// The first fix, applied, must compile with fewer errors.
fn heals(text: &str) -> String {
    let (diags, before) = compile(text);
    assert!(before > 0, "the sample must be broken: {text:?}");
    let s = run(Engine::Typst, text, &diags);
    let fix = s
        .fixes
        .first()
        .unwrap_or_else(|| panic!("no fix for {text:?}: {diags:?}"));
    let fixed = applied(text, fix);
    let (_, after) = compile(&fixed);
    assert!(
        after < before,
        "{text:?} -> {fixed:?}: {before} -> {after} errors"
    );
    fixed
}

#[test]
fn misspelled_keyword_is_corrected() {
    let fixed = heals("#sett text(size: 11pt)\nhello\n");
    assert_eq!(fixed, "#set text(size: 11pt)\nhello\n");
    let (diags, _) = compile("#sett text(size: 11pt)\nhello\n");
    insta::assert_json_snapshot!(
        "typst_misspelled",
        show(Engine::Typst, "#sett text(size: 11pt)\nhello\n", &diags)
    );
}

#[test]
fn swapped_letters_in_show() {
    let fixed = heals("#shwo heading: it => it\n= A\n");
    assert_eq!(fixed, "#show heading: it => it\n= A\n");
}

#[test]
fn misspelled_user_function() {
    let text = "#let myfunc(x) = x\n#myfnc(1)\n";
    assert_eq!(heals(text), "#let myfunc(x) = x\n#myfunc(1)\n");
}

#[test]
fn two_letters_off_is_not_guessed() {
    let text = "#stlye(1)\n";
    let (diags, _) = compile(text);
    assert!(run(Engine::Typst, text, &diags).fixes.is_empty());
}

#[test]
fn unclosed_call_closes_where_the_statement_ends() {
    assert_eq!(
        heals("= Title\n#lorem(10\n\ntext\n"),
        "= Title\n#lorem(10)\n\ntext\n"
    );
}

#[test]
fn unclosed_multiline_call_closes_after_its_body() {
    let text = "#figure(\n  rect(width: 3em),\n  caption: [x],\n\nafter\n";
    assert_eq!(
        heals(text),
        "#figure(\n  rect(width: 3em),\n  caption: [x],)\n\nafter\n"
    );
    let (diags, _) = compile(text);
    insta::assert_json_snapshot!("typst_unclosed_call", show(Engine::Typst, text, &diags));
}

#[test]
fn unclosed_content_block_and_nested_openers_close_together() {
    assert_eq!(heals("#let a = [x\nmore\n"), "#let a = [x\nmore]\n");
    let fixed = heals("#let a = f(g([x\n");
    assert_eq!(fixed, "#let a = f(g([x]))\n");
}

#[test]
fn unclosed_string_math_and_raw() {
    assert_eq!(heals("#let x = \"abc\n"), "#let x = \"abc\"\n");
    assert_eq!(heals("$ x + y\n"), "$ x + y $\n");
    assert_eq!(
        heals("```rust\nfn main() {}\n"),
        "```rust\nfn main() {}\n```\n"
    );
}

#[test]
fn stray_bracket_can_be_removed_or_escaped() {
    let text = "a ] b\n";
    let (diags, _) = compile(text);
    let s = run(Engine::Typst, text, &diags);
    assert_eq!(s.fixes.len(), 2);
    assert_eq!(applied(text, &s.fixes[0]), "a  b\n");
    assert_eq!(applied(text, &s.fixes[1]), "a \\] b\n");
    assert!(s.fixes.iter().all(|f| f.confidence == Confidence::Likely));
}

// The package fixes cannot be compiled here (no network in tests), so they
// are checked against the diagnostics the compiler really gives.

#[test]
fn cetz_namespace_and_names_are_imported() {
    let text = "#cetz.canvas({})\n";
    let (diags, _) = compile(text);
    let s = run(Engine::Typst, text, &diags);
    assert_eq!(s.fixes.len(), 1);
    assert_eq!(s.fixes[0].id, "typst.import:cetz@0.5.2:cetz");
    assert_eq!(s.fixes[0].confidence, Confidence::Certain);
    assert_eq!(
        applied(text, &s.fixes[0]),
        "#import \"@preview/cetz:0.5.2\"\n#cetz.canvas({})\n"
    );

    let text = "// A figure\n#canvas({ draw.line((0, 0), (1, 1)) })\n";
    let (diags, _) = compile(text);
    let s = run(Engine::Typst, text, &diags);
    assert_eq!(s.fixes.len(), 1, "{diags:?}");
    assert_eq!(
        applied(text, &s.fixes[0]),
        "// A figure\n#import \"@preview/cetz:0.5.2\": canvas, draw\n#canvas({ draw.line((0, 0), (1, 1)) })\n"
    );
    insta::assert_json_snapshot!("typst_cetz", show(Engine::Typst, text, &diags));
}

#[test]
fn several_names_of_one_package_make_one_import() {
    let text = "#diagram(node((0, 0), [A]), edge((0, 0), (1, 0)))\n";
    let (diags, _) = compile(text);
    let s = run(Engine::Typst, text, &diags);
    assert_eq!(s.fixes.len(), 1);
    assert!(
        applied(text, &s.fixes[0])
            .starts_with("#import \"@preview/fletcher:0.5.8\": diagram, node, edge\n")
    );
    assert_eq!(
        s.fixes[0].diagnostics.len(),
        1,
        "Typst reports only the first unknown name of a call"
    );
}

#[test]
fn an_existing_import_list_is_extended() {
    let text = "#import \"@preview/cetz:0.5.2\": draw // shapes\n#canvas({ })\n";
    let (diags, _) = compile(text);
    // The package is not in the test's cache: the import itself fails too,
    // but the unknown `canvas` is still reported when the module resolves; the
    // request below is what the shell sends after a successful resolve.
    let unknown = DiagnosticIn {
        severity: Some("error".into()),
        message: "unknown variable: canvas".into(),
        range: Some(manang_heal::ScalarRange { start: 47, end: 53 }),
        ..Default::default()
    };
    let _ = diags;
    let s = run(Engine::Typst, text, &[unknown]);
    assert_eq!(s.fixes.len(), 1);
    assert_eq!(
        applied(text, &s.fixes[0]),
        "#import \"@preview/cetz:0.5.2\": draw, canvas // shapes\n#canvas({ })\n"
    );
}

#[test]
fn a_wildcard_import_is_not_extended_and_defined_names_are_left_alone() {
    let text = "#import \"@preview/cetz:0.5.2\": *\n#canvas({})\n";
    let unknown = DiagnosticIn {
        severity: Some("error".into()),
        message: "unknown variable: canvas".into(),
        range: Some(manang_heal::ScalarRange { start: 34, end: 40 }),
        ..Default::default()
    };
    assert!(run(Engine::Typst, text, &[unknown]).fixes.is_empty());
    let text = "#let canvas(x) = x\n#canvaas(1)\n";
    let (diags, _) = compile(text);
    assert_eq!(
        run(Engine::Typst, text, &diags).fixes.len(),
        1,
        "typo of the user's own name"
    );
}

#[test]
fn touying_theme_needs_two_lines() {
    let text = "#show: simple-theme.with(aspect-ratio: \"16-9\")\n= A\n";
    let (diags, _) = compile(text);
    let s = run(Engine::Typst, text, &diags);
    assert_eq!(s.fixes.len(), 1, "{diags:?}");
    assert_eq!(
        applied(text, &s.fixes[0]),
        "#import \"@preview/touying:0.8.0\": *\n#import themes.simple: *\n#show: simple-theme.with(aspect-ratio: \"16-9\")\n= A\n"
    );
}

#[test]
fn math_functions_from_physica() {
    let text = "$ ket(psi) $\n";
    let (diags, _) = compile(text);
    let s = run(Engine::Typst, text, &diags);
    assert_eq!(s.fixes.len(), 1);
    assert_eq!(s.fixes[0].label, "Import ket from @preview/physica:0.9.8");
}

#[test]
fn every_known_function_name_exists_in_the_standard_library() {
    use manang_heal::typst::names::{FUNCTIONS, KEYWORDS};
    let library = typst::Library::default();
    for name in FUNCTIONS {
        let known =
            library.global.scope().get(name).is_some() || library.math.scope().get(name).is_some();
        assert!(known, "`{name}` is not in the Typst standard library");
    }
    assert!(KEYWORDS.contains(&"set"));
}
