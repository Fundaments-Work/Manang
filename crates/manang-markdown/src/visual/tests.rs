use super::*;
use manang_visual::SessionHost;
use manang_visual::text::map_through;
use proptest::prelude::*;
use std::sync::Arc;

const SAMPLE: &str = "---\ntitle: Demo\n---\n\n# Heading *one*\n\nSome **bold**, _em_ and `code` with $x^2$ and ~~gone~~.\nA [link](https://example.org) and ![fig](a/b.png).\n\n- first\n- [x] done\n  - nested\n\n1. one\n2. two\n\n> quoted **text**\n> second\n\n```rust\nlet x = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n$$\nE = mc^2\n$$\n\n---\n\n<div>raw</div>\n";

/// All spans of a session, as `layer kind[aux] "text" attr` lines.
fn dump_session(s: &MarkdownSession) -> Vec<String> {
    let mut raw = Vec::new();
    let _ = s.spans(0..s.text.len(), &mut raw);
    sorted(raw, &s.text)
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
    insta::assert_snapshot!(dump_session(&MarkdownSession::new(SAMPLE)).join("\n"));
}

#[test]
fn outline_has_titles_and_levels() {
    let s = MarkdownSession::new("# One\n\ntext\n\n## Two ##\n\nSetext\n---\n");
    let o = s.outline();
    let got: Vec<_> = o.iter().map(|h| (h.level, h.title.as_str())).collect();
    assert_eq!(got, vec![(1, "One"), (2, "Two"), (2, "Setext")]);
}

/// Apply a plan to a text the way the editor would, returning the new text and
/// the selected text.
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
    let s = MarkdownSession::new(text);
    let plan = s.run_command(command, sel, args).expect("command plans");
    apply(text, &plan)
}

fn after(text: &str, sel: Range<usize>, command: &str, args: &str) -> String {
    run(text, sel, command, args).0
}

#[test]
fn inline_toggles() {
    assert_eq!(after("a word b", 2..6, "bold", ""), "a **word** b");
    assert_eq!(after("a word b", 3..3, "italic", ""), "a *word* b");
    assert_eq!(after("a **word** b", 4..8, "bold", ""), "a word b");
    assert_eq!(after("a **word** b", 7..7, "bold", ""), "a word b");
    assert_eq!(after("a word b", 2..6, "code", ""), "a `word` b");
    assert_eq!(after("a `word` b", 4..4, "code", ""), "a word b");
    assert_eq!(after("a word b", 2..6, "strike", ""), "a ~~word~~ b");
    assert_eq!(after("x $a$ y", 3..3, "math", ""), "x a y");
    assert_eq!(after("x a y", 2..3, "math", ""), "x $a$ y");
    assert_eq!(after("", 0..0, "bold", ""), "****");
    assert_eq!(run("", 0..0, "bold", "").1, "");
}

#[test]
fn italic_inside_bold_nests() {
    assert_eq!(after("**word**", 2..6, "italic", ""), "***word***");
    assert_eq!(after("***word***", 3..7, "italic", ""), "**word**");
}

#[test]
fn headings_toggle_and_replace() {
    assert_eq!(after("title", 0..0, "heading", "2"), "## title");
    assert_eq!(after("## title", 3..3, "heading", "2"), "title");
    assert_eq!(after("## title", 3..3, "heading", "1"), "# title");
    assert_eq!(after("# title", 0..0, "heading", "0"), "title");
    assert_eq!(after("a\nb\n", 0..3, "heading", "3"), "### a\n### b\n");
    assert!(
        MarkdownSession::new("x")
            .run_command("heading", 0..0, "9")
            .is_err()
    );
}

#[test]
fn lists_and_quotes() {
    assert_eq!(after("a\nb", 0..3, "list", "bullet"), "- a\n- b");
    assert_eq!(after("- a\n- b", 0..7, "list", "bullet"), "a\nb");
    assert_eq!(after("- a\n- b", 0..7, "list", "ordered"), "1. a\n2. b");
    assert_eq!(after("1. a\n2. b", 0..9, "list", "ordered"), "a\nb");
    assert_eq!(after("  a", 2..2, "list", ""), "  - a");
    assert_eq!(after("a\nb", 0..3, "quote", ""), "> a\n> b");
    assert_eq!(after("> a\n> b", 0..7, "quote", ""), "a\nb");
}

#[test]
fn links_images_rules() {
    assert_eq!(
        after("see docs", 4..8, "link", "https://x.org"),
        "see [docs](https://x.org)"
    );
    assert_eq!(after("see docs", 4..8, "link", ""), "see [docs]()");
    assert_eq!(run("see docs", 4..8, "link", "").1, "");
    assert_eq!(
        after("a [b](u) c", 4..4, "link", ""),
        "a b c",
        "a second press removes the link"
    );
    assert_eq!(
        after("x ", 2..2, "link", "u"),
        "x [link](u)",
        "caret in empty space inserts a placeholder"
    );
    assert_eq!(after("", 0..0, "image", "figs/a.png"), "![a](figs/a.png)");
    assert_eq!(after("cap", 0..3, "image", "a.png"), "![cap](a.png)");
    assert_eq!(after("para", 4..4, "rule", ""), "para\n\n---\n");
    assert_eq!(after("", 0..0, "math", "block"), "$$\n\n$$");
    assert_eq!(after("e=mc", 0..4, "math", "block"), "$$\ne=mc\n$$");
}

#[test]
fn unknown_command_is_unsupported() {
    let s = MarkdownSession::new("x");
    assert!(matches!(
        s.run_command("figure", 0..0, ""),
        Err(AdapterError::Unsupported(_))
    ));
}

#[test]
fn command_state_reports_active_formats() {
    let t = "**b** and # not\n\n## h\n\n- i\n";
    let s = MarkdownSession::new(t);
    let get = |sel: Range<usize>, c: &str| {
        s.command_state(sel)
            .into_iter()
            .find(|x| x.command == c)
            .map(|x| (x.enabled, x.active))
    };
    assert_eq!(get(3..3, "bold"), Some((true, true)));
    assert_eq!(get(10..10, "bold"), Some((true, false)));
    let h = t.find("## h").unwrap_or(0);
    assert_eq!(get(h + 4..h + 4, "heading"), Some((true, true)));
    let l = t.find("- i").unwrap_or(0);
    assert_eq!(get(l + 3..l + 3, "list"), Some((true, true)));
}

#[test]
fn code_blocks_disable_inline_wrappers() {
    let t = "```\ncode\n```\n";
    let s = MarkdownSession::new(t);
    let st = s.command_state(5..5);
    assert!(st.iter().any(|c| c.command == "bold" && !c.enabled));
}

#[test]
fn incremental_edit_equals_a_fresh_parse_simple_cases() {
    let mut s = MarkdownSession::new(SAMPLE);
    let at = SAMPLE.find("first").unwrap_or(0);
    s.edit(at..at, "very ").expect("edit");
    let want = {
        let mut t = SAMPLE.to_owned();
        t.insert_str(at, "very ");
        t
    };
    assert_eq!(s.text, want);
    assert_eq!(dump_session(&s), dump_session(&MarkdownSession::new(&want)));
    // Close the fence early: later blocks must be re-read as code.
    let fence = want.find("```rust").unwrap_or(0);
    let mut s2 = MarkdownSession::new(&want);
    s2.edit(fence..fence + 7, "text").expect("edit");
    let mut want2 = want.clone();
    want2.replace_range(fence..fence + 7, "text");
    assert_eq!(
        dump_session(&s2),
        dump_session(&MarkdownSession::new(&want2))
    );
}

#[test]
fn opening_a_fence_swallows_what_follows() {
    let t = "para\n\n# h\n\nmore *x*\n\nend\n";
    let mut s = MarkdownSession::new(t);
    s.edit(0..0, "```\n").expect("edit");
    let want = format!("```\n{t}");
    assert_eq!(dump_session(&s), dump_session(&MarkdownSession::new(&want)));
}

#[test]
fn indented_blocks_keep_their_indentation_in_a_window() {
    let t = "intro\n\n    # not a heading\n\n- a\n\n  continued\n\nend\n";
    for at in 0..t.len() {
        let mut s = MarkdownSession::new(t);
        s.edit(at..at, "").expect("edit");
        assert_eq!(
            dump_session(&s),
            dump_session(&MarkdownSession::new(t)),
            "at {at}"
        );
    }
}

#[test]
fn reference_links_stay_source_text() {
    let t = "[a][r] and [r]\n\n[r]: https://x.org\n";
    let spans = dump_session(&MarkdownSession::new(t));
    assert!(spans.iter().all(|s| !s.contains("link[")), "{spans:?}");
    let mut s = MarkdownSession::new(t);
    let at = t.find("https").unwrap_or(0);
    s.edit(at..at + 5, "ftp").expect("edit");
    assert_eq!(s.text, t.replacen("https", "ftp", 1));
}

#[test]
fn rejects_edits_outside_the_text() {
    let mut s = MarkdownSession::new("héllo");
    assert!(s.edit(0..99, "").is_err());
    assert!(s.edit(2..2, "x").is_err(), "inside the two byte é");
}

#[test]
fn works_through_the_host_with_utf16_offsets() {
    let host = SessionHost::new();
    host.register(Arc::new(MarkdownAdapter::new()))
        .expect("register");
    let open = host
        .open("😀 **b**\n\n# t\n", None, Some("notes.md"))
        .expect("open");
    let table = host
        .with(open.session, |s| s.blocks(0, 100))
        .expect("blocks");
    let strong = table
        .spans
        .iter()
        .find(|s| s.kind == kind::STRONG)
        .expect("strong");
    assert_eq!((strong.start, strong.end), (3, 8));
    assert_eq!(
        table
            .spans
            .iter()
            .filter(|s| s.kind == kind::MARKER)
            .count(),
        3
    );
    // Typing inside the heading in UTF-16 units reports a dirty range.
    let out = host
        .with(open.session, |s| {
            s.edit(
                &[manang_visual::Change {
                    from: 13,
                    to: 13,
                    insert: "x".into(),
                }],
                15,
            )
        })
        .expect("edit");
    assert!(out.dirty.is_some());
    let plan = host
        .with(open.session, |s| s.run_command("bold", 3, 8, ""))
        .expect("plan");
    assert_eq!(plan.changes.len(), 2, "unbold removes both delimiters");
}

#[test]
fn the_manifest_declares_exactly_what_the_adapter_implements() {
    let manifest = crate::manifest().expect("manifest");
    let spec = manifest
        .contributes
        .format_adapters
        .iter()
        .find(|a| a.id == "markdown")
        .expect("the manifest contributes the adapter");
    let adapter = MarkdownAdapter::new();
    let info = adapter.info();
    assert_eq!(spec.id, info.id);
    assert_eq!(spec.title, info.title);
    assert_eq!(spec.extensions, info.extensions);
    assert_eq!(spec.kinds, info.kinds);
    assert_eq!(spec.commands, info.commands);
    // Every declared command is implemented, and the session offers them all.
    let s = adapter.open("x").expect("open");
    let states: Vec<String> = s
        .command_state(0..0)
        .into_iter()
        .map(|c| c.command)
        .collect();
    for c in &spec.commands {
        assert!(states.contains(c), "`{c}` has no command state");
    }
}

// ---------------------------------------------------------------- proptests

fn token() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("text ".to_owned()),
        Just("\n".to_owned()),
        Just("\n\n".to_owned()),
        Just("# Head\n".to_owned()),
        Just("## H2\n".to_owned()),
        Just("- item\n".to_owned()),
        Just("  - sub\n".to_owned()),
        Just("1. one\n".to_owned()),
        Just("> quote\n".to_owned()),
        Just("```\n".to_owned()),
        Just("```rust\n".to_owned()),
        Just("**b**".to_owned()),
        Just("*i*".to_owned()),
        Just("`c`".to_owned()),
        Just("[a](u)".to_owned()),
        Just("![i](p.png)".to_owned()),
        Just("$x$".to_owned()),
        Just("---\n".to_owned()),
        Just("| a | b |\n|---|---|\n| 1 | 2 |\n".to_owned()),
        Just("é😀".to_owned()),
        Just("[r]: http://x\n".to_owned()),
        Just("[r]".to_owned()),
        Just("---\na: b\n---\n".to_owned()),
        Just("<div>\n".to_owned()),
        Just("    indented\n".to_owned()),
        Just("  ".to_owned()),
        Just("- a\n\n  b\n".to_owned()),
        Just("A\n===\n".to_owned()),
        Just("*".to_owned()),
        Just("#".to_owned()),
        Just(" ".to_owned()),
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
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    /// The heart of the adapter: after any sequence of edits, the incrementally
    /// maintained block list describes exactly what a fresh parse does.
    #[test]
    fn incremental_equals_fresh(
        init in doc(),
        edits in proptest::collection::vec((0usize..400, 0usize..30, doc()), 1..10),
    ) {
        let mut s = MarkdownSession::new(&init);
        let mut text = init;
        for (pos, del, ins) in edits {
            let start = snap(&text, pos % (text.len() + 1));
            let end = snap(&text, start + del);
            s.edit(start..end, &ins).expect("valid edit");
            text.replace_range(start..end, &ins);
            prop_assert_eq!(&s.text, &text);
            prop_assert_eq!(dump_session(&s), dump_session(&MarkdownSession::new(&text)));
        }
    }

    /// Spans from the host are in bounds, ordered and non-overlapping per layer.
    #[test]
    fn host_tables_are_valid(init in doc()) {
        let host = SessionHost::new();
        host.register(Arc::new(MarkdownAdapter::new())).expect("register");
        let open = host.open(&init, Some("markdown"), None).expect("open");
        let table = host.with(open.session, |s| s.blocks(0, usize::MAX)).expect("blocks");
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

    /// Round trip: applying any command's edits leaves bytes outside the
    /// edited ranges untouched and never panics.
    #[test]
    fn commands_only_touch_their_edits(
        init in doc(),
        a in 0usize..300,
        b in 0usize..300,
        cmd in prop_oneof![
            Just(("bold", "")), Just(("italic", "")), Just(("code", "")),
            Just(("strike", "")), Just(("heading", "2")), Just(("heading", "0")),
            Just(("list", "bullet")), Just(("list", "ordered")), Just(("quote", "")),
            Just(("link", "u")), Just(("image", "a.png")), Just(("math", "inline")),
            Just(("rule", "")),
        ],
    ) {
        let s = MarkdownSession::new(&init);
        let x = snap(&init, a % (init.len() + 1));
        let y = snap(&init, b % (init.len() + 1));
        let sel = x.min(y)..x.max(y);
        let plan = s.run_command(cmd.0, sel, cmd.1).expect("plans");
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
        prop_assert_eq!(applied, rebuilt);
        if let Some(sel) = plan.selection {
            let mut after = init.clone();
            for e in plan.edits.iter().rev() {
                after.replace_range(e.range.clone(), &e.replacement);
            }
            prop_assert!(sel.end <= after.len() && sel.start <= sel.end);
            prop_assert!(after.is_char_boundary(sel.start) && after.is_char_boundary(sel.end));
            let _ = map_through(&plan.edits, 0);
        }
    }
}
