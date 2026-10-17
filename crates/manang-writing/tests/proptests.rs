//! Arbitrary text must never panic any tool, every reported offset must be a
//! valid character offset inside the text, and formatters must be idempotent.

use std::sync::OnceLock;

use manang_writing::format::{self, MarkdownOptions, markdown, typst as typst_fmt};
use manang_writing::lint::{LintConfig, lint};
use manang_writing::mask::{OPAQUE, prose};
use manang_writing::snippets::expand_body;
use manang_writing::spell::{self, Dictionaries, UserDictionary};
use manang_writing::stats;
use manang_writing::symbols::{self, MathSyntax};
use manang_writing::{CharRange, Syntax, TextIndex};
use proptest::prelude::*;

struct Shared {
    dictionaries: Dictionaries,
    user: UserDictionary,
    _dir: tempfile::TempDir,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        Shared {
            dictionaries: Dictionaries::new(dir.path().join("dictionaries")),
            user: UserDictionary::new(dir.path().join("user.txt")),
            _dir: dir,
        }
    })
}

fn any_text() -> impl Strategy<Value = String> {
    prop_oneof![
        "\\PC{0,300}",
        // Dense in markup metacharacters.
        "[a-zA-Z0-9 \\n$#@\\\\{}()\\[\\]`*_<>%.,;:'\"=+!?/~|&^-]{0,300}",
        // Words and sentences with a little noise and non-ASCII.
        "([A-Za-z\u{e9}\u{3b1}\u{4e2d}]{1,9}[ .,!?\\n$`]{1,3}){0,60}",
    ]
}

fn any_syntax() -> impl Strategy<Value = Syntax> {
    prop::sample::select(vec![
        Syntax::Typst,
        Syntax::Latex,
        Syntax::Markdown,
        Syntax::Plain,
        Syntax::Generic,
        Syntax::Auto,
    ])
}

fn slice(text: &str, range: CharRange) -> String {
    text.chars()
        .skip(range.start)
        .take(range.end - range.start)
        .collect()
}

fn assert_valid_range(text: &str, index: &TextIndex<'_>, range: CharRange) {
    let len = index.char_len();
    assert!(range.start <= range.end, "{range:?}");
    assert!(range.end <= len, "{range:?} beyond {len}");
    // Converting back must land on a character boundary.
    assert!(text.is_char_boundary(index.char_to_byte(range.start)));
    assert!(text.is_char_boundary(index.char_to_byte(range.end)));
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn masking_preserves_length_and_newlines(text in any_text(), syntax in any_syntax()) {
        let masked = prose(&text, syntax, &[]);
        prop_assert_eq!(masked.chars().count(), text.chars().count());
        for (a, b) in text.chars().zip(masked.chars()) {
            prop_assert!(a == b || b == OPAQUE || a == b'\r' as char, "{a:?} became {b:?}");
            prop_assert!(a != '\n' || b == '\n');
        }
    }

    #[test]
    fn spelling_ranges_are_valid_char_ranges(text in any_text(), syntax in any_syntax()) {
        let s = shared();
        let (_, dict) = s.dictionaries.get(None).expect("dictionary");
        let masked = prose(&text, syntax, &[]);
        let found = spell::check(&dict, &s.user, &masked, 0).expect("check");
        let index = TextIndex::new(&text);
        for m in found {
            assert_valid_range(&text, &index, m.range);
            prop_assert_eq!(slice(&masked, m.range), m.word);
            prop_assert!(m.line >= 1 && m.column >= 1);
            prop_assert!(m.suggestions.is_empty());
        }
    }

    #[test]
    fn ignore_regions_never_panic(text in any_text(), a in 0usize..400, b in 0usize..400) {
        let masked = prose(&text, Syntax::Auto, &[CharRange { start: a, end: b }]);
        prop_assert_eq!(masked.chars().count(), text.chars().count());
    }

    #[test]
    fn lint_diagnostics_are_valid(text in any_text(), syntax in any_syntax()) {
        let masked = prose(&text, syntax, &[]);
        let diags = lint(&masked, &LintConfig::default(), Some("/f"));
        let index = TextIndex::new(&text);
        for d in diags {
            let range = d.range.expect("lint always sets a range");
            assert_valid_range(&text, &index, range);
            prop_assert!(d.line.is_some_and(|l| l >= 1) && d.column.is_some_and(|c| c >= 1));
            prop_assert!(!d.message.is_empty());
        }
    }

    #[test]
    fn stats_are_consistent(text in any_text(), syntax in any_syntax()) {
        let s = stats::count(&text, syntax);
        prop_assert!(s.sentences <= s.words);
        prop_assert!(s.paragraphs <= s.words);
        prop_assert!(s.characters_no_spaces <= s.characters || s.words == 0);
        prop_assert!(s.characters <= text.chars().count());
    }

    #[test]
    fn markdown_formatting_is_idempotent(text in "([ \\t]{0,6}(```|~~~|\\$\\$|---|# H|- x|a b|text|\\|)?[ \\t]{0,4}\\r?\\n){0,25}") {
        let opts = MarkdownOptions::default();
        let once = markdown::format_document(&text, &opts);
        let twice = markdown::format_document(&once, &opts);
        prop_assert_eq!(&once, &twice);
    }

    #[test]
    fn markdown_formatting_of_arbitrary_text_is_idempotent(text in any_text()) {
        let opts = MarkdownOptions::default();
        let once = markdown::format_document(&text, &opts);
        prop_assert_eq!(markdown::format_document(&once, &opts), once);
    }

    #[test]
    fn markdown_range_is_a_valid_edit(text in any_text(), a in 0usize..400, b in 0usize..400) {
        let index = TextIndex::new(&text);
        let (from, to) = (a.min(b).min(index.char_len()), a.max(b).min(index.char_len()));
        let out = format::format_range(&text, from, to, Syntax::Markdown, None).expect("in range");
        assert_valid_range(&text, &index, out.range);
        prop_assert_eq!(&slice(&out.formatted, CharRange { start: 0, end: out.range.start }),
                        &slice(&text, CharRange { start: 0, end: out.range.start }));
    }

    #[test]
    fn typst_formatting_never_panics_and_is_idempotent_when_it_succeeds(text in any_text()) {
        if let Ok(once) = format::format_document(&text, Syntax::Typst, None) {
            let twice = format::format_document(&once.formatted, Syntax::Typst, None)
                .expect("formatted output formats again");
            prop_assert_eq!(&once.formatted, &twice.formatted);
        }
    }

    #[test]
    fn typst_range_formatting_never_panics(text in any_text(), a in 0usize..400, b in 0usize..400) {
        let index = TextIndex::new(&text);
        let (from, to) = (a.min(b).min(index.char_len()), a.max(b).min(index.char_len()));
        if let Ok(out) = format::format_range(&text, from, to, Syntax::Typst, None) {
            assert_valid_range(&text, &index, out.range);
            let after = TextIndex::new(&out.formatted);
            let end = out.range.start + out.replacement.chars().count();
            prop_assert!(end <= after.char_len());
            prop_assert_eq!(
                slice(&out.formatted, CharRange { start: out.range.start, end }),
                out.replacement
            );
        }
    }

    #[test]
    fn snippet_expansion_is_consistent(body in "[a-z${}:0-9\\\\ \\n]{0,40}") {
        if let Ok(e) = expand_body(&body, "  ") {
            let len = e.text.chars().count();
            prop_assert!(e.final_cursor <= len);
            for stop in &e.tab_stops {
                for r in &stop.ranges {
                    prop_assert!(r.start <= r.end && r.end <= len);
                    prop_assert_eq!(slice(&e.text, *r), stop.placeholder.clone());
                }
            }
        }
    }

    #[test]
    fn symbol_search_never_panics(query in "\\PC{0,20}", limit in 0usize..300) {
        for syntax in [MathSyntax::Typst, MathSyntax::Latex] {
            let hits = symbols::search(&query, syntax, limit, None);
            prop_assert!(hits.len() <= limit.clamp(1, symbols::MAX_LIMIT));
        }
    }
}

proptest! {
    // Suggestions are slow in debug builds, so far fewer cases.
    #![proptest_config(ProptestConfig { cases: 12, ..ProptestConfig::default() })]

    #[test]
    fn suggestions_respect_the_limit(text in "([a-z]{3,9} ){0,12}") {
        let s = shared();
        let (_, dict) = s.dictionaries.get(None).expect("dictionary");
        for m in spell::check(&dict, &s.user, &text, 2).expect("check") {
            prop_assert!(m.suggestions.len() <= 2);
        }
    }
}

#[test]
fn typst_options_do_not_break_idempotence() {
    use serde_json::json;
    let src = "#let f(x,y)=x+y\nText   with  spaces and a very long line that goes on and on and on past eighty columns wide.\n";
    for options in [
        json!({"max_width": 40}),
        json!({"tab_spaces": 4, "reorder_import_items": false}),
        json!({"wrap_mode": "fill", "max_width": 50}),
        json!({"wrap_mode": "sentence"}),
        json!({"collapse_markup_spaces": true}),
        json!({"line_ending": "crlf"}),
    ] {
        let config = typst_fmt::config_from_json(Some(&options)).expect("valid");
        let once = typst_fmt::format_document(src, &config).expect("formats");
        let twice = typst_fmt::format_document(&once.formatted, &config).expect("formats again");
        assert_eq!(once.formatted, twice.formatted, "{options}");
    }
}
