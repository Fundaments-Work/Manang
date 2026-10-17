//! Formatting must never change what a Typst document compiles to. Each
//! document is compiled before and after formatting and the rendered pages
//! compared byte for byte (embedded fonts only, so this is machine independent).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use manang_typst::{CompileRequest, Compiler, CompilerConfig};
use manang_writing::Syntax;
use manang_writing::format::{format_document, format_range};
use proptest::prelude::*;

fn compiler() -> &'static Compiler {
    static C: OnceLock<Compiler> = OnceLock::new();
    C.get_or_init(|| {
        Compiler::new(CompilerConfig {
            threads: 2,
            ..CompilerConfig::default()
        })
        .expect("pool starts")
    })
}

/// Rendered SVG of every page, or `None` when the document does not compile.
fn render(source: &str) -> Option<Vec<Vec<u8>>> {
    // The compiler keeps one "newest document"; serialise compile + render so
    // parallel tests cannot render each other's pages.
    static SERIAL: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    let _guard = SERIAL.lock();
    let compiler = compiler();
    let report = compiler
        .compile(&CompileRequest {
            main: "main.typ".to_owned(),
            root: Some(std::env::temp_dir().join("manang-writing-format-root")),
            sources: BTreeMap::from([("main.typ".to_owned(), source.to_owned())]),
        })
        .expect("compile request is valid");
    if !report.ok {
        return None;
    }
    Some(
        (0..report.pages)
            .map(|p| compiler.render(p).expect("render").to_vec())
            .collect(),
    )
}

fn assert_same_output(source: &str) {
    let Ok(out) = format_document(source, Syntax::Typst, None) else {
        return; // Not formattable (syntax errors): nothing to preserve.
    };
    let before = render(source);
    let after = render(&out.formatted);
    assert_eq!(
        before.is_some(),
        after.is_some(),
        "compile success changed by formatting\n--- before\n{source}\n--- after\n{}",
        out.formatted
    );
    assert!(
        before == after,
        "rendered output changed by formatting\n--- before\n{source}\n--- after\n{}",
        out.formatted
    );
}

const PAGE: &str = "#set page(width: 220pt, height: 160pt, margin: 12pt)\n";

const DOCUMENTS: &[&str] = &[
    "= Title\nSome   text with *bold*  and _italic_ words.\n\n\n\nNext paragraph.",
    "#let f(x,y)=x+y\n#f(1,2) and #f( 3 , 4 )\n",
    "#set text(size: 9pt)\n#show heading: it => [#it.body]\n= Heading\nBody  text.\n",
    "- item one\n-   item two\n  - nested   item\n+ numbered\n+ second\n",
    "Inline math $a^2+b^2=c^2$ and display:\n$ sum_(i=1)^n i = (n(n+1))/2 $\n",
    "#table(columns: 3, [a],[b],[c], [1],[2],[3])\n",
    "#for i in range(3) [Item #i \\ ]\n",
    "#let items = (1,2,3)\n#items.map(x => str(x)).join(\", \")\n",
    "Text with `raw  code` and a\n```rust\nfn  main()  {}\n```\nblock.\n",
    "#import \"@preview/none:0.0.0\": x\nText\n",
    "// comment\nText /* inline */ here.\n/* block\n   comment */\nEnd.",
    "#figure(rect(width: 20pt, height: 10pt), caption: [A  caption.]) <fig:a>\nSee @fig:a.\n",
    "#set par(justify: true)\nA long paragraph that should wrap across the page width for sure because it has many words in it.\n",
    "Hello  \\\nworld  with  breaks.\n",
    "#let x = [content with   spaces]\n#x\n#x\n",
];

#[test]
fn fixtures_render_identically_after_formatting() {
    let mut compiled = 0;
    for doc in DOCUMENTS {
        let source = format!("{PAGE}{doc}");
        assert_same_output(&source);
        compiled += usize::from(render(&source).is_some());
    }
    assert!(
        compiled >= DOCUMENTS.len() - 2,
        "fixtures should mostly compile, got {compiled}"
    );
}

#[test]
fn range_formatting_keeps_output() {
    for doc in DOCUMENTS {
        let source = format!("{PAGE}{doc}");
        let len = source.chars().count();
        // Format the middle of the document.
        let Ok(out) = format_range(&source, len / 3, len / 2, Syntax::Typst, None) else {
            continue;
        };
        let before = render(&source);
        let after = render(&out.formatted);
        assert_eq!(before.is_some(), after.is_some(), "{source}");
        assert!(
            before == after,
            "range format changed output of\n{source}\n---\n{}",
            out.formatted
        );
    }
}

fn fragment() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec![
        "= Heading\n",
        "== Sub  heading\n",
        "Some   words  with *bold*  and _emph_.\n",
        "\n\n\n",
        "- list   item\n",
        "+ numbered item\n",
        "$a+b=c$ inline.\n",
        "$ x^2 + y^2 $\n",
        "#let f(a,b)=a+b\n",
        "#f(1,2)\n",
        "#lorem(5)\n",
        "#strong[bold  call]\n",
        "// line comment\n",
        "#let v = (1,2,3)\n",
        "#v.len()\n",
        "#box(width: 10pt, height: 5pt, fill: red)\n",
        "text with `code`  here\n",
        "#table(columns: 2, [a],[b])\n",
        "#set text(size: 9pt)\n",
        "#h(1em)spaced\n",
        "@ref-missing\n",
    ])
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]

    #[test]
    fn generated_documents_keep_their_output(parts in prop::collection::vec(fragment(), 1..10)) {
        assert_same_output(&format!("{PAGE}{}", parts.concat()));
    }
}
