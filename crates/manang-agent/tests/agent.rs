//! Kernel-driven integration tests: commands go through `Kernel::invoke`, events are read from
//! the bus, providers replay cassettes. The real project, search, citations and Typst plugins sit
//! beside the agent, on a temp project.

mod common;

use common::*;
use manang_agent::{AgentError, AgentOptions, AgentPlugin, Limits, SYSTEM_PROMPT};
use manang_agent_core::{
    ContentBlock, Message, ProviderEvent, StopReason, TranscriptEntry, read_all,
};
use manang_kernel::{EventBus, Kernel, Payload};
use manang_keystore::{ChainKeyStore, KeyCommands, SessionKeyStore};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

const PAPER: &str = "= Intro\nhello world. Bye @knuth84.\nSecond line.\n";
const BIB: &str = "@article{knuth84,\n  author = {Donald E. Knuth},\n  title = {Literate Programming},\n  year = {1984}\n}\n";

fn ok_edit(old: &str, new: &str) -> Value {
    json!({"path": "main.typ", "edits": [{"old": old, "new": new}], "note": "Mock note."})
}

// ---- prompt, header, turn context ------------------------------------------------------------

#[test]
fn a_simple_chat_streams_text_usage_and_done() {
    let (h, mock) = H::new(vec![says("Hello.")]);
    let sid = h.start("Say hello", json!({})).expect("start");
    let end = h.until_done();
    assert_eq!(end["stop_reason"], "end_turn");
    assert_eq!(h.streamed_text(), "Hello.");
    assert!(
        h.of("agent.text")
            .iter()
            .all(|d| d["session_id"] == sid.as_str())
    );
    // One usage event per author message, with the fill level and the cache ratio.
    let usages = h.of("agent.usage");
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0]["input"], 100);
    assert!(usages[0]["est_cost_micro_usd"].as_u64().is_some());
    assert_eq!(usages[0]["fill"], 100);
    assert_eq!(usages[0]["window"], 100_000);
    // The system prompt is the constant; the author's text reaches the model after the header.
    let req = &mock.requests()[0];
    assert_eq!(req.system.as_deref(), Some(SYSTEM_PROMPT));
    let first = req.messages[0].text();
    assert!(first.starts_with("<project name="), "{first}");
    assert!(first.contains("main.typ"), "{first}");
    assert!(first.ends_with("Say hello"), "{first}");
    // The sessions folder is git-ignored by a generated .gitignore.
    let ignore = std::fs::read_to_string(h.project.path().join(".manang/agent/.gitignore"))
        .expect("gitignore");
    assert_eq!(ignore, "*\n");
    assert!(h.session_dir(&sid).join("meta.toml").exists());
    let meta = std::fs::read_to_string(h.session_dir(&sid).join("meta.toml")).expect("meta");
    assert!(!meta.contains("feature"), "{meta}");
}

#[test]
fn the_header_comes_once_and_the_turn_context_follows_the_text() {
    let (h, mock) = H::new(vec![says("one"), says("two")]);
    h.write("refs.bib", BIB);
    h.write("MANANG.md", "Use British spelling.");
    h.write("AGENTS.md", "cargo build --release");
    let ctx = json!({
        "active": {"path": "main.typ", "line": 2},
        "selection": {"path": "main.typ", "text": "hello world", "line_from": 2, "line_to": 2},
        "mentions": [{"kind": "file", "path": "refs.bib"}],
    });
    let sid = h.start("Shorten this.", ctx).expect("start");
    h.until_done();
    let first = mock.requests()[0].messages[0].text();
    // Header: files and the author's instructions (AGENTS.md is not read by default).
    assert!(first.contains("refs.bib"), "{first}");
    assert!(first.contains("<instructions source=\"MANANG.md\">\nUse British spelling."));
    assert!(!first.contains("cargo build"), "{first}");
    // The author's words come first, then the data blocks.
    let text_at = first.find("Shorten this.").expect("text");
    let active_at = first
        .find("<active path=\"main.typ\" line=\"2\"/>")
        .expect("active");
    let selection_at = first
        .find("<selection path=\"main.typ\" lines=\"2-2\">\nhello world\n</selection>")
        .expect("selection");
    let mention_at = first.find("<file path=\"refs.bib\">").expect("mention");
    assert!(text_at < active_at && active_at < selection_at && selection_at < mention_at);
    assert!(
        first.contains("Literate Programming"),
        "the host read the mentioned file"
    );
    // The second message of the session carries no header.
    h.call(
        "agent.start",
        h.args(
            "And now?",
            json!({"active": {"path": "main.typ"}}),
            json!({"session_id": sid}),
        ),
    )
    .expect("resume");
    h.until_done();
    let second = &mock.requests()[1];
    assert_eq!(second.messages.len(), 3);
    let last = second.messages[2].text();
    assert!(!last.contains("<project"), "{last}");
    assert!(last.starts_with("And now?"), "{last}");
}

#[test]
fn the_prefix_is_constant_and_the_tool_list_is_small() {
    let (h, mock) = H::new(vec![says("a"), says("b")]);
    h.start("first", json!({})).expect("start");
    h.until_done();
    h.start("second", selection_ctx("main.typ", "x"))
        .expect("start");
    h.until_done();
    let (a, b) = (&mock.requests()[0], &mock.requests()[1]);
    assert_eq!(a.system, b.system);
    assert_eq!(
        serde_json::to_string(&a.tools).expect("json"),
        serde_json::to_string(&b.tools).expect("json"),
        "the tool definitions are byte-identical between sessions"
    );
    let names: Vec<&str> = a.tools.iter().map(|t| t.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted, "tools are sorted by name");
    for n in [
        "list_files",
        "read_file",
        "search",
        "outline",
        "diagnostics",
        "bib_search",
        "edit_text",
        "ask_user",
        "verify_citation",
    ] {
        assert!(names.contains(&n), "{n}");
    }
    assert!(
        !names.iter().any(|n| n.starts_with("figure_")),
        "figure tools stay out of documents"
    );
    let wire = serde_json::to_string(&a.tools).expect("json");
    for t in serde_json::from_str::<Vec<Value>>(&wire).expect("tools") {
        assert!(
            t.get("title").is_none() && t.get("output_schema").is_none(),
            "{t}"
        );
    }
    let tokens = manang_agent::estimate_tokens(&wire);
    assert!(tokens < 2_200, "{tokens} tokens of tool definitions");
}

// ---- the tools -----------------------------------------------------------------------------------

#[test]
fn the_read_tools_answer_in_plain_text() {
    let (h, mock) = H::new(vec![
        calls(vec![
            tool("t1", "list_files", json!({})),
            tool("t2", "read_file", json!({"path": "main.typ"})),
            tool("t3", "search", json!({"query": "BYE"})),
            tool("t4", "outline", json!({"path": "main.typ"})),
            tool("t5", "bib_search", json!({"query": "knuth"})),
            tool("t6", "diagnostics", json!({})),
            tool("t7", "read_file", json!({"path": "../etc/passwd"})),
            tool(
                "t8",
                "read_file",
                json!({"path": "main.typ", "from_line": 99}),
            ),
        ]),
        says("ok"),
    ]);
    h.write("main.typ", PAPER);
    h.write("chapters/deep.typ", "= Deep\nbody\n");
    h.load_bib("refs.bib", BIB);
    let ctx = json!({"diagnostics": [{"path": "main.typ", "line": 2, "column": 3, "severity": "error", "message": "unknown variable: x"}]});
    h.start("look around", ctx).expect("start");
    h.until_done();
    let r = results_in(&mock.requests()[1]);
    assert!(
        r[0].contains("chapters/\n  deep.typ")
            && r[0].contains("main.typ")
            && r[0].contains("files in the project"),
        "{}",
        r[0]
    );
    assert_eq!(
        r[1],
        "main.typ lines 1-3 of 3\n1| = Intro\n2| hello world. Bye @knuth84.\n3| Second line.\n"
    );
    assert!(
        r[2].starts_with("main.typ:2: hello world. Bye @knuth84.\n"),
        "{}",
        r[2]
    );
    assert!(r[2].contains("1 matches in 1 files."), "{}", r[2]);
    assert_eq!(r[3], "main.typ\nIntro (line 1)\n");
    assert_eq!(r[4], "knuth84 | Knuth (1984) | Literate Programming\n");
    assert_eq!(
        r[5],
        "error main.typ:2:3 unknown variable: x\n    1| = Intro\n    2| hello world. Bye @knuth84.\n"
    );
    assert!(
        r[6].contains("relative to the project") || r[6].contains("off limits"),
        "{}",
        r[6]
    );
    assert!(result_is_error(&mock.requests()[1], 6));
    assert!(
        r[7].contains("past the end: main.typ has 3 lines"),
        "{}",
        r[7]
    );
    // One updating row per call: running, then done (or error).
    let rows = h.of("agent.tool");
    assert_eq!(rows.len(), 16);
    assert_eq!(rows[0]["status"], "running");
    assert_eq!(rows[1]["status"], "done");
    assert_eq!(rows[1]["name"], "list_files");
}

#[test]
fn the_editor_text_wins_over_the_disk_and_long_files_are_paged() {
    let (h, mock) = H::new(vec![
        calls(vec![
            tool("t1", "read_file", json!({"path": "main.typ"})),
            tool("t2", "search", json!({"query": "unsaved"})),
            tool(
                "t3",
                "read_file",
                json!({"path": "long.md", "from_line": 395}),
            ),
        ]),
        says("ok"),
    ]);
    h.write("main.typ", PAPER);
    let long: String = (1..=500).map(|i| format!("row {i}\n")).collect();
    h.write("long.md", &long);
    h.start(
        "x",
        json!({"buffers": {"main.typ": "= Edited\nunsaved text\n"}}),
    )
    .expect("start");
    h.until_done();
    let r = results_in(&mock.requests()[1]);
    assert!(
        r[0].starts_with("main.typ lines 1-2 of 2 (editor text)\n1| = Edited\n"),
        "{}",
        r[0]
    );
    assert!(r[1].starts_with("main.typ:2: unsaved text\n"), "{}", r[1]);
    assert!(
        r[2].starts_with("long.md lines 395-500 of 500\n"),
        "{}",
        r[2]
    );
    // The default window is 400 lines and the result says how to continue.
    let (h2, mock2) = H::new(vec![
        calls(vec![tool("t1", "read_file", json!({"path": "long.md"}))]),
        says("ok"),
    ]);
    h2.write("long.md", &long);
    h2.start("x", json!({})).expect("start");
    h2.until_done();
    let r = results_in(&mock2.requests()[1]);
    assert!(
        r[0].starts_with("long.md lines 1-400 of 500. Continue with from_line 401\n"),
        "{}",
        r[0]
    );
}

#[test]
fn edit_text_queues_a_proposal_the_host_located_and_never_writes() {
    let (h, mock) = H::new(vec![
        calls(vec![tool(
            "t1",
            "edit_text",
            ok_edit("hello world", "Hello, world"),
        )]),
        says("Done."),
    ]);
    h.write("main.typ", PAPER);
    h.load_bib("refs.bib", BIB);
    h.start("/rewrite", selection_ctx("main.typ", "hello world"))
        .expect("start");
    h.until_done();
    let proposal = &h.of("agent.proposal")[0];
    assert_eq!(proposal["kind"], "text_edit");
    let p = &proposal["payload"];
    assert_eq!(p["path"], "main.typ");
    assert_eq!(p["note"], "Mock note.");
    let e = &p["edits"][0];
    // Offsets are the host's, in UTF-16 units of the text the model read.
    assert_eq!((e["from"].as_u64(), e["to"].as_u64()), (Some(8), Some(19)));
    assert_eq!(e["before"], "hello world");
    assert_eq!(e["after"], "Hello, world");
    assert_eq!(h.read("main.typ"), PAPER, "nothing was written");
    // The model sees a one-line receipt, not the diff.
    let id = proposal["proposal_id"].as_str().expect("id").to_owned();
    let r = results_in(&mock.requests()[1]);
    assert_eq!(
        r[0],
        format!(
            "Proposal {id} queued for main.typ: 1 edit, -11 +12 characters. Markup check passed."
        )
    );
    // Accepting returns the edit and is audited; deciding twice fails.
    let edit = h
        .call("agent.apply-proposal", json!({"proposal_id": id}))
        .expect("apply");
    assert_eq!(edit["edit"]["edits"][0]["after"], "Hello, world");
    let again = h
        .call("agent.reject-proposal", json!({"proposal_id": id}))
        .expect_err("decided");
    assert!(again.contains("[already_decided]"), "{again}");
}

#[test]
fn edit_text_errors_steer_the_model_and_a_retry_succeeds() {
    let (h, mock) = H::new(vec![
        calls(vec![
            tool("t1", "edit_text", ok_edit("hello wrold. Bye", "x")),
            tool("t2", "edit_text", ok_edit("line", "row")),
            tool("t3", "edit_text", ok_edit("2| Second", "x")),
            tool(
                "t4",
                "edit_text",
                json!({"path": "nope.typ", "edits": [{"old": "a", "new": "b"}]}),
            ),
        ]),
        calls(vec![tool(
            "t5",
            "edit_text",
            ok_edit("Second line.", "Second row."),
        )]),
        says("ok"),
    ]);
    h.write(
        "main.typ",
        "= Intro\nhello world. Bye @knuth84.\nSecond line.\nThird line.\n",
    );
    h.load_bib("refs.bib", BIB);
    h.start("fix", json!({})).expect("start");
    h.until_done();
    let first = &mock.requests()[1];
    let r = results_in(first);
    assert!(
        r[0].starts_with("old text not found in main.typ. Closest match at line 2:"),
        "{}",
        r[0]
    );
    assert!(r[0].contains("hello world. Bye @knuth84."), "{}", r[0]);
    assert!(
        r[1].contains("old text matches 2 places in main.typ (lines 3, 4)"),
        "{}",
        r[1]
    );
    assert!(r[2].contains("without the numbering"), "{}", r[2]);
    assert!(
        r[3].contains("Creating, deleting and renaming files is not possible"),
        "{}",
        r[3]
    );
    for i in 0..4 {
        assert!(result_is_error(first, i), "{i}");
    }
    assert_eq!(
        h.of("agent.proposal").len(),
        1,
        "only the corrected edit became a proposal"
    );
    assert!(results_in(&mock.requests()[2])[0].starts_with("Proposal "));
}

#[test]
fn the_markup_guard_protects_citations_labels_math_and_environments() {
    let (h, mock) = H::new(vec![
        calls(vec![
            tool("t1", "edit_text", ok_edit("Bye @knuth84.", "Goodbye.")),
            tool("t2", "edit_text", ok_edit("hello world", "hello {world")),
            tool(
                "t3",
                "edit_text",
                ok_edit("Second line.", "Second @nobody1999 line."),
            ),
            tool(
                "t4",
                "edit_text",
                ok_edit("Second line.", "Second @knuth84 line."),
            ),
            tool("t5", "edit_text", ok_edit("= Intro", "= $Intro")),
        ]),
        says("ok"),
    ]);
    h.write("main.typ", PAPER);
    h.load_bib("refs.bib", BIB);
    h.start("polish", json!({})).expect("start");
    h.until_done();
    let r = results_in(&mock.requests()[1]);
    assert!(
        r[0].contains("drops or changes a reference that must stay: knuth84")
            && r[0].contains("ask_user"),
        "{}",
        r[0]
    );
    assert!(r[1].contains("unbalances { and }"), "{}", r[1]);
    assert!(
        r[2].contains("`nobody1999`") && r[2].contains("bib_search"),
        "{}",
        r[2]
    );
    assert!(
        r[3].starts_with("Proposal "),
        "an existing key may be cited: {}",
        r[3]
    );
    assert!(r[4].contains("math delimiters"), "{}", r[4]);
    // The author may ask for the change: naming the key in the message lifts the guard for it.
    let (h2, mock2) = H::new(vec![
        calls(vec![tool(
            "t1",
            "edit_text",
            ok_edit("Bye @knuth84.", "Goodbye."),
        )]),
        says("ok"),
    ]);
    h2.write("main.typ", PAPER);
    h2.start("remove the citation @knuth84", json!({}))
        .expect("start");
    h2.until_done();
    assert!(results_in(&mock2.requests()[1])[0].starts_with("Proposal "));
    // LaTeX environments must stay balanced.
    let (h3, mock3) = H::new(vec![
        calls(vec![tool(
            "t1",
            "edit_text",
            json!({"path": "p.tex", "edits": [{"old": "\\end{itemize}", "new": ""}]}),
        )]),
        says("ok"),
    ]);
    h3.write("p.tex", "\\begin{itemize}\n\\item a\n\\end{itemize}\n");
    h3.start("x", json!({})).expect("start");
    h3.until_done();
    assert!(results_in(&mock3.requests()[1])[0].contains("unbalances \\begin and \\end (itemize)"));
}

#[test]
fn pending_proposals_overlay_what_the_model_reads_and_edits_next() {
    let (h, mock) = H::new(vec![
        calls(vec![tool(
            "t1",
            "edit_text",
            ok_edit("Second line.", "Second row."),
        )]),
        calls(vec![
            tool(
                "t2",
                "read_file",
                json!({"path": "main.typ", "from_line": 3}),
            ),
            tool("t3", "search", json!({"query": "row"})),
            tool("t4", "edit_text", ok_edit("Second row.", "Second column.")),
        ]),
        says("ok"),
    ]);
    h.write("main.typ", PAPER);
    h.load_bib("refs.bib", BIB);
    h.start("x", json!({})).expect("start");
    h.until_done();
    let r = results_in(&mock.requests()[2]);
    assert!(
        r[0].starts_with(
            "main.typ lines 3-3 of 3 (with your pending edits applied)\n3| Second row."
        ),
        "{}",
        r[0]
    );
    assert!(r[1].starts_with("main.typ:3: Second row."), "{}", r[1]);
    assert!(
        r[2].starts_with("Proposal "),
        "the second edit was located against the first: {}",
        r[2]
    );
    // Rejecting a proposal removes it from the overlay.
    let (h2, mock2) = H::new(vec![
        calls(vec![tool(
            "t1",
            "edit_text",
            ok_edit("Second line.", "Second row."),
        )]),
        says("ok"),
        calls(vec![tool("t2", "read_file", json!({"path": "main.typ"}))]),
        says("done"),
    ]);
    h2.write("main.typ", PAPER);
    let sid = h2.start("x", json!({})).expect("start");
    h2.until_done();
    let id = h2.of("agent.proposal")[0]["proposal_id"]
        .as_str()
        .expect("id")
        .to_owned();
    h2.call("agent.reject-proposal", json!({"proposal_id": id}))
        .expect("reject");
    h2.call(
        "agent.start",
        h2.args("again", json!({}), json!({"session_id": sid})),
    )
    .expect("resume");
    h2.until_done();
    let read = &results_in(&mock2.requests()[3])[0];
    assert!(read.starts_with("main.typ lines 1-3 of 3\n"), "{read}");
    assert!(
        read.contains("Second line.") && !read.contains("pending"),
        "{read}"
    );
}

#[test]
fn diagnostics_with_pending_checks_the_project_as_if_the_edit_were_accepted() {
    let (h, mock) = H::new(vec![
        calls(vec![
            tool("t0", "diagnostics", json!({"with_pending": true})),
            tool("t1", "edit_text", ok_edit("#figur(\"x\")", "#upper(\"x\")")),
        ]),
        calls(vec![tool(
            "t2",
            "diagnostics",
            json!({"with_pending": true}),
        )]),
        says("ok"),
    ]);
    h.write("main.typ", "= T\n#figur(\"x\")\n");
    h.start("fix it", json!({"active": {"path": "main.typ"}}))
        .expect("start");
    h.until_done();
    let before = results_in(&mock.requests()[1]);
    assert!(
        before[0].contains("With 0 pending edit(s) applied: 1 problem(s), before: 1. No change"),
        "{}",
        before[0]
    );
    assert!(before[0].contains("error main.typ:2:"), "{}", before[0]);
    assert!(
        before[0].contains("unknown variable: figur"),
        "{}",
        before[0]
    );
    let after = results_in(&mock.requests()[2]);
    assert!(
        after[0].starts_with(
            "With 1 pending edit(s) applied: 0 problem(s), before: 1. Fewer problems."
        ),
        "{}",
        after[0]
    );
    assert_eq!(h.read("main.typ"), "= T\n#figur(\"x\")\n");
}

#[test]
fn ask_user_ends_the_turn_and_shows_the_question() {
    let (h, mock) = H::new(vec![calls(vec![tool(
        "t1",
        "ask_user",
        json!({"question": "Delete section 3?", "choices": ["Yes", "No"]}),
    )])]);
    h.start("Delete section 3 and renumber", json!({}))
        .expect("start");
    let end = h.until_done();
    assert_eq!(end["stop_reason"], "ask_user");
    assert_eq!(mock.requests().len(), 1, "the model is not asked again");
    let row = h
        .of("agent.tool")
        .into_iter()
        .find(|r| r["status"] == "done")
        .expect("done row");
    assert_eq!(row["ask"]["question"], "Delete section 3?");
    assert_eq!(row["ask"]["choices"], json!(["Yes", "No"]));
}

#[test]
fn an_identical_read_in_one_message_is_answered_with_a_note() {
    let (h, mock) = H::new(vec![
        calls(vec![tool("t1", "read_file", json!({"path": "main.typ"}))]),
        calls(vec![tool("t2", "read_file", json!({"path": "main.typ"}))]),
        says("ok"),
    ]);
    h.write("main.typ", PAPER);
    h.start("x", json!({})).expect("start");
    h.until_done();
    assert!(results_in(&mock.requests()[1])[0].starts_with("main.typ lines 1-3 of 3"));
    assert_eq!(
        results_in(&mock.requests()[2])[0],
        "Same as the earlier result (unchanged)."
    );
}

// ---- approvals and the tool boundary ---------------------------------------------------------------

#[test]
fn a_network_tool_asks_and_a_denial_reaches_the_model() {
    let (h, mock) = H::new(vec![
        calls(vec![tool(
            "n1",
            "net_fetch",
            json!({"url": "https://example.org"}),
        )]),
        says("ok"),
    ]);
    h.start("fetch", json!({})).expect("start");
    let ask = h.wait_for(|t, _| t == "agent.approval");
    assert_eq!(ask["tool"], "net_fetch");
    assert_eq!(ask["risk"], "network");
    h.call(
        "agent.approve",
        json!({"call_id": ask["call_id"], "decision": "deny"}),
    )
    .expect("approve");
    h.until_done();
    assert!(results_in(&mock.requests()[1])[0].contains("The author declined this action"));
}

#[test]
fn after_untrusted_content_a_network_call_asks_again_even_if_the_session_allowed_it() {
    let (h, _mock) = H::new(vec![
        calls(vec![tool(
            "n1",
            "net_fetch",
            json!({"url": "https://a.test"}),
        )]),
        calls(vec![tool(
            "n2",
            "net_fetch",
            json!({"url": "https://b.test"}),
        )]),
        says("ok"),
    ]);
    h.start("fetch two", json!({})).expect("start");
    let first = h.wait_for(|t, _| t == "agent.approval");
    assert_eq!(first["after_untrusted_content"], false);
    h.call(
        "agent.approve",
        json!({"call_id": first["call_id"], "decision": "allow_session"}),
    )
    .expect("approve");
    let second = h.wait_for(|t, _| t == "agent.approval");
    assert_eq!(second["after_untrusted_content"], true);
    h.call(
        "agent.approve",
        json!({"call_id": second["call_id"], "decision": "allow"}),
    )
    .expect("approve");
    assert_eq!(h.until_done()["stop_reason"], "end_turn");
}

#[test]
fn verify_citation_names_its_host_and_runs_after_approval() {
    let (h, mock) = H::new(vec![
        calls(vec![tool(
            "v1",
            "verify_citation",
            json!({"doi": "10.1000/real"}),
        )]),
        says("ok"),
    ]);
    h.http.set_json(
        "https://api.crossref.org/works/10.1000/real",
        200,
        &json!({"status": "ok", "message": {
            "title": ["A Real Paper"], "author": [{"family": "Roe", "given": "R."}],
            "issued": {"date-parts": [[2001]]}, "DOI": "10.1000/real"}}),
    );
    h.start("is this real?", json!({})).expect("start");
    let ask = h.wait_for(|t, _| t == "agent.approval");
    assert_eq!(ask["tool"], "verify_citation");
    assert_eq!(ask["host"], "api.crossref.org");
    assert_eq!(ask["scope"], "network:api.crossref.org");
    assert!(
        h.http.requests().is_empty(),
        "nothing was contacted before the answer"
    );
    h.call(
        "agent.approve",
        json!({"call_id": ask["call_id"], "decision": "allow"}),
    )
    .expect("approve");
    h.until_done();
    assert!(results_in(&mock.requests()[1])[0].starts_with("Verified:"));
    assert_eq!(
        h.http.requests(),
        vec!["https://api.crossref.org/works/10.1000/real"]
    );
}

#[test]
fn an_unknown_tool_name_is_an_error_result_not_a_crash() {
    let (h, mock) = H::new(vec![
        calls(vec![tool("x1", "rm_rf", json!({}))]),
        says("ok"),
    ]);
    h.start("x", json!({})).expect("start");
    h.until_done();
    assert!(results_in(&mock.requests()[1])[0].contains("There is no tool named `rm_rf`"));
    assert!(result_is_error(&mock.requests()[1], 0));
}

#[test]
fn a_bad_argument_shape_is_reported_with_its_path() {
    let (h, mock) = H::new(vec![
        calls(vec![tool("t1", "edit_text", json!({"edits": []}))]),
        says("ok"),
    ]);
    h.start("x", json!({})).expect("start");
    h.until_done();
    let r = results_in(&mock.requests()[1]);
    assert!(r[0].starts_with("Invalid arguments:"), "{}", r[0]);
    assert!(
        r[0].contains("Fix them and call the tool again."),
        "{}",
        r[0]
    );
}

#[test]
fn figure_tools_exist_only_inside_the_figure_studio_and_edits_ask_first() {
    let (h, mock) = H::new(vec![
        says("no tools here"),
        calls(vec![tool(
            "f1",
            "figure_add_shapes",
            json!({"shapes": [{"label": "A"}]}),
        )]),
        says("ok"),
    ]);
    h.kernel
        .register(
            manang_figure::manifest().expect("m"),
            Arc::new(manang_figure::plugin()),
        )
        .expect("figure");
    h.call("figure.create", json!({})).expect("create");
    h.start("hi", json!({})).expect("start");
    h.until_done();
    assert!(
        !mock.requests()[0]
            .tools
            .iter()
            .any(|t| t.name.starts_with("figure_"))
    );
    h.start("add a box", json!({"view": "figure"}))
        .expect("start");
    let ask = h.wait_for(|t, _| t == "agent.approval");
    assert!(
        mock.requests()[1]
            .tools
            .iter()
            .any(|t| t.name == "figure_add_shapes")
    );
    assert_eq!(ask["scope"], "write");
    h.call(
        "agent.approve",
        json!({"call_id": ask["call_id"], "decision": "allow"}),
    )
    .expect("approve");
    h.until_done();
}

// ---- cancel, retry, errors ------------------------------------------------------------------------

#[test]
fn cancel_stops_a_stream_and_reports_it_in_done() {
    let h = H::with(
        Arc::new(Hang(vec![started(), text("partial")])),
        Limits {
            flush: Duration::from_millis(5),
            ..Limits::default()
        },
    );
    let sid = h.start("x", json!({})).expect("start");
    h.wait_for(|t, _| t == "agent.text");
    let out = h
        .call("agent.cancel", json!({"session_id": sid}))
        .expect("cancel");
    assert_eq!(out["cancelled"], true);
    let end = h.until_done();
    assert_eq!(end["stop_reason"], "cancelled");
    assert_eq!(end["cancelled"], true);
    assert!(
        !h.topics().contains(&"agent.cancelled".to_owned()),
        "cancellation is a field of done"
    );
    assert!(
        h.call(
            "agent.transcript",
            json!({"session_id": sid, "project_root": h.root()})
        )
        .is_ok()
    );
}

#[test]
fn cancelling_mid_tool_leaves_a_valid_history_and_the_next_turn_works() {
    let (h, mock) = H::new(vec![
        calls(vec![
            tool("n1", "net_fetch", json!({"url": "https://a.test"})),
            tool("r1", "read_file", json!({"path": "main.typ"})),
        ]),
        says("fine"),
    ]);
    h.write("main.typ", PAPER);
    let sid = h.start("fetch and read", json!({})).expect("start");
    h.wait_for(|t, _| t == "agent.approval");
    h.call("agent.cancel", json!({"session_id": sid}))
        .expect("cancel");
    assert_eq!(h.until_done()["stop_reason"], "cancelled");
    // Every tool call in the transcript has its result, even the one that never ran.
    let history = h
        .call(
            "agent.transcript",
            json!({"session_id": sid, "project_root": h.root()}),
        )
        .expect("transcript");
    let messages: Vec<Message> =
        serde_json::from_value(history["messages"].clone()).expect("messages");
    let uses: Vec<String> = messages
        .iter()
        .flat_map(|m| {
            m.content
                .iter()
                .filter_map(ContentBlock::as_tool_use)
                .map(|u| u.id)
        })
        .collect();
    let results = messages.last().expect("last").content.len();
    assert_eq!(uses.len(), 2);
    assert_eq!(results, 2, "no orphan tool_use");
    h.call(
        "agent.start",
        h.args("go on", json!({}), json!({"session_id": sid})),
    )
    .expect("resume");
    assert_eq!(h.until_done()["stop_reason"], "end_turn");
    assert_eq!(mock.requests().len(), 2);
    assert!(results_in(&mock.requests()[1]).is_empty() || mock.requests()[1].messages.len() >= 4);
}

#[test]
fn a_connection_error_can_be_retried_without_repeating_the_message() {
    let (h, mock) = H::new(vec![
        vec![
            started(),
            ProviderEvent::Error {
                error: manang_agent_core::ProviderError {
                    code: "overloaded_error".into(),
                    message: "busy".into(),
                    retryable: true,
                    request_id: None,
                },
            },
        ],
        says("here"),
    ]);
    let sid = h.start("question", json!({})).expect("start");
    assert_eq!(h.until_done()["stop_reason"], "error");
    assert_eq!(h.of("agent.error")[0]["retryable"], true);
    h.call(
        "agent.start",
        h.args("", json!({}), json!({"session_id": sid, "retry": true})),
    )
    .expect("retry");
    assert_eq!(h.until_done()["stop_reason"], "end_turn");
    let again = &mock.requests()[1];
    assert_eq!(
        again.messages.len(),
        1,
        "the author's message was not added twice"
    );
    assert_eq!(mock.requests()[0].messages, again.messages);
    // There is nothing to retry after an answer.
    let err = h
        .call(
            "agent.start",
            h.args("", json!({}), json!({"session_id": sid, "retry": true})),
        )
        .expect_err("nothing to retry");
    assert!(err.contains("[invalid_argument]"), "{err}");
}

#[test]
fn a_provider_error_is_reported_with_its_request_id() {
    let (h, _m) = H::new(vec![vec![
        started(),
        ProviderEvent::Error {
            error: manang_agent_core::ProviderError {
                code: "authentication_error".into(),
                message: "bad".into(),
                retryable: false,
                request_id: Some("req_9".into()),
            },
        },
    ]]);
    h.start("x", json!({})).expect("start");
    let end = h.until_done();
    assert_eq!(end["stop_reason"], "error");
    let err = &h.of("agent.error")[0];
    assert_eq!(err["code"], "authentication_error");
    assert_eq!(err["request_id"], "req_9");
    assert!(
        err["message"]
            .as_str()
            .expect("m")
            .contains("rejected the key")
    );
}

#[test]
fn the_round_cap_ends_a_looping_model_with_a_visible_stop() {
    let looping = calls(vec![tool("t", "read_file", json!({"path": "a.md"}))]);
    let (h, mock) = H::new(vec![
        looping.clone(),
        looping.clone(),
        looping.clone(),
        looping,
    ]);
    h.write("a.md", "# A\n");
    h.start_with("loop", json!({}), json!({"options": {"max_iterations": 2}}))
        .expect("start");
    assert_eq!(h.until_done()["stop_reason"], "iteration_cap");
    assert_eq!(mock.requests().len(), 2);
}

#[test]
fn reasoning_is_a_signal_not_text_unless_the_developer_flag_is_on() {
    let turn = vec![
        started(),
        ProviderEvent::ThinkingDelta {
            text: "secret plan".into(),
        },
        text("Answer."),
        usage(),
        done(StopReason::EndTurn),
    ];
    let (h, _m) = H::new(vec![turn.clone(), turn]);
    h.start("x", json!({})).expect("start");
    h.until_done();
    let signal = h
        .of("agent.text")
        .into_iter()
        .find(|d| d["thinking"] == true)
        .expect("signal");
    assert_eq!(signal["text"], "");
    assert!(
        !serde_json::to_string(&*h.seen.borrow())
            .expect("json")
            .contains("secret plan")
    );
    h.call(
        "agent.configure",
        json!({"values": {"agent.include-thinking": true}}),
    )
    .expect("flag");
    h.start("y", json!({})).expect("start");
    h.until_done();
    assert!(
        h.of("agent.text")
            .iter()
            .any(|d| d["thinking"] == true && d["text"] == "secret plan")
    );
}

// ---- slash commands and effort -----------------------------------------------------------------------

#[test]
fn slash_commands_are_the_first_message_and_set_the_reasoning_effort_once() {
    let (h, mock) = H::new(vec![says("a"), says("b"), says("c")]);
    h.write(
        ".manang/commands/abstract.md",
        "---\ndescription: Draft an abstract\n---\nWrite an abstract. $ARGUMENTS",
    );
    let list = h
        .call("agent.commands", json!({"project_root": h.root()}))
        .expect("commands");
    let names: Vec<&str> = list["commands"]
        .as_array()
        .expect("list")
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert_eq!(
        names,
        [
            "abstract",
            "cite",
            "explain",
            "fix",
            "rewrite",
            "shorten",
            "translate"
        ]
    );
    assert_eq!(list["commands"][0]["description"], "Draft an abstract");
    // A command expands to the author's message; effort is low for commands, high for chat.
    let sid = h
        .start_with(
            "German",
            selection_ctx("main.typ", "hello"),
            json!({"command": "translate"}),
        )
        .expect("start");
    h.until_done();
    assert!(
        mock.requests()[0].messages[0]
            .text()
            .contains("Translate the selected text into German. Keep markup")
    );
    assert_eq!(h.specs.lock()[0].effort.as_deref(), Some("low"));
    // The effort is fixed for the session: a plain message afterwards keeps it.
    h.call(
        "agent.start",
        h.args("thanks", json!({}), json!({"session_id": sid})),
    )
    .expect("resume");
    h.until_done();
    assert_eq!(h.specs.lock()[1].effort.as_deref(), Some("low"));
    h.start("a chat", json!({})).expect("start");
    h.until_done();
    assert_eq!(h.specs.lock()[2].effort.as_deref(), Some("high"));
    // An unknown command is an error, an empty message too.
    let err = h
        .start_with("", json!({}), json!({"command": "nonsense"}))
        .expect_err("unknown");
    assert!(err.contains("[unknown_command]"), "{err}");
    assert!(
        h.start("  ", json!({}))
            .expect_err("empty")
            .contains("write a message")
    );
}

#[test]
fn the_effort_setting_overrides_auto() {
    let (h, _m) = H::new(vec![says("a")]);
    h.call(
        "agent.configure",
        json!({"values": {"agent.reasoning-effort": "high"}}),
    )
    .expect("configure");
    h.start_with("x", json!({}), json!({"command": "rewrite"}))
        .expect("start");
    h.until_done();
    assert_eq!(h.specs.lock()[0].effort.as_deref(), Some("high"));
    let err = h
        .call(
            "agent.configure",
            json!({"values": {"agent.reasoning-effort": "max"}}),
        )
        .expect_err("unknown effort");
    assert!(err.contains("[invalid_argument]"), "{err}");
}

// ---- context management ----------------------------------------------------------------------------

fn small_window(h: &H) {
    h.call(
        "agent.configure",
        json!({"values": {"agent.context-window": 8000}}),
    )
    .expect("window");
}

#[test]
fn stage_one_clears_old_tool_results_once_the_window_is_60_percent_full() {
    let reads: Vec<Vec<ProviderEvent>> = (1..=5)
        .map(|i| {
            let mut turn = calls(vec![tool(
                &format!("t{i}"),
                "read_file",
                json!({"path": "main.typ", "from_line": i}),
            )]);
            turn.insert(1, usage_of(5000, 10));
            turn
        })
        .chain([says("done")])
        .collect();
    let (h, mock) = H::new(reads);
    small_window(&h);
    h.write("main.typ", "a\nb\nc\nd\ne\nf\ng\nh\n");
    let sid = h.start("read a lot", json!({})).expect("start");
    h.until_done();
    // Before the fifth result exists there is nothing old enough to clear; then one batch runs.
    let last = &mock.requests()[5];
    let stubs: Vec<String> = last
        .messages
        .iter()
        .flat_map(|m| m.content.iter().map(|b| b.raw_json().to_owned()))
        .filter(|j| j.contains("cleared]"))
        .collect();
    assert_eq!(stubs.len(), 1, "{stubs:?}");
    assert!(
        stubs[0].contains("[read_file main.typ 1-end: cleared]"),
        "{}",
        stubs[0]
    );
    // The four newest results are whole, and the transcript on disk is lossless.
    let whole = last
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter(|b| b.raw_json().contains("main.typ lines "))
        .count();
    assert_eq!(whole, 4);
    let transcript =
        std::fs::read_to_string(h.session_dir(&sid).join("transcript.jsonl")).expect("transcript");
    assert!(!transcript.contains("cleared]"));
    assert_eq!(h.of("agent.usage")[0]["compacted"], "cleared");
}

#[test]
fn stage_two_summarises_everything_before_the_last_two_turns() {
    let big = |t: &str| {
        vec![
            started(),
            text(t),
            usage_of(7000, 20),
            done(StopReason::EndTurn),
        ]
    };
    let (h, mock) = H::new(vec![
        big("answer one"),
        big("answer two"),
        // The summarising call (no tools), then the real answer.
        vec![
            started(),
            text("Task: polish the paper. Files touched: main.typ."),
            usage_of(900, 40),
            done(StopReason::EndTurn),
        ],
        says("answer three"),
    ]);
    small_window(&h);
    let sid = h.start("first task", json!({})).expect("start");
    h.until_done();
    h.call(
        "agent.start",
        h.args("second task", json!({}), json!({"session_id": sid})),
    )
    .expect("resume");
    h.until_done();
    h.call(
        "agent.start",
        h.args("third task", json!({}), json!({"session_id": sid})),
    )
    .expect("resume");
    h.until_done();
    let requests = mock.requests();
    assert_eq!(requests.len(), 4);
    let summarising = &requests[2];
    assert!(summarising.tools.is_empty());
    assert_ne!(summarising.system.as_deref(), Some(SYSTEM_PROMPT));
    assert!(summarising.messages[0].text().contains("first task"));
    let after = &requests[3];
    assert_eq!(
        after.system.as_deref(),
        Some(SYSTEM_PROMPT),
        "the prefix is unchanged"
    );
    assert_eq!(
        after.messages.len(),
        5,
        "summary, ack, then the last two turns"
    );
    let head = after.messages[0].text();
    assert!(head.starts_with("<project name="), "{head}");
    assert!(head.contains("Task: polish the paper."), "{head}");
    assert!(after.messages[2].text().starts_with("second task"));
    assert!(after.messages[4].text().starts_with("third task"));
    assert_eq!(
        h.of("agent.usage").last().expect("usage")["compacted"],
        "summarised"
    );
    // The transcript keeps every message.
    let text =
        std::fs::read_to_string(h.session_dir(&sid).join("transcript.jsonl")).expect("transcript");
    let log = read_all::<TranscriptEntry>(&text).expect("parse");
    assert_eq!(log.entries.len(), 6);
    assert!(text.contains("first task"));
}

// ---- policy, consent, privacy ------------------------------------------------------------------------

#[test]
fn policy_and_settings_switch_everything_off_with_disabled() {
    let (h, _m) = H::new(vec![]);
    h.call(
        "agent.configure",
        json!({"values": {"agent.enabled": false}}),
    )
    .expect("configure");
    for (cmd, args) in [
        ("agent.start", json!({"prompt": "x"})),
        ("agent.preview-request", json!({"prompt": "x"})),
        ("agent.test-connection", json!({})),
    ] {
        let err = h.call(cmd, args).expect_err("disabled");
        assert!(err.contains("[disabled]"), "{cmd}: {err}");
    }
    // A project's policy can only tighten: it switches off, it cannot switch on.
    h.call(
        "agent.configure",
        json!({"values": {"agent.enabled": true}}),
    )
    .expect("configure");
    let dir = h.project.path().join(".manang/agent");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("policy.toml"), "enabled = false\n").expect("policy");
    assert!(
        h.start("x", json!({}))
            .expect_err("project policy")
            .contains("[disabled]")
    );
    std::fs::write(dir.join("policy.toml"), "enabled = true\nmystery = 1\n").expect("policy");
    assert!(
        h.start("x", json!({}))
            .expect_err("invalid")
            .contains("[disabled]")
    );
    std::fs::write(
        dir.join("policy.toml"),
        "allowed_providers = [\"anthropic\"]\n",
    )
    .expect("policy");
    assert!(
        h.start("x", json!({}))
            .expect_err("provider")
            .contains("[policy]")
    );
}

#[test]
fn consent_is_required_in_rust_before_anything_is_sent() {
    let (h, mock) = H::new(vec![says("x")]);
    h.call(
        "agent.consent",
        json!({"provider": "deepseek", "granted": false}),
    )
    .expect("revoke");
    let err = h.start("x", json!({})).expect_err("consent");
    assert!(err.contains("[consent_required]"), "{err}");
    assert!(mock.requests().is_empty());
    let status = h
        .call("agent.status", json!({"project_root": h.root()}))
        .expect("status");
    assert_eq!(status["ready"], false);
    assert!(
        status["problems"]
            .as_array()
            .expect("p")
            .iter()
            .any(|p| p == "consent_required")
    );
    assert!(
        status["consent_text"]
            .as_str()
            .expect("t")
            .contains("DeepSeek")
    );
}

#[test]
fn offline_mode_blocks_a_non_loopback_base_url_but_allows_localhost() {
    let (h, mock) = H::new(vec![says("x")]);
    h.call(
        "agent.configure",
        json!({"values": {"agent.offline": true}}),
    )
    .expect("configure");
    assert!(
        h.start("x", json!({}))
            .expect_err("offline")
            .contains("[offline_blocked]")
    );
    h.call(
        "agent.configure",
        json!({"values": {"agent.base-url": "http://203.0.113.5:8080/v1"}}),
    )
    .expect("configure");
    assert!(
        h.start("x", json!({}))
            .expect_err("ip")
            .contains("[offline_blocked]")
    );
    h.call(
        "agent.configure",
        json!({"values": {"agent.base-url": "http://localhost:11434/v1"}}),
    )
    .expect("configure");
    h.start("x", json!({})).expect("loopback is allowed");
    h.until_done();
    assert_eq!(mock.requests().len(), 1);
}

#[test]
fn the_transcript_replays_thinking_blocks_byte_exact() {
    let thinking = r#"{"type":"thinking","thinking":"plan  it","signature":"EqQBCgIYAhIM=="}"#;
    let block = ContentBlock::from_json_str(thinking).expect("block");
    let (h, mock) = H::new(vec![
        vec![
            started(),
            ProviderEvent::Block { block },
            tool("t1", "outline", json!({"path": "main.typ"})),
            done(StopReason::ToolUse),
        ],
        vec![text("done"), done(StopReason::EndTurn)],
    ]);
    let sid = h
        .start(
            "outline",
            json!({"buffers": {"main.typ": "= One\n== Two\n"}}),
        )
        .expect("start");
    h.until_done();
    let replayed = &mock.requests()[1].messages[1].content[0];
    assert_eq!(replayed.raw_json(), thinking);
    let text =
        std::fs::read_to_string(h.session_dir(&sid).join("transcript.jsonl")).expect("transcript");
    assert!(text.contains(thinking), "{text}");
    let log = read_all::<TranscriptEntry>(&text).expect("parse");
    assert_eq!(log.entries.len(), 4);
    assert_eq!(log.entries[1].message.content[0].raw_json(), thinking);
    let out = h
        .call(
            "agent.transcript",
            json!({"session_id": sid, "project_root": h.root()}),
        )
        .expect("transcript");
    assert_eq!(out["messages"].as_array().map(Vec::len), Some(4));
    // The outline tool saw the editor's text, not the disk.
    let result = &results_in(&mock.requests()[1])[0];
    assert!(
        result.contains("One") && result.contains("  Two"),
        "{result}"
    );
}

#[test]
fn secrets_are_redacted_before_sending_and_the_preview_shows_the_diff() {
    // A syntactically realistic but fake key, split so secret scanning does not
    // flag the fixture. The redaction path still sees the assembled value.
    let secret = concat!("sk-", "abcdefghijklmnopqrstuvwxyz0123456789ABCD");
    let (h, mock) = H::new(vec![says("ok")]);
    let ctx = selection_ctx("a.typ", &format!("token = {secret} end"));
    let preview = h
        .call(
            "agent.preview-request",
            h.args("check", ctx.clone(), json!({})),
        )
        .expect("preview");
    let body = preview["body"].as_str().expect("body");
    assert!(
        !body.contains(secret),
        "the preview is the redacted payload"
    );
    assert!(body.contains("[REDACTED:secret]"));
    assert_eq!(preview["redaction_count"], 1);
    let part = preview["parts"]
        .as_array()
        .expect("parts")
        .iter()
        .find(|p| p["kind"] == "selection")
        .expect("selection part");
    assert_eq!(part["redactions"].as_array().map(Vec::len), Some(1));
    assert!(
        part["original"]
            .as_str()
            .expect("original")
            .contains(secret),
        "the diff shows what was removed"
    );
    assert!(part["tokens"].as_u64().expect("tokens") > 0);
    assert_eq!(preview["cost"]["estimate"], true);
    assert!(mock.requests().is_empty(), "previewing sends nothing");
    h.start("check", ctx).expect("start");
    h.until_done();
    let sent = serde_json::to_string(&mock.requests()[0]).expect("json");
    assert!(!sent.contains(secret));
    assert_eq!(sent, body, "preview and request carry identical bytes");
}

#[test]
fn a_project_limit_refuses_oversized_context_before_sending() {
    let (h, mock) = H::new(vec![]);
    let dir = h.project.path().join(".manang/agent");
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join("policy.toml"), "max_context_tokens = 1000\n").expect("policy");
    let big = "word ".repeat(3000);
    let err = h
        .start("x", selection_ctx("a.typ", &big))
        .expect_err("too big");
    assert!(err.contains("[context_too_large]"), "{err}");
    assert!(mock.requests().is_empty());
    // A long selection is cut at 4k tokens and says how to read the rest.
    let (h2, mock2) = H::new(vec![says("ok")]);
    h2.start("x", selection_ctx("a.typ", &"w".repeat(30_000)))
        .expect("start");
    h2.until_done();
    assert!(
        mock2.requests()[0].messages[0]
            .text()
            .contains("selection truncated, 16000 more characters; use read_file")
    );
}

#[test]
fn the_key_never_appears_in_events_transcript_audit_errors_or_status() {
    let (h, _m) = H::new(vec![
        vec![
            started(),
            text("a"),
            tool("t1", "outline", json!({"path": "a.md"})),
            usage(),
            done(StopReason::ToolUse),
        ],
        says("b"),
    ]);
    let sid = h
        .start("hi", json!({"buffers": {"a.md": "# A"}}))
        .expect("start");
    h.until_done();
    let mut all = h.all_output(&sid);
    for (cmd, args) in [
        ("agent.key.status", json!({"provider": "deepseek"})),
        ("agent.status", json!({"project_root": h.root()})),
        ("agent.sessions", json!({"project_root": h.root()})),
        ("agent.models", json!({"provider": "deepseek"})),
        ("agent.preview-request", h.args("x", json!({}), json!({}))),
    ] {
        all.push_str(&serde_json::to_string(&h.call(cmd, args).expect("ok")).expect("json"));
    }
    for (cmd, args) in [
        ("agent.key.set", json!({"provider": "deepseek", "key": ""})),
        ("agent.start", json!({"feature": "nope"})),
        ("agent.cancel", json!({"session_id": "zzz"})),
    ] {
        all.push_str(&h.call(cmd, args).expect_err("fails"));
    }
    all.push_str(
        &std::fs::read_to_string(h.config.path().join("agent/usage.jsonl")).expect("ledger"),
    );
    assert!(!all.contains(KEY), "the key leaked");
    assert!(!all.contains("must-never-leak"));
}

#[test]
fn sessions_list_resume_and_reject_path_like_ids() {
    let (h, mock) = H::new(vec![says("one"), says("two")]);
    let sid = h.start("first question", json!({})).expect("start");
    h.until_done();
    let list = h
        .call("agent.sessions", json!({"project_root": h.root()}))
        .expect("sessions");
    assert_eq!(list["sessions"][0]["id"], sid.as_str());
    assert_eq!(list["sessions"][0]["version"], 1);
    assert_eq!(list["sessions"][0]["title"], "first question");
    let out = h
        .call(
            "agent.start",
            h.args("more", json!({}), json!({"session_id": sid})),
        )
        .expect("resume");
    assert_eq!(out["session_id"], sid.as_str());
    h.until_done();
    assert_eq!(mock.requests()[1].messages.len(), 3, "append-only history");
    for bad in ["../etc", "a/b", ""] {
        let err = h
            .call("agent.transcript", json!({"session_id": bad}))
            .expect_err("bad id");
        assert!(err.contains("[invalid_argument]"), "{bad}: {err}");
    }
    // A session folder written by an older version (with a `feature` field) still loads.
    let old = h.session_dir("sold-1");
    std::fs::create_dir_all(&old).expect("dir");
    std::fs::write(
        old.join("meta.toml"),
        "version = 1\nid = \"sold-1\"\ncreated_ms = 1\nupdated_ms = 2\nfeature = \"rewrite\"\nprovider = \"deepseek\"\nmodel = \"m\"\ntitle = \"Old\"\nstate = \"idle\"\nturns = 1\n",
    )
    .expect("meta");
    let list = h
        .call("agent.sessions", json!({"project_root": h.root()}))
        .expect("sessions");
    assert!(
        list["sessions"]
            .as_array()
            .expect("list")
            .iter()
            .any(|s| s["id"] == "sold-1")
    );
}

#[test]
fn settings_are_validated_all_or_nothing() {
    let (h, _m) = H::new(vec![]);
    let err = h
        .call(
            "agent.configure",
            json!({"values": {"agent.offline": true, "agent.provider": "nonsense"}}),
        )
        .expect_err("invalid");
    assert!(err.contains("[invalid_argument]"), "{err}");
    assert_eq!(
        h.call("agent.status", json!({})).expect("status")["offline"],
        false,
        "nothing was applied"
    );
    assert!(
        h.call(
            "agent.configure",
            json!({"values": {"agent.base-url": "ftp://x"}})
        )
        .is_err()
    );
    assert!(
        h.call(
            "agent.configure",
            json!({"values": {"agent.context-window": 100}})
        )
        .is_err()
    );
    h.call(
        "agent.configure",
        json!({"values": {"agent.context-window": 50000}}),
    )
    .expect("ok");
    assert_eq!(
        h.call("agent.status", json!({})).expect("status")["context_window"],
        50000
    );
}

#[test]
fn the_agent_tools_are_declared_and_valid_for_the_kernel() {
    let (h, _m) = H::new(vec![]);
    let list = h.kernel.agent_tools();
    assert!(list.problems.is_empty(), "{:?}", list.problems);
    let names: Vec<&str> = list.tools.iter().map(|t| t.tool.name.as_str()).collect();
    for n in [
        "list_files",
        "read_file",
        "search",
        "outline",
        "diagnostics",
        "bib_search",
        "edit_text",
        "ask_user",
        "verify_citation",
    ] {
        assert!(names.contains(&n), "{n}");
    }
    for old in [
        "doc_read",
        "doc_outline",
        "doc_diagnostics",
        "doc_propose_edit",
        "project_search",
    ] {
        assert!(!names.contains(&old), "{old} is gone");
    }
}

// ---- the real provider path against a local server ------------------------------------------------------

/// Serves one SSE answer on a free loopback port; the request text comes back on the channel.
fn one_shot_server() -> (u16, std::sync::mpsc::Receiver<String>) {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let mut length = 0usize;
            let mut head_end = None;
            loop {
                let n = stream.read(&mut chunk).unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                if head_end.is_none()
                    && let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n")
                {
                    head_end = Some(p + 4);
                    let head = String::from_utf8_lossy(&buf[..p]).to_lowercase();
                    length = head
                        .lines()
                        .find_map(|l| {
                            l.strip_prefix("content-length:")
                                .and_then(|v| v.trim().parse().ok())
                        })
                        .unwrap_or(0);
                }
                if let Some(h) = head_end
                    && buf.len() >= h + length
                {
                    break;
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
            let body = "data: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\ndata: {\"id\":\"c1\",\"object\":\"chat.completion.chunk\",\"model\":\"m\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" there\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2,\"total_tokens\":12}}\n\ndata: [DONE]\n\n";
            let reply = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nx-request-id: req_real\r\nconnection: close\r\ncontent-length: {}\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    (port, rx)
}

/// A bare kernel with the agent only, pointed at the local server.
fn real_provider_run(provider: &str, model: &str, start: Value) -> (String, String, String) {
    let (port, rx) = one_shot_server();
    let config = tempfile::tempdir().expect("config");
    let project = tempfile::tempdir().expect("project");
    let events = EventBus::new();
    let sub = events.subscribe("agent.*");
    let mut options = AgentOptions::new(config.path().to_path_buf(), events);
    options.keys = Arc::new(KeyCommands::new(Arc::new(ChainKeyStore::new(vec![
        Box::new(SessionKeyStore::new()),
    ]))));
    let kernel = Arc::new(Kernel::new());
    let _plugin = AgentPlugin::new(options)
        .expect("plugin")
        .register(&kernel)
        .expect("register");
    let call = |c: &str, a: Value| match kernel.invoke(c, Payload::Json(a)) {
        Ok(Payload::Json(v)) => Ok(v),
        Ok(_) => Ok(Value::Null),
        Err(e) => Err(e.to_string()),
    };
    call(
        "agent.configure",
        json!({"values": {
            "agent.provider": provider, "agent.model": model, "agent.offline": true,
            "agent.base-url": format!("http://127.0.0.1:{port}/v1")}}),
    )
    .expect("configure");
    call("agent.key.set", json!({"provider": provider, "key": KEY})).expect("key");
    call("agent.consent", json!({"provider": provider})).expect("consent");
    let mut args = json!({"project_root": project.path().to_string_lossy()});
    if let (Some(map), Value::Object(more)) = (args.as_object_mut(), start) {
        map.extend(more);
    }
    let out = call("agent.start", args).expect("start");
    let sid = out["session_id"].as_str().unwrap_or_default().to_owned();
    let mut text = String::new();
    let mut all = String::new();
    loop {
        let e = sub.recv_timeout(Duration::from_secs(20)).expect("event");
        let Payload::Json(p) = e.payload else {
            continue;
        };
        all.push_str(&p.to_string());
        if e.topic == "agent.text" {
            text.push_str(p["text"].as_str().unwrap_or(""));
        }
        assert_ne!(e.topic, "agent.error", "{p}");
        if e.topic == "agent.done" {
            assert_eq!(p["stop_reason"], "end_turn");
            break;
        }
    }
    let request = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("server saw the request");
    let dir = project.path().join(".manang/agent/sessions").join(&sid);
    for f in ["transcript.jsonl", "audit.jsonl"] {
        all.push_str(&std::fs::read_to_string(dir.join(f)).expect("file"));
    }
    (text, request, all)
}

#[test]
fn the_rig_provider_streams_from_a_local_openai_compatible_server() {
    let (text, request, all) = real_provider_run("custom", "m", json!({"prompt": "hi"}));
    assert_eq!(text, "Hello there");
    assert!(
        request
            .to_lowercase()
            .contains(&format!("authorization: bearer {}", KEY.to_lowercase())),
        "the key goes to the provider only"
    );
    assert!(!all.contains(KEY));
    assert!(all.contains("req_real"), "the request id is recorded");
    // The constant prefix is what travels: the system prompt and the tool names, no titles.
    assert!(request.contains("You are the writing assistant inside Manang"));
    assert!(request.contains("\"name\":\"edit_text\""));
    assert!(!request.contains("output_schema"));
    assert!(
        !request.contains("reasoning_effort"),
        "only providers that take it get it"
    );
}

#[test]
fn deepseek_gets_a_reasoning_effort_that_follows_the_kind_of_request() {
    let (_t, chat, _a) = real_provider_run("deepseek", "deepseek-flash", json!({"prompt": "hi"}));
    assert!(chat.contains("\"reasoning_effort\":\"high\""), "{chat}");
    let (_t, edit, _a) = real_provider_run(
        "deepseek",
        "deepseek-flash",
        json!({"prompt": "", "command": "rewrite"}),
    );
    assert!(edit.contains("\"reasoning_effort\":\"low\""), "{edit}");
}

#[test]
fn proptest_markup_guard_accepts_identity() {
    use proptest::prelude::*;
    proptest!(|(s in "[a-z @$\\\\{}<>:.,-]{0,60}")| {
        prop_assert!(manang_agent::markup_problem(&s, &s).is_none());
    });
}

#[test]
fn utf16_helpers_match_the_editor() {
    assert_eq!(manang_agent::utf16_len("a😀b"), 4);
    assert_eq!(manang_agent::estimate_tokens("abcdefg"), 2);
    let outline = manang_agent::outline("x.tex", "\\section{Intro}\n\\subsection*{Deep}\n");
    assert_eq!(outline.len(), 2);
    assert_eq!(outline[1].level, 3);
    let _ = AgentError::invalid("x");
}
