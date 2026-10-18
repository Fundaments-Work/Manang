//! LaTeX rules: one test per rule, with snapshots of the whole suggestion.

mod common;

use common::{applied, latex_diag, run, show};
use manang_heal::{Confidence, Engine};

const ARTICLE: &str = "\\documentclass{article}\n\\usepackage[utf8]{inputenc}\n\\begin{document}\n\\includegraphics[width=3cm]{a.png}\n\\end{document}\n";

#[test]
fn missing_graphicx_is_added_after_the_last_package() {
    let diags = [latex_diag(
        "Undefined control sequence: \\includegraphics",
        4,
    )];
    let s = run(Engine::Latex, ARTICLE, &diags);
    assert_eq!(s.fixes.len(), 1);
    let fix = &s.fixes[0];
    assert_eq!(fix.id, "latex.package:graphicx");
    assert_eq!(fix.label, "Add \\usepackage{graphicx}");
    assert_eq!(fix.confidence, Confidence::Certain);
    assert_eq!(fix.rule, "latex.missing-package");
    assert_eq!(
        applied(ARTICLE, fix),
        "\\documentclass{article}\n\\usepackage[utf8]{inputenc}\n\\usepackage{graphicx}\n\\begin{document}\n\\includegraphics[width=3cm]{a.png}\n\\end{document}\n"
    );
    insta::assert_json_snapshot!("graphicx", show(Engine::Latex, ARTICLE, &diags));
}

#[test]
fn without_a_token_the_line_names_the_command() {
    // Tectonic prints `! Undefined control sequence.` and the source line.
    let diags = [latex_diag("Undefined control sequence", 4)];
    let s = run(Engine::Latex, ARTICLE, &diags);
    assert_eq!(s.fixes.len(), 1);
    assert_eq!(s.fixes[0].id, "latex.package:graphicx");
    assert_eq!(s.fixes[0].confidence, Confidence::Certain);
}

#[test]
fn package_goes_after_documentclass_when_there_are_none() {
    let text = "\\documentclass[11pt]{article} % base\n\\begin{document}\n\\textcolor{red}{x}\n\\end{document}\n";
    let diags = [latex_diag("Undefined control sequence: \\textcolor", 3)];
    let s = run(Engine::Latex, text, &diags);
    assert_eq!(
        applied(text, &s.fixes[0]),
        "\\documentclass[11pt]{article} % base\n\\usepackage{xcolor}\n\\begin{document}\n\\textcolor{red}{x}\n\\end{document}\n"
    );
    insta::assert_json_snapshot!("after_documentclass", show(Engine::Latex, text, &diags));
}

#[test]
fn a_loaded_package_is_never_added_twice() {
    let text = "\\documentclass{article}\n\\usepackage{tikz,\n  graphicx}\n\\begin{document}\n\\includegraphics{a}\n\\end{document}\n";
    let diags = [latex_diag(
        "Undefined control sequence: \\includegraphics",
        5,
    )];
    assert!(run(Engine::Latex, text, &diags).fixes.is_empty());
    // A class that loads it counts too.
    let text = "\\documentclass{amsart}\n\\begin{document}\n$\\text{a}$\n\\end{document}\n";
    let diags = [latex_diag("Undefined control sequence: \\text", 3)];
    assert!(run(Engine::Latex, text, &diags).fixes.is_empty());
}

#[test]
fn a_file_without_preamble_gets_no_fix() {
    let text = "\\section{A}\n\\includegraphics{a}\n";
    let diags = [latex_diag(
        "Undefined control sequence: \\includegraphics",
        2,
    )];
    assert!(run(Engine::Latex, text, &diags).fixes.is_empty());
}

#[test]
fn a_user_definition_is_not_a_missing_package() {
    let text = "\\documentclass{article}\n\\newcommand{\\url}[1]{#1}\n\\begin{document}\n\\url{x}\n\\end{document}\n";
    let diags = [latex_diag("Undefined control sequence: \\url", 4)];
    assert!(run(Engine::Latex, text, &diags).fixes.is_empty());
}

#[test]
fn commented_out_packages_do_not_count_as_loaded() {
    let text = "\\documentclass{article}\n% \\usepackage{graphicx}\n\\begin{document}\n\\includegraphics{a}\n\\end{document}\n";
    let diags = [latex_diag(
        "Undefined control sequence: \\includegraphics",
        4,
    )];
    let s = run(Engine::Latex, text, &diags);
    assert_eq!(s.fixes.len(), 1);
    assert!(applied(text, &s.fixes[0]).contains("\n\\usepackage{graphicx}\n"));
}

#[test]
fn two_packages_for_one_command_are_two_likely_fixes() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\url{x}\n\\end{document}\n";
    let diags = [latex_diag("Undefined control sequence: \\url", 3)];
    let s = run(Engine::Latex, text, &diags);
    let ids: Vec<&str> = s.fixes.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids, ["latex.package:hyperref", "latex.package:url"]);
    assert!(s.fixes.iter().all(|f| f.confidence == Confidence::Likely));
    assert!(s.fixes[0].why.contains("Another package"));
    assert!(s.fix_all.is_none());
    insta::assert_json_snapshot!("two_packages", show(Engine::Latex, text, &diags));
}

#[test]
fn several_errors_make_a_fix_all() {
    let text = "\\documentclass{article}\n\\usepackage{amsmath}\n\\begin{document}\n\\includegraphics{a}\n\\toprule\n\\includegraphics{b}\n\\end{document}\n";
    let diags = [
        latex_diag("Undefined control sequence: \\includegraphics", 4),
        latex_diag("Undefined control sequence: \\toprule", 5),
        latex_diag("Undefined control sequence: \\includegraphics", 6),
    ];
    let s = run(Engine::Latex, text, &diags);
    assert_eq!(s.fixes.len(), 2, "graphicx is one fix for two errors");
    assert_eq!(s.fixes[0].diagnostics, vec![0, 2]);
    let all = s.fix_all.expect("fix all");
    assert_eq!(all.fixes.len(), 2);
    let out = manang_heal::apply_edits(text, &all.edits);
    assert!(
        out.contains("\\usepackage{amsmath}\n\\usepackage{graphicx}\n\\usepackage{booktabs}\n")
    );
    insta::assert_json_snapshot!("fix_all", show(Engine::Latex, text, &diags));
}

#[test]
fn environment_from_a_missing_package() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\begin{align}\na&=b\n\\end{align}\n\\end{document}\n";
    let diags = [latex_diag("LaTeX Error: Environment align undefined", 3)];
    let s = run(Engine::Latex, text, &diags);
    assert_eq!(s.fixes.len(), 1);
    assert_eq!(s.fixes[0].id, "latex.package:amsmath");
    assert_eq!(s.fixes[0].rule, "latex.missing-environment-package");
    assert_eq!(s.fixes[0].confidence, Confidence::Certain);
    insta::assert_json_snapshot!("environment", show(Engine::Latex, text, &diags));
}

#[test]
fn tikzpicture_and_tikz_commands_share_one_fix() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\begin{tikzpicture}\n\\draw (0,0)--(1,1);\n\\end{tikzpicture}\n\\end{document}\n";
    let diags = [
        latex_diag("LaTeX Error: Environment tikzpicture undefined", 3),
        latex_diag("Undefined control sequence: \\draw", 4),
    ];
    let s = run(Engine::Latex, text, &diags);
    assert_eq!(s.fixes.len(), 1);
    assert_eq!(s.fixes[0].diagnostics, vec![0, 1]);
}

#[test]
fn theorem_needs_a_declaration() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\begin{theorem}\nx\n\\end{theorem}\n\\begin{lemma}y\\end{lemma}\n\\end{document}\n";
    let diags = [
        latex_diag("LaTeX Error: Environment theorem undefined", 3),
        latex_diag("LaTeX Error: Environment lemma undefined", 6),
    ];
    let s = run(Engine::Latex, text, &diags);
    assert_eq!(s.fixes.len(), 2);
    assert!(s.fixes.iter().all(|f| f.confidence == Confidence::Likely));
    assert!(
        applied(text, &s.fixes[0])
            .contains("\\usepackage{amsthm}\n\\newtheorem{theorem}{Theorem}\n")
    );
    insta::assert_json_snapshot!("theorem", show(Engine::Latex, text, &diags));
}

#[test]
fn tikz_library_for_positioning_and_arrows() {
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\begin{document}\n\\begin{tikzpicture}\n\\node (a) {A};\n\\node[right=of a] (b) {B};\n\\draw[-{Stealth}] (a)--(b);\n\\end{tikzpicture}\n\\end{document}\n";
    let diags = [latex_diag(
        "Package pgfkeys Error: I do not know the key '/tikz/right=of a' and I am going to ignore it",
        6,
    )];
    let s = run(Engine::Latex, text, &diags);
    assert_eq!(s.fixes.len(), 1);
    assert_eq!(s.fixes[0].id, "latex.tikzlibrary:arrows.meta,positioning");
    assert_eq!(
        applied(text, &s.fixes[0]),
        text.replace(
            "\\usepackage{tikz}\n",
            "\\usepackage{tikz}\n\\usetikzlibrary{arrows.meta,positioning}\n"
        )
    );
    insta::assert_json_snapshot!("tikz_library", show(Engine::Latex, text, &diags));
}

#[test]
fn tikz_library_is_not_added_when_already_named() {
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\usetikzlibrary{positioning}\n\\begin{document}\n\\begin{tikzpicture}\n\\node[right=of a] {B};\n\\end{tikzpicture}\n\\end{document}\n";
    let diags = [latex_diag(
        "Package pgfkeys Error: I do not know the key '/tikz/right=of a'",
        6,
    )];
    assert!(run(Engine::Latex, text, &diags).fixes.is_empty());
}

#[test]
fn usetikzlibrary_is_extended_after_an_existing_one() {
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\usetikzlibrary{calc}\n\\begin{document}\n\\begin{tikzpicture}\n\\node[right=of a] {B};\n\\end{tikzpicture}\n\\end{document}\n";
    let diags = [latex_diag(
        "Package pgfkeys Error: I do not know the key '/tikz/right=of a'",
        6,
    )];
    let s = run(Engine::Latex, text, &diags);
    assert!(
        applied(text, &s.fixes[0])
            .contains("\\usetikzlibrary{calc}\n\\usetikzlibrary{positioning}\n")
    );
}

#[test]
fn missing_end_at_the_end_of_the_document() {
    let text = "\\documentclass{article}\n\\begin{document}\nHello\n";
    let diags = [latex_diag("File ended while scanning use of \\@xargs", 3)];
    let s = run(Engine::Latex, text, &diags);
    assert_eq!(s.fixes.len(), 1);
    assert_eq!(s.fixes[0].confidence, Confidence::Certain);
    assert_eq!(
        applied(text, &s.fixes[0]),
        format!("{text}\\end{{document}}\n")
    );
}

#[test]
fn unclosed_environment_before_end_document() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\begin{itemize}\n\\item a\n\\end{document}\n";
    let diags = [latex_diag(
        "LaTeX Error: \\begin{itemize} on input line 3 ended by \\end{document}",
        5,
    )];
    let s = run(Engine::Latex, text, &diags);
    let insert = s
        .fixes
        .iter()
        .find(|f| f.id.starts_with("latex.end-insert"))
        .expect("insert fix");
    assert_eq!(
        applied(text, insert),
        "\\documentclass{article}\n\\begin{document}\n\\begin{itemize}\n\\item a\n\\end{itemize}\n\\end{document}\n"
    );
    assert_eq!(insert.confidence, Confidence::Likely);
    insta::assert_json_snapshot!("end_insert", show(Engine::Latex, text, &diags));
}

#[test]
fn mismatched_end_offers_both_readings() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\begin{itemize}\n\\item a\n\\end{enumerate}\n\\end{document}\n";
    let diags = [latex_diag(
        "LaTeX Error: \\begin{itemize} on input line 3 ended by \\end{enumerate}",
        5,
    )];
    let s = run(Engine::Latex, text, &diags);
    let ids: Vec<&str> = s.fixes.iter().map(|f| f.id.as_str()).collect();
    assert_eq!(ids.len(), 2, "{ids:?}");
    assert!(applied(text, &s.fixes[0]).contains("\\end{itemize}"));
    assert!(applied(text, &s.fixes[1]).contains("\\begin{enumerate}"));
}

#[test]
fn one_unclosed_brace_closes_at_the_end_of_its_line() {
    let text = "\\documentclass{article}\n\\begin{document}\nSome \\textbf{bold text % note\nmore\n\\end{document}\n";
    let diags = [latex_diag("Runaway argument", 3)];
    let s = run(Engine::Latex, text, &diags);
    assert_eq!(s.fixes.len(), 1);
    assert_eq!(s.fixes[0].confidence, Confidence::Likely);
    assert!(applied(text, &s.fixes[0]).contains("\\textbf{bold text} % note\n"));
    insta::assert_json_snapshot!("brace_open", show(Engine::Latex, text, &diags));
}

#[test]
fn one_extra_brace_is_removed() {
    let text = "\\documentclass{article}\n\\begin{document}\nx}\n\\end{document}\n";
    let diags = [latex_diag("Too many }'s", 3)];
    let s = run(Engine::Latex, text, &diags);
    assert_eq!(
        applied(text, &s.fixes[0]),
        "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n"
    );
}

#[test]
fn several_imbalances_are_left_alone() {
    let text =
        "\\documentclass{article}\n\\begin{document}\n\\textbf{a \\emph{b\n\\end{document}\n";
    let diags = [latex_diag("Runaway argument", 3)];
    assert!(run(Engine::Latex, text, &diags).fixes.is_empty());
}

#[test]
fn escaped_and_verbatim_braces_do_not_count() {
    let text = "\\documentclass{article}\n\\begin{document}\n\\{ \\verb|{| \\begin{verbatim}}}\\end{verbatim}\n\\end{document}\n";
    let diags = [latex_diag("Runaway argument", 3)];
    assert!(run(Engine::Latex, text, &diags).fixes.is_empty());
}

#[test]
fn offsets_are_utf16() {
    // An astral character before the insertion point is two UTF-16 units.
    let text = "% 😀\n\\documentclass{article}\n\\begin{document}\n\\includegraphics{a}\n\\end{document}\n";
    let diags = [latex_diag(
        "Undefined control sequence: \\includegraphics",
        4,
    )];
    let s = run(Engine::Latex, text, &diags);
    let edit = &s.fixes[0].edits[0];
    let line_end: usize = "% 😀\n\\documentclass{article}".encode_utf16().count();
    assert_eq!(edit.from, line_end);
    assert_eq!(edit.to, line_end);
    assert_eq!(edit.insert, "\n\\usepackage{graphicx}");
}

#[test]
fn crlf_documents_keep_their_line_endings() {
    let text = "\\documentclass{article}\r\n\\begin{document}\r\n\\includegraphics{a}\r\n\\end{document}\r\n";
    let diags = [latex_diag(
        "Undefined control sequence: \\includegraphics",
        3,
    )];
    let s = run(Engine::Latex, text, &diags);
    assert_eq!(
        applied(text, &s.fixes[0]),
        "\\documentclass{article}\r\n\\usepackage{graphicx}\r\n\\begin{document}\r\n\\includegraphics{a}\r\n\\end{document}\r\n"
    );
}

#[test]
fn warnings_and_unknown_messages_get_nothing() {
    let diags = [
        manang_heal::DiagnosticIn {
            severity: Some("warning".into()),
            message: "Undefined control sequence: \\includegraphics".into(),
            line: Some(4),
            ..Default::default()
        },
        latex_diag("Misplaced alignment tab character &", 4),
    ];
    assert!(run(Engine::Latex, ARTICLE, &diags).fixes.is_empty());
}
