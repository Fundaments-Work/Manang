//! `typst.svg-to-pdf`: one SVG in, a one-page PDF out, through the kernel.

use std::sync::Arc;

use manang_kernel::{Kernel, Payload};
use manang_typst::{CompilerConfig, manifest, plugin};
use serde_json::json;

/// Shaped like the diagram plugin's output: a leading comment, a percentage
/// width and a viewBox.
const SVG: &str = "<!-- manang-diagram mermaid key=abc -->\n\
<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100%\" viewBox=\"0 0 120 60\" style=\"max-width:120px\">\
<rect x=\"10\" y=\"10\" width=\"100\" height=\"40\" fill=\"#ffffff\" stroke=\"#1c1c1c\"/>\
<text x=\"60\" y=\"35\" font-family=\"DM Sans, sans-serif\" font-size=\"12\" text-anchor=\"middle\">Start</text>\
</svg>\n";

fn kernel() -> Kernel {
    let kernel = Kernel::new();
    kernel
        .register(
            manifest().expect("manifest is valid"),
            Arc::new(plugin(CompilerConfig::default()).expect("plugin starts")),
        )
        .expect("registers");
    kernel
}

fn pdf_of(k: &Kernel, input: Payload) -> Arc<[u8]> {
    match k.invoke("typst.svg-to-pdf", input).expect("converts") {
        Payload::Binary(bytes) => bytes,
        other => panic!("expected binary, got {other:?}"),
    }
}

/// Pages in a PDF: `/Type/Page` dictionaries (typst-pdf writes them
/// uncompressed and without spaces), not the `/Pages` tree nodes.
fn page_count(pdf: &[u8]) -> usize {
    let text = String::from_utf8_lossy(pdf);
    text.match_indices("/Type/Page")
        .filter(|(at, _)| !text[at + "/Type/Page".len()..].starts_with('s'))
        .count()
}

#[test]
fn an_svg_becomes_a_one_page_pdf() {
    let k = kernel();
    let pdf = pdf_of(&k, Payload::Binary(Arc::from(SVG.as_bytes())));
    assert!(pdf.starts_with(b"%PDF-"), "PDF magic bytes");
    assert_eq!(page_count(&pdf), 1, "one page");
}

#[test]
fn json_input_works_and_output_is_deterministic() {
    let k = kernel();
    let a = pdf_of(&k, Payload::Binary(Arc::from(SVG.as_bytes())));
    let b = pdf_of(&k, Payload::Json(json!({ "svg": SVG })));
    assert_eq!(a, b, "same SVG, same bytes");
}

#[test]
fn broken_input_is_an_error_not_a_panic() {
    let k = kernel();
    for input in [
        Payload::Empty,
        Payload::Json(json!({ "svg": 3 })),
        Payload::Json(json!({ "svg": "not an svg" })),
        Payload::Binary(Arc::from(&b"\xff\xfe"[..])),
        Payload::Binary(Arc::from(&b"<svg"[..])),
    ] {
        assert!(k.invoke("typst.svg-to-pdf", input).is_err());
    }
}

#[test]
fn converting_does_not_touch_the_compiled_document() {
    let k = kernel();
    assert!(
        k.invoke("typst.export-pdf", Payload::Empty).is_err(),
        "no document yet"
    );
    pdf_of(&k, Payload::Binary(Arc::from(SVG.as_bytes())));
    assert!(
        k.invoke("typst.export-pdf", Payload::Empty).is_err(),
        "still no document"
    );
}
