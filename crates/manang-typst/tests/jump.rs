//! Source and preview jumps against real (temporary) projects.
//! Only embedded fonts are used so positions are machine independent.

use std::collections::BTreeMap;
use std::fs;

use manang_typst::{
    CompileRequest, Compiler, CompilerConfig, Environment, PagePoint, SourceJump, TypstError,
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

fn request(dir: &TempDir, sources: &[(&str, &str)]) -> CompileRequest {
    CompileRequest {
        main: "main.typ".to_owned(),
        root: Some(dir.path().to_path_buf()),
        sources: sources
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect::<BTreeMap<_, _>>(),
    }
}

const DOC: &str = "#set page(width: 200pt, height: 120pt, margin: 10pt)
= Heading

First paragraph here.

#pagebreak()
Second page text.
#include \"chapter.typ\"
";
const CHAPTER: &str = "Chapter words.\n";

fn compiled() -> (TempDir, Compiler) {
    let dir = project(&[("main.typ", DOC), ("chapter.typ", CHAPTER)]);
    let c = compiler();
    let report = c.compile(&request(&dir, &[])).expect("compile");
    assert!(report.ok, "{:?}", report.diagnostics);
    assert_eq!(report.pages, 2);
    (dir, c)
}

/// A point just inside the glyph box above the baseline `find_in_frame` gives.
fn inside(p: PagePoint) -> (usize, f64, f64) {
    (p.page, p.x + 1.0, p.y - 2.0)
}

#[test]
fn cursor_on_text_maps_to_its_page_and_back() {
    let (_dir, c) = compiled();
    let first = c.jump_from_cursor("main.typ", 4, 3).expect("cursor");
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0].page, 0);
    let (page, x, y) = inside(first[0]);
    match c.jump_from_click(page, x, y).expect("click") {
        Some(SourceJump::File { path, line, .. }) => {
            assert_eq!(path, "main.typ");
            assert_eq!(line, 4);
        }
        other => panic!("expected a file jump, got {other:?}"),
    }

    let second = c.jump_from_cursor("main.typ", 7, 1).expect("cursor");
    assert_eq!(second.first().map(|p| p.page), Some(1), "{second:?}");
}

#[test]
fn included_files_jump_by_their_own_path() {
    let (dir, c) = compiled();
    let found = c.jump_from_cursor("chapter.typ", 1, 2).expect("cursor");
    assert_eq!(found.first().map(|p| p.page), Some(1), "{found:?}");
    let (page, x, y) = inside(found[0]);
    assert_eq!(
        c.jump_from_click(page, x, y).expect("click"),
        Some(SourceJump::File {
            path: "chapter.typ".to_owned(),
            line: 1,
            // A click resolves to the start of the text run it hit.
            column: 1,
        })
    );
    // An absolute path inside the root is the same file.
    let absolute = dir.path().join("chapter.typ");
    let again = c
        .jump_from_cursor(absolute.to_str().expect("utf8"), 1, 2)
        .expect("cursor");
    assert_eq!(again, found);
}

#[test]
fn clicks_on_nothing_and_cursors_on_markup_find_nothing() {
    let (_dir, c) = compiled();
    // The bottom-right margin.
    assert_eq!(c.jump_from_click(0, 199.0, 119.0).expect("click"), None);
    // `#set ...` produces no text.
    assert!(
        c.jump_from_cursor("main.typ", 1, 3)
            .expect("cursor")
            .is_empty()
    );
    // A file that was not part of the compile.
    assert!(
        c.jump_from_cursor("other.typ", 1, 1)
            .expect("cursor")
            .is_empty()
    );
    // Lines and columns past the end clamp, never panic.
    let _ = c.jump_from_cursor("main.typ", 999, 999).expect("cursor");
    let _ = c.jump_from_cursor("main.typ", 0, 0).expect("cursor");
}

#[test]
fn links_jump_to_urls_and_places() {
    let doc = "#set page(width: 200pt, height: 120pt, margin: 10pt)
#link(\"https://typst.app\")[Site]

#link(<target>)[Go]
#pagebreak()
= Target <target>
";
    let dir = project(&[("main.typ", doc)]);
    let c = compiler();
    assert!(c.compile(&request(&dir, &[])).expect("compile").ok);

    let site = c.jump_from_cursor("main.typ", 2, 29).expect("cursor");
    let (page, x, y) = inside(site[0]);
    assert_eq!(
        c.jump_from_click(page, x, y).expect("click"),
        Some(SourceJump::Url {
            url: "https://typst.app".to_owned()
        })
    );

    let go = c.jump_from_cursor("main.typ", 4, 17).expect("cursor");
    let (page, x, y) = inside(go[0]);
    match c.jump_from_click(page, x, y).expect("click") {
        Some(SourceJump::Position(at)) => assert_eq!(at.page, 1),
        other => panic!("expected a position, got {other:?}"),
    }
}

#[test]
fn a_failed_compile_keeps_jumps_on_the_shown_document() {
    let (dir, c) = compiled();
    let before = c.jump_from_cursor("main.typ", 4, 3).expect("cursor");
    // Insert lines above and break the document: the pages on screen are
    // still the old ones, and so are the lines they map to.
    let broken = format!("#nope\n\n\n{DOC}");
    let report = c
        .compile(&request(&dir, &[("main.typ", broken.as_str())]))
        .expect("compile runs");
    assert!(!report.ok);
    let (page, x, y) = inside(before[0]);
    match c.jump_from_click(page, x, y).expect("click") {
        Some(SourceJump::File { line, .. }) => assert_eq!(line, 4),
        other => panic!("expected a file jump, got {other:?}"),
    }
}

#[test]
fn errors_are_structured() {
    let c = compiler();
    assert!(matches!(
        c.jump_from_click(0, 1.0, 1.0),
        Err(TypstError::NoDocument)
    ));
    assert!(matches!(
        c.jump_from_cursor("main.typ", 1, 1),
        Err(TypstError::NoDocument)
    ));
    let (_dir, c) = compiled();
    assert!(matches!(
        c.jump_from_click(5, 1.0, 1.0),
        Err(TypstError::PageOutOfRange { page: 5, pages: 2 })
    ));
    assert_eq!(c.jump_from_click(0, f64::NAN, 1.0).expect("nan"), None);
    assert!(matches!(
        c.jump_from_cursor("/elsewhere/x.typ", 1, 1),
        Err(TypstError::OutsideRoot(_))
    ));
}

/// Rounded so the snapshot is stable across float formatting.
fn rounded(points: &[PagePoint]) -> Vec<(usize, f64, f64)> {
    points
        .iter()
        .map(|p| {
            (
                p.page,
                (p.x * 100.0).round() / 100.0,
                (p.y * 100.0).round() / 100.0,
            )
        })
        .collect()
}

#[test]
fn snapshot_of_every_line() {
    let (_dir, c) = compiled();
    let lines: Vec<_> = (1..=8)
        .map(|line| {
            let points = c.jump_from_cursor("main.typ", line, 3).expect("cursor");
            let back = points
                .first()
                .map(|p| {
                    let (page, x, y) = inside(*p);
                    c.jump_from_click(page, x, y).expect("click")
                })
                .unwrap_or_default();
            (line, rounded(&points), back)
        })
        .collect();
    insta::assert_json_snapshot!(lines);
}
