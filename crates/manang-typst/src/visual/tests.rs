use super::*;
use manang_visual::SessionHost;
use proptest::prelude::*;
use std::sync::Arc;

const SAMPLE: &str = "#set text(font: \"Libertinus Serif\")\n#show heading: it => it\n\n= Introduction <intro>\n\nSome *bold* and _emphasis_ with `code` and $x^2$, see @intro and <lbl>.\nVisit https://typst.app or #link(\"https://example.org\")[the *site*].\n\n== Lists\n- first\n- second\n  - nested\n+ one\n+ two\n\n$ E = m c^2 $\n\n#figure(\n  image(\"figs/a.svg\", width: 80%),\n  caption: [A figure],\n) <fig>\n\n#image(\"b.png\")\n\n```rust\nlet x = 1;\n```\n\n// a comment\n#let n = 3\n#box[*boxed*]\n";

fn dump(s: &TypstSession) -> Vec<String> {
    let mut raw = Vec::new();
    collect(&s.source, 0..s.source.text().len(), &mut raw);
    sorted(raw, s.source.text())
}

fn sorted(mut raw: Vec<RawSpan>, text: &str) -> Vec<String> {
    raw.sort_by(|a, b| {
        a.range
            .start
            .cmp(&b.range.start)
            .then(b.range.end.cmp(&a.range.end))
            .then(a.kind.cmp(&b.kind))
    });
    raw.iter()
        .map(|s| {
            format!(
                "{}..{} {}[{}] {:?} {}",
                s.range.start,
                s.range.end,
                kind::name(s.kind).unwrap_or("?"),
                s.aux,
                &text[s.range.clone()],
                s.attr.as_deref().unwrap_or("")
            )
        })
        .collect()
}

#[test]
fn snapshot_of_the_sample_document() {
    insta::assert_snapshot!(dump(&TypstSession::new(SAMPLE)).join("\n"));
}

#[test]
fn outline_has_titles_and_levels() {
    let s = TypstSession::new("= One <a>\n\ntext\n\n=== Three\n");
    let got: Vec<_> = s
        .outline()
        .into_iter()
        .map(|h| (h.level, h.title))
        .collect();
    assert_eq!(got, vec![(1, "One".to_owned()), (3, "Three".to_owned())]);
}

fn apply(text: &str, plan: &EditPlan) -> (String, String) {
    let mut out = text.to_owned();
    for e in plan.edits.iter().rev() {
        out.replace_range(e.range.clone(), &e.replacement);
    }
    let sel = plan
        .selection
        .clone()
        .map(|r| out[r].to_owned())
        .unwrap_or_default();
    (out, sel)
}

fn run(text: &str, sel: Range<usize>, command: &str, args: &str) -> (String, String) {
    let s = TypstSession::new(text);
    let plan = s.run_command(command, sel, args).expect("command plans");
    apply(text, &plan)
}

fn after(text: &str, sel: Range<usize>, command: &str, args: &str) -> String {
    run(text, sel, command, args).0
}

#[test]
fn inline_toggles() {
    assert_eq!(after("a word b", 2..6, "bold", ""), "a *word* b");
    assert_eq!(after("a word b", 3..3, "italic", ""), "a _word_ b");
    assert_eq!(after("a *word* b", 3..7, "bold", ""), "a word b");
    assert_eq!(after("a *word* b", 5..5, "bold", ""), "a word b");
    assert_eq!(after("a _word_ b", 5..5, "italic", ""), "a word b");
    assert_eq!(after("a word b", 2..6, "code", ""), "a `word` b");
    assert_eq!(after("x $a$ y", 3..3, "math", ""), "x a y");
    assert_eq!(after("x a y", 2..3, "math", ""), "x $a$ y");
    assert_eq!(after("e=mc", 0..4, "math", "block"), "$ e=mc $");
    assert_eq!(run("e=mc", 0..4, "math", "block").1, "e=mc");
    assert_eq!(after("", 0..0, "bold", ""), "**");
}

#[test]
fn bold_inside_a_heading_and_link_label() {
    assert_eq!(after("= Title here", 2..7, "bold", ""), "= *Title* here");
    assert_eq!(
        after("#link(\"u\")[label]", 11..16, "bold", ""),
        "#link(\"u\")[*label*]"
    );
}

#[test]
fn headings_lists() {
    assert_eq!(after("title", 0..0, "heading", "2"), "== title");
    assert_eq!(after("== title", 4..4, "heading", "2"), "title");
    assert_eq!(after("== title", 4..4, "heading", "1"), "= title");
    assert_eq!(after("= title", 0..0, "heading", "0"), "title");
    assert_eq!(after("a\nb", 0..3, "heading", "3"), "=== a\n=== b");
    assert_eq!(after("a\nb", 0..3, "list", "bullet"), "- a\n- b");
    assert_eq!(after("- a\n- b", 0..7, "list", "bullet"), "a\nb");
    assert_eq!(after("- a\n- b", 0..7, "list", "ordered"), "+ a\n+ b");
    assert_eq!(after("+ a", 0..3, "list", "ordered"), "a");
    assert!(
        TypstSession::new("x")
            .run_command("heading", 0..0, "7")
            .is_err()
    );
}

#[test]
fn links_images_figures() {
    assert_eq!(
        after("see docs", 4..8, "link", "https://x.org"),
        "see #link(\"https://x.org\")[docs]"
    );
    let (t, sel) = run("see docs", 4..8, "link", "");
    assert_eq!(t, "see #link(\"\")[docs]");
    assert_eq!(sel, "");
    assert_eq!(
        after("a #link(\"u\")[b] c", 14..14, "link", ""),
        "a b c",
        "a second press removes the link"
    );
    assert_eq!(
        after("q\"x", 0..0, "image", "a\"b.png"),
        "#image(\"a\\\"b.png\")q\"x"
    );
    assert_eq!(
        after("cap", 0..3, "figure", "f.svg"),
        "#figure(image(\"f.svg\"), caption: [cap])"
    );
    let (t, sel) = run("", 0..0, "figure", "f.svg");
    assert_eq!(t, "#figure(image(\"f.svg\"), caption: [])");
    assert_eq!(sel, "");
    assert!(
        TypstSession::new("")
            .run_command("image", 0..0, "")
            .is_err()
    );
}

#[test]
fn unknown_command_is_unsupported() {
    let s = TypstSession::new("x");
    assert!(matches!(
        s.run_command("quote", 0..0, ""),
        Err(AdapterError::Unsupported(_))
    ));
}

#[test]
fn command_state_and_opaque_code() {
    let t = "= H\n*b* #set text(1)\n- i\n";
    let s = TypstSession::new(t);
    let get = |sel: Range<usize>, c: &str| {
        s.command_state(sel)
            .into_iter()
            .find(|x| x.command == c)
            .map(|x| (x.enabled, x.active))
    };
    assert_eq!(get(1..1, "heading"), Some((true, true)));
    assert_eq!(get(6..6, "bold"), Some((true, true)));
    let set = t.find("#set").unwrap_or(0);
    assert_eq!(get(set + 6..set + 6, "bold"), Some((false, false)));
    let l = t.find("- i").unwrap_or(0);
    assert_eq!(get(l + 3..l + 3, "list"), Some((true, true)));
    assert_eq!(get(0..0, "quote"), None);
}

#[test]
fn scripting_is_never_claimed_as_structure() {
    // A heading made by a function is not a heading span.
    let s = TypstSession::new("#heading(level: 2)[Hi]\n#let x = [= not]\n#show: doc => [= hmm]\n");
    let lines = dump(&s);
    assert!(lines.iter().all(|l| !l.contains("heading[")), "{lines:?}");
}

#[test]
fn incremental_edit_equals_fresh_simple() {
    let mut s = TypstSession::new(SAMPLE);
    let at = SAMPLE.find("bold").unwrap_or(0);
    s.edit(at..at + 4, "much").expect("edit");
    let want = SAMPLE.replacen("bold", "much", 1);
    assert_eq!(s.source.text(), want);
    assert_eq!(dump(&s), dump(&TypstSession::new(&want)));
}

#[test]
fn rejects_edits_outside_the_text() {
    let mut s = TypstSession::new("héllo");
    assert!(s.edit(0..99, "").is_err());
    assert!(s.edit(2..2, "x").is_err());
}

#[test]
fn works_through_the_host_with_utf16_offsets() {
    let host = SessionHost::new();
    host.register(Arc::new(TypstAdapter::new()))
        .expect("register");
    let open = host
        .open("😀 *b*\n\n= t\n", None, Some("main.typ"))
        .expect("open");
    let table = host
        .with(open.session, |s| s.blocks(0, 100))
        .expect("blocks");
    let strong = table
        .spans
        .iter()
        .find(|s| s.kind == kind::STRONG)
        .expect("strong");
    assert_eq!((strong.start, strong.end), (3, 6));
    let plan = host
        .with(open.session, |s| s.run_command("bold", 4, 5, ""))
        .expect("plan");
    assert_eq!(plan.changes.len(), 2, "unbold removes both stars");
}

// ---------------------------------------------------------------- proptests

fn token() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("text ".to_owned()),
        Just("\n".to_owned()),
        Just("\n\n".to_owned()),
        Just("= Head\n".to_owned()),
        Just("== H2 <l>\n".to_owned()),
        Just("- item\n".to_owned()),
        Just("  + sub\n".to_owned()),
        Just("*b*".to_owned()),
        Just("_i_".to_owned()),
        Just("`c`".to_owned()),
        Just("```rust\ncode\n```".to_owned()),
        Just("$x^2$".to_owned()),
        Just("$ a + b $".to_owned()),
        Just("@ref".to_owned()),
        Just("<lbl>".to_owned()),
        Just("#link(\"u\")[t]".to_owned()),
        Just("#image(\"p.png\")".to_owned()),
        Just("#figure(image(\"f.svg\"), caption: [c])".to_owned()),
        Just("#set text(1pt)\n".to_owned()),
        Just("#let x = [*a*]\n".to_owned()),
        Just("#box[*z*]".to_owned()),
        Just("// c\n".to_owned()),
        Just("/* c */".to_owned()),
        Just("é😀".to_owned()),
        Just("*".to_owned()),
        Just("_".to_owned()),
        Just("#".to_owned()),
        Just("$".to_owned()),
        Just("[".to_owned()),
        Just("]".to_owned()),
        Just("\"".to_owned()),
    ]
}

fn doc() -> impl Strategy<Value = String> {
    proptest::collection::vec(token(), 0..30).prop_map(|v| v.concat())
}

fn snap(text: &str, mut at: usize) -> usize {
    at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, ..ProptestConfig::default() })]

    /// Edited sessions describe what a fresh parse does (for valid markup).
    #[test]
    fn incremental_equals_fresh(
        init in doc(),
        edits in proptest::collection::vec((0usize..400, 0usize..30, doc()), 1..10),
    ) {
        let mut s = TypstSession::new(&init);
        let mut text = init;
        for (pos, del, ins) in edits {
            let start = snap(&text, pos % (text.len() + 1));
            let end = snap(&text, start + del);
            let dirty = s.edit(start..end, &ins).expect("valid edit");
            text.replace_range(start..end, &ins);
            prop_assert!(dirty.start <= dirty.end && dirty.end <= text.len());
            prop_assert_eq!(s.source.text(), text.as_str());
            // Typst's incremental reparse is exact for valid markup. For a
            // document with a syntax error somewhere it may keep an older
            // (still well-formed) recovery, so only valid documents are held
            // to equality; the tables stay valid either way (next test).
            let fresh = TypstSession::new(&text);
            if fresh.source.root().errors_and_warnings().0.is_empty() {
                prop_assert_eq!(dump(&s), dump(&fresh));
            }
        }
    }

    /// Host tables are in bounds, ordered and non-overlapping per layer.
    #[test]
    fn host_tables_are_valid(init in doc(), a in 0usize..300, b in 0usize..300) {
        let host = SessionHost::new();
        host.register(Arc::new(TypstAdapter::new())).expect("register");
        let open = host.open(&init, Some("typst"), None).expect("open");
        let (x, y) = (a.min(b), a.max(b));
        let table = host.with(open.session, |s| s.blocks(x, y)).expect("blocks");
        for (i, s) in table.spans.iter().enumerate() {
            prop_assert!(s.start < s.end && s.end <= table.doc_len);
            for q in &table.spans[..i] {
                prop_assert!(q.end <= s.start || s.end <= q.end);
                if q.layer == s.layer {
                    prop_assert!(q.end <= s.start);
                }
            }
        }
    }

    /// Commands return well-formed plans that leave text outside their edits alone.
    #[test]
    fn commands_only_touch_their_edits(
        init in doc(),
        a in 0usize..300,
        b in 0usize..300,
        cmd in prop_oneof![
            Just(("bold", "")), Just(("italic", "")), Just(("code", "")),
            Just(("heading", "2")), Just(("heading", "0")),
            Just(("list", "bullet")), Just(("list", "ordered")),
            Just(("link", "u")), Just(("image", "a.png")), Just(("figure", "f.svg")),
            Just(("math", "inline")), Just(("math", "block")),
        ],
    ) {
        let s = TypstSession::new(&init);
        let x = snap(&init, a % (init.len() + 1));
        let y = snap(&init, b % (init.len() + 1));
        let plan = s.run_command(cmd.0, x.min(y)..x.max(y), cmd.1).expect("plans");
        let mut prev = 0usize;
        let mut rebuilt = String::new();
        for e in &plan.edits {
            prop_assert!(e.range.start >= prev && e.range.end >= e.range.start && e.range.end <= init.len());
            rebuilt.push_str(&init[prev..e.range.start]);
            rebuilt.push_str(&e.replacement);
            prev = e.range.end;
        }
        rebuilt.push_str(&init[prev..]);
        let mut applied = init.clone();
        for e in plan.edits.iter().rev() {
            applied.replace_range(e.range.clone(), &e.replacement);
        }
        prop_assert_eq!(&applied, &rebuilt);
        if let Some(sel) = plan.selection {
            prop_assert!(sel.start <= sel.end && sel.end <= applied.len());
            prop_assert!(applied.is_char_boundary(sel.start) && applied.is_char_boundary(sel.end));
        }
    }
}
