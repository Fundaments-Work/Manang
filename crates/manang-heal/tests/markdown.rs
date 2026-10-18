//! Markdown rules: scans that need no compiler diagnostic.

mod common;

use common::{applied, run, show};
use manang_heal::{Confidence, Engine};

#[test]
fn unclosed_fence_is_closed_at_the_end() {
    let text = "# Notes\n\n```rust\nfn main() {}\n";
    let s = run(Engine::Markdown, text, &[]);
    assert_eq!(s.fixes.len(), 1);
    let fix = &s.fixes[0];
    assert_eq!(fix.rule, "markdown.unclosed-fence");
    assert_eq!(fix.confidence, Confidence::Certain);
    assert_eq!(
        applied(text, fix),
        "# Notes\n\n```rust\nfn main() {}\n```\n"
    );
    let finding = &s.findings[s.fixes[0].finding.expect("finding")];
    assert_eq!((finding.line, finding.from, finding.to), (3, 9, 16));
    insta::assert_json_snapshot!("md_fence", show(Engine::Markdown, text, &[]));
}

#[test]
fn closed_fences_tildes_and_longer_fences() {
    assert!(run(Engine::Markdown, "```\na\n```\n", &[]).fixes.is_empty());
    assert!(run(Engine::Markdown, "~~~\na\n~~~\n", &[]).fixes.is_empty());
    // A longer opening fence needs a closing fence at least as long.
    let text = "````\n```\ninner\n```\n";
    let s = run(Engine::Markdown, text, &[]);
    assert_eq!(applied(text, &s.fixes[0]), format!("{text}````\n"));
    // A shorter or different fence does not close.
    let text = "~~~\ncode\n```\n";
    assert_eq!(run(Engine::Markdown, text, &[]).fixes.len(), 1);
}

#[test]
fn front_matter_and_missing_final_newline() {
    let text = "---\ntitle: \"```\"\n---\n\n```\ncode";
    let s = run(Engine::Markdown, text, &[]);
    assert_eq!(
        applied(text, &s.fixes[0]),
        "---\ntitle: \"```\"\n---\n\n```\ncode\n```\n"
    );
    let text = "---\nnote: ```\n---\ntext\n";
    assert!(run(Engine::Markdown, text, &[]).fixes.is_empty());
}

#[test]
fn table_delimiter_row_is_widened_to_the_header() {
    let text = "| a | b | c |\n|---|---|\n| 1 | 2 | 3 |\n";
    let s = run(Engine::Markdown, text, &[]);
    assert_eq!(s.fixes.len(), 1);
    assert_eq!(s.fixes[0].confidence, Confidence::Likely);
    assert_eq!(
        applied(text, &s.fixes[0]),
        "| a | b | c |\n| --- | --- | --- |\n| 1 | 2 | 3 |\n"
    );
    assert!(
        s.findings[0]
            .message
            .contains("2 columns but the header has 3")
    );
    insta::assert_json_snapshot!("md_table", show(Engine::Markdown, text, &[]));
}

#[test]
fn alignment_markers_are_kept_and_good_tables_are_left_alone() {
    let text = "a | b | c\n:-- | --:\n";
    let s = run(Engine::Markdown, text, &[]);
    assert_eq!(applied(text, &s.fixes[0]), "a | b | c\n:-- | --: | ---\n");
    assert!(
        run(Engine::Markdown, "| a | b |\n|---|:-:|\n", &[])
            .fixes
            .is_empty()
    );
    // Inside a fence it is code, not a table.
    assert!(
        run(Engine::Markdown, "```\n| a | b |\n|---|\n```\n", &[])
            .fixes
            .is_empty()
    );
}

#[test]
fn offsets_are_utf16() {
    let text = "😀 text\n\n```\ncode\n";
    let s = run(Engine::Markdown, text, &[]);
    let finding = &s.findings[0];
    assert_eq!(finding.from, "😀 text\n\n".encode_utf16().count());
    assert_eq!(s.fixes[0].edits[0].from, text.encode_utf16().count());
}
