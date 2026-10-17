//! Snapshot tests over real logs produced by Tectonic (XeTeX) and pdfTeX.

use manang_latex::diagnostics::{parse_bibtex_log, parse_tex_log};
use proptest::prelude::*;

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

macro_rules! tex_snapshot {
    ($test:ident, $file:literal) => {
        #[test]
        fn $test() {
            let diagnostics = parse_tex_log(&fixture($file), "main.tex");
            insta::assert_json_snapshot!(diagnostics);
        }
    };
}

tex_snapshot!(undefined_control_sequence, "undefined_cs.log");
tex_snapshot!(overfull_and_underfull_boxes, "boxes.log");
tex_snapshot!(undefined_references, "refs.log");
tex_snapshot!(missing_package, "missing_pkg.log");
tex_snapshot!(package_and_latex_warnings, "package_warning.log");
tex_snapshot!(missing_input_file, "missing_input.log");
tex_snapshot!(missing_dollar, "math_error.log");
tex_snapshot!(runaway_argument, "runaway.log");
tex_snapshot!(error_inside_input_file, "multi_file.log");
tex_snapshot!(pdftex_nonstop_mode, "pdftex_nonstop.log");
tex_snapshot!(clean_bibtex_pass_log, "bib_ok.log");

#[test]
fn bibtex_missing_database() {
    insta::assert_json_snapshot!(parse_bibtex_log(&fixture("refs.blg"), "main.aux"));
}

#[test]
fn bibtex_clean_run_has_no_diagnostics() {
    assert!(parse_bibtex_log(&fixture("bib_ok.blg"), "main.aux").is_empty());
}

#[test]
fn file_line_error_style() {
    let log = "(./main.tex\n./main.tex:7: Undefined control sequence.\nl.7 \\foo\n)";
    insta::assert_json_snapshot!(parse_tex_log(log, "main.tex"));
}

proptest! {
    #[test]
    fn arbitrary_text_never_panics(text in any::<String>()) {
        let _ = parse_tex_log(&text, "main.tex");
        let _ = parse_bibtex_log(&text, "main.aux");
    }

    #[test]
    fn log_shaped_noise_never_panics(
        lines in proptest::collection::vec(
            prop_oneof![
                Just("! ".to_owned()),
                Just("l.".to_owned()),
                Just("(".to_owned()),
                Just(")".to_owned()),
                Just("Overfull \\hbox ".to_owned()),
                Just("LaTeX Warning: ".to_owned()),
                Just("Package x Warning: ".to_owned()),
                Just("on input line ".to_owned()),
                Just("Warning--".to_owned()),
                Just("---line ".to_owned()),
                Just("é\u{1F600}".to_owned()),
                "[ -~]{0,100}",
                "[0-9]{0,12}",
            ],
            0..60,
        )
    ) {
        let text = lines.join("\n");
        let tex = parse_tex_log(&text, "main.tex");
        prop_assert!(tex.len() <= 2000);
        let joined: String = lines.concat();
        let _ = parse_tex_log(&joined, "main.tex");
        let _ = parse_bibtex_log(&text, "main.aux");
    }

    #[test]
    fn diagnostics_survive_wrapped_lines(width in 1usize..200) {
        let long = "x".repeat(width);
        let log = format!(
            "(main.tex\nOverfull \\hbox (1pt too wide) in paragraph at lines 3--4\n{long}\n\n)"
        );
        let d = parse_tex_log(&log, "main.tex");
        prop_assert_eq!(d.len(), 1);
        prop_assert_eq!(d[0].line, Some(3));
    }
}
