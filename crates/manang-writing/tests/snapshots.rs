//! Snapshot tests: diagnostics, formatter output, symbol lookups, snippet
//! expansion and statistics. Review changes with `cargo insta review`.

use manang_writing::format::{format_document, format_range};
use manang_writing::lint::{LintConfig, lint};
use manang_writing::mask::prose;
use manang_writing::snippets::{SnippetStore, expand_body};
use manang_writing::spell::{self, Dictionaries, UserDictionary};
use manang_writing::symbols::{MathSyntax, search};
use manang_writing::{Syntax, stats};
use serde_json::json;

const TYPST_SAMPLE: &str = "\
= Intoduction
This is is a sample document.  It was written by the the team, and it is very clear that
results were obtained quickly. Inline math like $x + x$ and `a a` raw text are ignored.

#figure(caption: [A well-known result and a well known one.])[body]
See @smith2020 for details, or #link(\"https://exmple.org/qqq\")[this link].
// a a repeated comment comment
";

const LATEX_SAMPLE: &str = "\
\\section{Method}
We compute the the integral $\\int_0^1 x\\,dx$ as shown in \\cite{knuth84}.  The value
was determined numerically. % a a comment
\\begin{equation}
  x x = y y
\\end{equation}
It is very obviously correct.
";

const MARKDOWN_SAMPLE: &str = "\
# Resluts

The sample was heated, and and then cooled.

```
code code code
```

See [the docs](https://exmple.org/a) and $a a$ and `b b`.
";

fn shared() -> (Dictionaries, UserDictionary, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    (
        Dictionaries::new(dir.path().join("dictionaries")),
        UserDictionary::new(dir.path().join("user.txt")),
        dir,
    )
}

#[test]
fn lint_typst() {
    let masked = prose(TYPST_SAMPLE, Syntax::Typst, &[]);
    insta::assert_json_snapshot!(lint(&masked, &LintConfig::default(), Some("/main.typ")));
}

#[test]
fn lint_latex() {
    let masked = prose(LATEX_SAMPLE, Syntax::Latex, &[]);
    insta::assert_json_snapshot!(lint(&masked, &LintConfig::default(), Some("/main.tex")));
}

#[test]
fn lint_markdown() {
    let masked = prose(MARKDOWN_SAMPLE, Syntax::Markdown, &[]);
    insta::assert_json_snapshot!(lint(&masked, &LintConfig::default(), Some("/notes.md")));
}

#[test]
fn lint_configured_rules() {
    let masked = prose(TYPST_SAMPLE, Syntax::Typst, &[]);
    let config = LintConfig::from_json(Some(&json!({
        "long-sentence": {"max_words": 8, "severity": "warning"},
        "weasel-words": false,
        "passive-voice": {"severity": "error"},
    })))
    .expect("valid");
    insta::assert_json_snapshot!(lint(&masked, &config, None));
}

#[test]
fn spelling_diagnostics() {
    let (dicts, user, _dir) = shared();
    let (_, dict) = dicts.get(None).expect("dictionary");
    let masked = prose(TYPST_SAMPLE, Syntax::Typst, &[]);
    let found = spell::check(&dict, &user, &masked, 3).expect("check");
    insta::assert_json_snapshot!("spelling_found", found);
    insta::assert_json_snapshot!(
        "spelling_diagnostics",
        spell::diagnostics(Some("/main.typ"), &masked, &found)
    );
}

#[test]
fn format_typst_output() {
    let src = "#let f(x,y)=x+y\n#import \"a.typ\": z, b, a\n\n\n\n= Title\nText   here  with *bold*.\n#table(columns:2,[a],[b],[c],[d])\n#f(1,2)\n";
    let out = format_document(src, Syntax::Typst, None).expect("formats");
    insta::assert_snapshot!("typst_document", out.formatted);
    let narrow = format_document(
        src,
        Syntax::Typst,
        Some(&json!({"max_width": 30, "tab_spaces": 4})),
    )
    .expect("formats");
    insta::assert_snapshot!("typst_document_narrow", narrow.formatted);
    let range =
        format_range("#let a=1\n#let b=2\n#let c=3\n", 9, 17, Syntax::Typst, None).expect("range");
    insta::assert_json_snapshot!("typst_range", range);
}

#[test]
fn format_markdown_output() {
    let src = "\n\n# Title  \n\n\n\nSome text with trailing space \nhard break  \nnext\n\n```\ncode   \n\n\n\nmore\n```\n\n$$\nx   \n\n\ny\n$$\n\n    indented code   \n\n\n- item\t\n\n\n";
    let out = format_document(src, Syntax::Markdown, None).expect("formats");
    insta::assert_snapshot!("markdown_document", out.formatted);
}

#[test]
fn symbol_lookups() {
    let queries = [
        "alpha",
        "a",
        "forall",
        "for all",
        "\\rightarrow",
        "arrow.r",
        "implies",
        "integral",
        "nabla",
        "phi",
        "real numbers",
        "\u{2208}",
        "sqrt",
        "zzzz",
    ];
    let mut rows = Vec::new();
    for q in queries {
        for (label, syntax) in [("typst", MathSyntax::Typst), ("latex", MathSyntax::Latex)] {
            let hits: Vec<String> = search(q, syntax, 4, None)
                .into_iter()
                .map(|h| format!("{} {} {}", h.name, h.insert, h.preview))
                .collect();
            rows.push(json!({"query": q, "syntax": label, "hits": hits}));
        }
    }
    insta::assert_json_snapshot!(rows);
}

#[test]
fn snippet_expansions() {
    let store = SnippetStore::new(std::path::PathBuf::from("/nonexistent/snippets.toml"));
    let view = store.view();
    let mut out = Vec::new();
    for trigger in [
        "theorem",
        "equation",
        "figure",
        "table",
        "cite",
        "algorithm",
    ] {
        for syntax in [Syntax::Typst, Syntax::Latex, Syntax::Markdown] {
            let snippet = view.get(trigger, syntax).expect("builtin exists");
            let e = expand_body(&snippet.body, "").expect("expands");
            out.push(json!({"trigger": trigger, "syntax": syntax, "expansion": e}));
        }
    }
    insta::assert_json_snapshot!(out);
}

#[test]
fn statistics() {
    let samples = [
        ("typst", Syntax::Typst, TYPST_SAMPLE),
        ("latex", Syntax::Latex, LATEX_SAMPLE),
        ("markdown", Syntax::Markdown, MARKDOWN_SAMPLE),
    ];
    let rows: Vec<_> = samples
        .iter()
        .map(|(name, syntax, text)| json!({"sample": name, "stats": stats::count(text, *syntax)}))
        .collect();
    insta::assert_json_snapshot!(rows);
}
