use std::sync::Arc;
use std::time::Instant;

use manang_writing::Syntax;
use manang_writing::format::{format_document, format_range};
use manang_writing::lint::{LintConfig, lint};
use manang_writing::mask::prose;
use manang_writing::spell::{Dictionaries, Dictionary, UserDictionary, check};
use manang_writing::stats;
use manang_writing::text::TextIndex;
use manang_writing::words::words;

fn setup_spell() -> (Arc<Dictionary>, UserDictionary, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let dicts = Dictionaries::new(dir.path().join("dictionaries"));
    let user = UserDictionary::new(dir.path().join("user.txt"));
    let (_, dict) = dicts.get(Some("en_US")).expect("load bundled dict");
    (dict, user, dir)
}

#[test]
fn test_emoji_only_text() {
    let fixture = include_str!("../../../testdata/robustness/writing/emoji_only.txt");
    let (dict, user, _dir) = setup_spell();

    for syntax in [
        Syntax::Plain,
        Syntax::Typst,
        Syntax::Markdown,
        Syntax::Latex,
    ] {
        let masked = prose(fixture, syntax, &[]);
        let w = words(&masked);
        assert_eq!(w.len(), 0, "Emoji text should yield 0 alphanumeric words");

        let diags = lint(&masked, &LintConfig::default(), Some("emoji.txt"));
        assert!(
            diags.is_empty(),
            "Emoji text should not trigger prose lint rules"
        );

        let misspellings = check(&dict, &user, &masked, 5).expect("spell check succeeds");
        assert!(
            misspellings.is_empty(),
            "Emoji text should yield no misspellings"
        );

        let st = stats::count(fixture, syntax);
        assert_eq!(st.words, 0);
    }

    // Formatting emoji text
    let fmt_md = format_document(fixture, Syntax::Markdown, None).expect("format markdown");
    assert!(!fmt_md.formatted.is_empty());

    let fmt_typ = format_document(fixture, Syntax::Typst, None).expect("format typst");
    assert!(!fmt_typ.formatted.is_empty());
}

#[test]
fn test_mixed_scripts_and_zero_width() {
    let fixture = include_str!("../../../testdata/robustness/writing/mixed_scripts.txt");
    let (dict, user, _dir) = setup_spell();

    for syntax in [
        Syntax::Plain,
        Syntax::Typst,
        Syntax::Markdown,
        Syntax::Latex,
    ] {
        let masked = prose(fixture, syntax, &[]);
        let index = TextIndex::new(fixture);

        // Check word extraction and character boundaries
        let w = words(&masked);
        for word in &w {
            assert!(
                fixture.is_char_boundary(word.start),
                "word start {} not on char boundary",
                word.start
            );
            assert!(
                fixture.is_char_boundary(word.end),
                "word end {} not on char boundary",
                word.end
            );
            let s = word.text(&masked);
            assert!(!s.is_empty());
        }

        // Check prose lint
        let diags = lint(&masked, &LintConfig::default(), Some("mixed.txt"));
        for d in &diags {
            if let Some(range) = d.range {
                let start_b = index.char_to_byte(range.start);
                let end_b = index.char_to_byte(range.end);
                assert!(fixture.is_char_boundary(start_b));
                assert!(fixture.is_char_boundary(end_b));
            }
        }

        // Check spell check
        let misspellings = check(&dict, &user, &masked, 5).expect("spell check succeeds");
        for m in &misspellings {
            let start_b = index.char_to_byte(m.range.start);
            let end_b = index.char_to_byte(m.range.end);
            assert!(fixture.is_char_boundary(start_b));
            assert!(fixture.is_char_boundary(end_b));
        }

        let _st = stats::count(fixture, syntax);
    }

    // Markdown range formatting on various character slices
    let index = TextIndex::new(fixture);
    for char_offset in [0, 5, 15, 30, 50, index.char_len().saturating_sub(10)] {
        let to = (char_offset + 10).min(index.char_len());
        if char_offset <= to {
            let res = format_range(fixture, char_offset, to, Syntax::Markdown, None);
            assert!(res.is_ok(), "format_range failed at {char_offset}..{to}");
            let r = res.unwrap();
            let b_start = index.char_to_byte(r.range.start);
            let b_end = index.char_to_byte(r.range.end);
            assert!(fixture.is_char_boundary(b_start));
            assert!(fixture.is_char_boundary(b_end));
        }
    }
}

#[test]
fn test_unbalanced_delimiters_typst() {
    let fixture = include_str!("../../../testdata/robustness/writing/unbalanced_delimiters.typ");
    let (dict, user, _dir) = setup_spell();

    let masked = prose(fixture, Syntax::Typst, &[]);
    let _diags = lint(&masked, &LintConfig::default(), Some("unbalanced.typ"));
    let _misspellings = check(&dict, &user, &masked, 5).expect("spell check succeeds");
    let _st = stats::count(fixture, Syntax::Typst);

    // Format should either succeed or return skipped/unsupported/error without panic
    let fmt_res = format_document(fixture, Syntax::Typst, None);
    // Unbalanced typst might fail to parse cleanly or return skipped
    match fmt_res {
        Ok(out) => {
            // Either changed or skipped
            if let Some(reason) = out.skipped {
                assert!(!reason.is_empty());
            }
        }
        Err(e) => {
            // An error is also acceptable for invalid syntax, never a panic
            let _ = e.to_string();
        }
    }
}

#[test]
fn test_unbalanced_delimiters_latex() {
    let fixture = include_str!("../../../testdata/robustness/writing/unbalanced_delimiters.tex");
    let (dict, user, _dir) = setup_spell();

    let masked = prose(fixture, Syntax::Latex, &[]);
    let _diags = lint(&masked, &LintConfig::default(), Some("unbalanced.tex"));
    let _misspellings = check(&dict, &user, &masked, 5).expect("spell check succeeds");
    let _st = stats::count(fixture, Syntax::Latex);

    // LaTeX formatting is documented as unsupported, must return Err(Unsupported), not panic
    let fmt_res = format_document(fixture, Syntax::Latex, None);
    assert!(fmt_res.is_err());
}

#[test]
fn test_very_long_words() {
    let (dict, user, _dir) = setup_spell();

    // 10,000-character single word
    let long_word = "a".repeat(10_000);
    let doc = format!(
        "Start of text {} middle {} end of text.",
        long_word,
        "b".repeat(5_000)
    );

    let masked = prose(&doc, Syntax::Plain, &[]);
    let w = words(&masked);
    assert_eq!(w.len(), 9);

    let diags = lint(&masked, &LintConfig::default(), Some("long_words.txt"));
    // Must complete without panic
    let _ = diags.len();

    let misspellings = check(&dict, &user, &masked, 5).expect("spell check succeeds");
    // Long word should be flagged or skipped, without panic
    let _ = misspellings.len();

    let st = stats::count(&doc, Syntax::Plain);
    assert_eq!(st.words, 9);
}

#[test]
fn test_five_megabyte_text_performance() {
    let (dict, user, _dir) = setup_spell();

    // Generate ~5 MB of realistic English text with some markup
    let base_paragraph = "The quick brown fox jumps over the lazy dog. Scientific publishing requires absolute precision and high performance. In this study, we demonstrate that Typst and LaTeX workflows can be unified seamlessly without compromise.\n\n";
    let repeats = (5 * 1024 * 1024) / base_paragraph.len() + 1;
    let large_text = base_paragraph.repeat(repeats);
    let size_mb = large_text.len() as f64 / (1024.0 * 1024.0);

    // 1. Masking
    let t0 = Instant::now();
    let masked = prose(&large_text, Syntax::Markdown, &[]);
    let mask_time = t0.elapsed();

    // 2. Word tokenization
    let t0 = Instant::now();
    let w = words(&masked);
    let words_time = t0.elapsed();

    // 3. Prose lint
    let t0 = Instant::now();
    let diags = lint(&masked, &LintConfig::default(), Some("large.md"));
    let lint_time = t0.elapsed();

    // 4. Spell check
    let t0 = Instant::now();
    let misspellings = check(&dict, &user, &masked, 5).expect("spell check succeeds");
    let spell_time = t0.elapsed();

    // 5. Statistics
    let t0 = Instant::now();
    let st = stats::count(&large_text, Syntax::Markdown);
    let stats_time = t0.elapsed();

    // 6. Markdown formatting
    let t0 = Instant::now();
    let fmt = format_document(&large_text, Syntax::Markdown, None).expect("format markdown");
    let format_time = t0.elapsed();

    println!(
        "\n[writing::robustness] 5 MB text ({:.2} MB):\n  \
         masking: {:?}\n  \
         words (count={}): {:?}\n  \
         lint (diags={}): {:?}\n  \
         spell (misspellings={}): {:?}\n  \
         stats (words={}): {:?}\n  \
         format: {:?}\n",
        size_mb,
        mask_time,
        w.len(),
        words_time,
        diags.len(),
        lint_time,
        misspellings.len(),
        spell_time,
        st.words,
        stats_time,
        format_time
    );

    assert!(!fmt.formatted.is_empty());
}
