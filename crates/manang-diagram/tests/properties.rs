//! Property tests: arbitrary input never panics, rendering is deterministic,
//! and diagnostics always point inside the source.

mod fixtures;

use fixtures::FIXTURES;
use manang_diagram::{DiagramPlugin, Theme, plugin};
use proptest::prelude::*;

const LANGUAGES: [&str; 3] = ["mermaid", "pikchr", "svgbob"];

fn diagrams() -> &'static DiagramPlugin {
    static PLUGIN: std::sync::OnceLock<DiagramPlugin> = std::sync::OnceLock::new();
    PLUGIN.get_or_init(plugin)
}

/// Text that looks like a diagram often enough to reach the parsers' and
/// layouts' deeper paths: fixture prefixes, keywords, punctuation and
/// multi-byte characters.
fn source() -> impl Strategy<Value = String> {
    let fixture = prop::sample::select(FIXTURES.iter().map(|f| f.2).collect::<Vec<_>>());
    let noise = prop::collection::vec(
        prop_oneof![
            Just("-->".to_owned()),
            Just("\n".to_owned()),
            Just("[".to_owned()),
            Just("(".to_owned()),
            Just("{".to_owned()),
            Just(":::accent".to_owned()),
            Just("$$x^2$$".to_owned()),
            Just("é→😀".to_owned()),
            "[a-zA-Z0-9 ]{0,8}",
            any::<char>().prop_map(String::from),
        ],
        0..24,
    );
    (fixture, any::<prop::sample::Index>(), noise).prop_map(|(fixture, cut, noise)| {
        let mut at = cut.index(fixture.len() + 1);
        while !fixture.is_char_boundary(at) {
            at -= 1;
        }
        let mut s = fixture[..at].to_owned();
        s.push_str(&noise.concat());
        s
    })
}

fn theme() -> impl Strategy<Value = Theme> {
    prop::sample::select(Theme::ALL.to_vec())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn check_never_panics_and_stays_in_bounds(lang in prop::sample::select(LANGUAGES.to_vec()), src in source()) {
        let diagnostics = diagrams().check(lang, &src).expect("known language");
        let lines = src.split('\n').count();
        for d in diagnostics {
            prop_assert!(d.line >= 1 && d.line <= lines, "{d:?}");
            prop_assert!(d.column >= 1, "{d:?}");
            prop_assert!((d.end_line, d.end_column) >= (d.line, d.column), "{d:?}");
        }
    }

    #[test]
    fn render_is_deterministic(lang in prop::sample::select(LANGUAGES.to_vec()), src in source(), theme in theme()) {
        let first = diagrams().render(lang, &src, theme);
        let second = diagrams().render(lang, &src, theme);
        prop_assert_eq!(first, second);
    }

    #[test]
    fn arbitrary_strings_never_panic(lang in prop::sample::select(LANGUAGES.to_vec()), src in any::<String>()) {
        let _ = diagrams().check(lang, &src);
        let _ = diagrams().render(lang, &src, Theme::Dark);
    }
}

#[test]
fn diagnostics_map_multibyte_sources_to_character_columns() {
    let d = |lang: &str, src: &str| {
        diagrams()
            .check(lang, src)
            .expect("known language")
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("{lang}: expected a diagnostic for {src:?}"))
    };

    // Mermaid: CJK and emoji (3- and 4-byte) before the error on line 3.
    let m = d(
        "mermaid",
        "flowchart TD\n  A[日本] --> B\n  B[😀x] --> C(\n",
    );
    assert_eq!((m.line, m.column), (3, 14), "{m:?}");
    // A lexer error without a span: the offset in the message is used.
    let m = d("mermaid", "flowchart TD\n  A[日本] --> B\n  😀x --> C\n");
    assert_eq!((m.line, m.column), (3, 3), "{m:?}");

    // Sequence diagram error after an accented name.
    let s = d(
        "mermaid",
        "sequenceDiagram\n  Zoë->>Bob: héllo\n  Bob-->>: x\n",
    );
    assert_eq!((s.line, s.column), (3, 10), "{s:?}");

    // Pikchr: the grammar's byte offset becomes a character column.
    let p = d("pikchr", "box \"é→ü\"\nfoo bar baz\n");
    assert_eq!((p.line, p.column), (2, 5), "{p:?}");

    // No diagram at all marks the whole source.
    let e = d("mermaid", "not a diagram\nat all");
    assert_eq!(
        (e.line, e.column, e.end_line, e.end_column),
        (1, 1, 2, 7),
        "{e:?}"
    );
}
