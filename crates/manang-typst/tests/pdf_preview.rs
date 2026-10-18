//! The PDF the preview shows (ADR 0006, 2026-10-18 amendment): Typst is
//! previewed through `export_pdf`, so the PDF must carry what the viewer
//! needs (an outline, link annotations, pages sized like the compile report)
//! and its geometry must agree with `jump_from_click` / `jump_from_cursor`,
//! which speak points from the top-left corner of a page.
//!
//! `write_preview_fixtures` (ignored) regenerates the PDF and jump points the
//! frontend tests read from `testdata/typst-preview/`, plus larger documents
//! for timing when `MANANG_PDF_OUT` names a directory.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use manang_typst::{CompileRequest, Compiler, CompilerConfig, Environment};
use tempfile::TempDir;

fn compiler() -> Compiler {
    Compiler::new(CompilerConfig {
        env: Environment::default(),
        root: None,
        threads: 2,
    })
    .expect("pool starts")
}

fn request(dir: &TempDir, text: &str) -> CompileRequest {
    CompileRequest {
        main: "main.typ".to_owned(),
        root: Some(dir.path().to_path_buf()),
        sources: BTreeMap::from([("main.typ".to_owned(), text.to_owned())]),
    }
}

/// Two A5 pages: headings (the outline), an internal link to a label on the
/// second page and an external link. Embedded fonts only.
const FIXTURE: &str = "#set page(paper: \"a5\", margin: 40pt)
#set heading(numbering: \"1.\")
= Introduction <intro>
Entropy is the measure of disorder. See #link(<method>)[the method section]
and #link(\"https://typst.app\")[the Typst website].

Landauer bound and reversible computing.

#pagebreak()
= Method <method>
Measurements were taken at room temperature.
Back to #link(<intro>)[the introduction].
";

fn compiled(text: &str) -> (TempDir, Compiler, Vec<u8>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let c = compiler();
    let report = c.compile(&request(&dir, text)).expect("compile");
    assert!(report.ok, "{:?}", report.diagnostics);
    let pdf = c.export_pdf().expect("export").to_vec();
    (dir, c, pdf)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn the_preview_pdf_is_cached_per_compile() {
    let dir = tempfile::tempdir().expect("tempdir");
    let c = compiler();
    c.compile(&request(&dir, FIXTURE)).expect("compile");
    let first = c.export_pdf().expect("export");
    let again = c.export_pdf().expect("export");
    assert!(
        std::sync::Arc::ptr_eq(&first, &again),
        "same compile, same bytes"
    );
    c.compile(&request(&dir, &format!("{FIXTURE}\nOne more line.")))
        .expect("compile");
    let next = c.export_pdf().expect("export");
    assert!(
        !std::sync::Arc::ptr_eq(&first, &next),
        "a new compile exports anew"
    );
    assert!(next.starts_with(b"%PDF-"));
}

#[test]
fn the_preview_pdf_has_links_and_an_outline() {
    let (_dir, _c, pdf) = compiled(FIXTURE);
    assert!(pdf.starts_with(b"%PDF-"));
    // Uncompressed dictionaries: the viewer's link and outline layers need them.
    assert!(contains(&pdf, b"/Outlines"), "an outline from the headings");
    assert!(contains(&pdf, b"/URI"), "the external link");
    assert!(contains(&pdf, b"/Link"), "link annotations");
}

#[test]
fn jump_points_use_the_pdf_page_geometry() {
    // The viewer turns a click into points from the page's top-left corner by
    // dividing by its scale; that equals Typst's frame coordinates only if the
    // page box starts at the origin and has the compiled size.
    let dir = tempfile::tempdir().expect("tempdir");
    let c = compiler();
    let report = c.compile(&request(&dir, FIXTURE)).expect("compile");
    let pdf = c.export_pdf().expect("export");
    let boxes = media_boxes(&pdf);
    assert_eq!(boxes.len(), report.page_sizes.len());
    for (found, [w, h]) in boxes.iter().zip(&report.page_sizes) {
        let close = |a: f64, b: f64| (a - b).abs() < 0.01;
        assert!(
            close(found[0], 0.0)
                && close(found[1], 0.0)
                && close(found[2], *w)
                && close(found[3], *h),
            "page box {found:?}, compiled size {w} x {h}"
        );
    }
    // And a jump from the heading's line lands on its page, inside it.
    let points = c.jump_from_cursor("main.typ", 10, 4).expect("jump");
    let first = points.first().expect("the Method heading is on a page");
    assert_eq!(first.page, 1);
    let [w, h] = report.page_sizes[1];
    assert!(first.x > 0.0 && first.x < w && first.y > 0.0 && first.y < h);
}

/// Every `/MediaBox[x0 y0 x1 y1]` in the file, in order.
fn media_boxes(pdf: &[u8]) -> Vec<[f64; 4]> {
    let text = String::from_utf8_lossy(pdf);
    text.split("/MediaBox")
        .skip(1)
        .filter_map(|rest| {
            let inner = rest.trim_start().strip_prefix('[')?;
            let inner = &inner[..inner.find(']')?];
            let numbers: Vec<f64> = inner
                .split_whitespace()
                .filter_map(|n| n.parse().ok())
                .collect();
            <[f64; 4]>::try_from(numbers).ok()
        })
        .collect()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Regenerate `testdata/typst-preview/` (and, with `MANANG_PDF_OUT`, the 10
/// and 50 page timing documents):
/// `cargo test -p manang-typst --test pdf_preview -- --ignored`.
#[test]
#[ignore = "writes fixtures; run by hand"]
fn write_preview_fixtures() {
    let dir = tempfile::tempdir().expect("tempdir");
    let c = compiler();
    let report = c.compile(&request(&dir, FIXTURE)).expect("compile");
    assert!(report.ok, "{:?}", report.diagnostics);
    let pdf = c.export_pdf().expect("export");
    let out = repo_root().join("testdata/typst-preview");
    fs::create_dir_all(&out).expect("mkdir");
    fs::write(out.join("fixture.pdf"), &pdf[..]).expect("write pdf");
    // Where Typst says lines 3 (Introduction) and 10 (Method) landed.
    let mut jumps = Vec::new();
    for line in [3, 10] {
        let points = c.jump_from_cursor("main.typ", line, 4).expect("jump");
        let p = points.first().expect("on a page");
        jumps.push(format!(
            "{{\"line\":{line},\"page\":{},\"x\":{},\"y\":{}}}",
            p.page, p.x, p.y
        ));
    }
    let sizes: Vec<String> = report
        .page_sizes
        .iter()
        .map(|[w, h]| format!("[{w},{h}]"))
        .collect();
    fs::write(
        out.join("fixture.json"),
        format!(
            "{{\"pageSizes\":[{}],\"jumps\":[{}]}}\n",
            sizes.join(","),
            jumps.join(",")
        ),
    )
    .expect("write json");

    if let Ok(dest) = std::env::var("MANANG_PDF_OUT") {
        for pages in [10usize, 50] {
            let mut text = String::from(
                "#set page(paper: \"a4\")\n#set heading(numbering: \"1.\")\n#outline()\n",
            );
            for s in 0..pages {
                text.push_str(&format!(
                    "= Section {s} <sec-{s}>\nSee #link(<sec-0>)[the first section].\n\n#lorem(600)\n\n"
                ));
            }
            let report = c.compile(&request(&dir, &text)).expect("compile");
            assert!(report.ok, "{:?}", report.diagnostics);
            let pdf = c.export_pdf().expect("export");
            fs::write(
                PathBuf::from(&dest).join(format!("typst-{}p.pdf", report.pages)),
                &pdf[..],
            )
            .expect("write timing pdf");
        }
    }
}
