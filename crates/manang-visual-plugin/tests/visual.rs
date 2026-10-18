//! The `visual` plugin driven through the kernel, the way the shell drives it.

use std::sync::Arc;

use manang_editor::{Editor, EditorPlugin};
use manang_kernel::{Kernel, KernelError, Payload, PluginError};
use manang_visual::SpanTable;
use manang_visual::kind;
use manang_visual_plugin::VisualPlugin;
use serde_json::{Value, json};

struct Rig {
    kernel: Arc<Kernel>,
    editor: Arc<Editor>,
}

fn rig() -> Rig {
    let editor = Arc::new(Editor::new());
    let kernel = Arc::new(Kernel::new());
    kernel
        .register(
            manang_editor::manifest().expect("editor manifest"),
            Arc::new(EditorPlugin::new(Arc::clone(&editor))),
        )
        .expect("register editor");
    let plugin = VisualPlugin::new(Arc::clone(&editor))
        .expect("pool")
        .with_adapter(Arc::new(manang_markdown::visual::MarkdownAdapter::new()))
        .expect("markdown")
        .with_adapter(Arc::new(manang_typst::visual::TypstAdapter::new()))
        .expect("typst");
    kernel
        .register(
            manang_visual_plugin::manifest().expect("visual manifest"),
            Arc::new(plugin),
        )
        .expect("register visual");
    Rig { kernel, editor }
}

fn call(rig: &Rig, command: &str, args: Value) -> Result<Value, String> {
    match rig.kernel.invoke(command, Payload::Json(args)) {
        Ok(Payload::Json(v)) => Ok(v),
        Ok(Payload::Empty) => Ok(Value::Null),
        Ok(Payload::Binary(_)) => panic!("{command} returned binary"),
        Err(KernelError::Plugin(PluginError::Failed(m))) => Err(m),
        Err(other) => Err(other.to_string()),
    }
}

fn blocks(rig: &Rig, session: &Value, from: usize, to: usize) -> SpanTable {
    let out = rig
        .kernel
        .invoke(
            "visual.blocks",
            Payload::Json(json!({ "session": session, "from": from, "to": to })),
        )
        .expect("visual.blocks");
    let Payload::Binary(bytes) = out else {
        panic!("visual.blocks must return raw bytes");
    };
    SpanTable::decode(&bytes).expect("a valid span table")
}

fn kinds(table: &SpanTable) -> Vec<u16> {
    table.spans.iter().map(|s| s.kind).collect()
}

#[test]
fn lists_the_adapters_and_registers_its_contributions() {
    let rig = rig();
    let adapters = call(&rig, "visual.adapters", json!({})).expect("adapters");
    let ids: Vec<&str> = adapters
        .as_array()
        .expect("array")
        .iter()
        .filter_map(|a| a["id"].as_str())
        .collect();
    assert_eq!(ids, ["markdown", "typst"]);
    assert_eq!(adapters[0]["extensions"], json!(["md", "markdown", "mdx"]));

    assert_eq!(rig.kernel.editors().len(), 2, "source and visual");
    let toolbar = rig.kernel.toolbars();
    assert_eq!(toolbar.len(), 1);
    for item in &toolbar[0].toolbar.items {
        assert!(rig.kernel.has_command(&item.command), "{}", item.command);
    }
}

#[test]
fn opens_an_editor_document_by_path_and_serves_a_binary_table() {
    let rig = rig();
    let dir = std::env::temp_dir().join(format!("manang-visual-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let file = dir.join("note.md");
    std::fs::write(
        &file,
        "# Title\n\nSome **bold** and [a link](https://x.org).\n",
    )
    .expect("write");
    let doc = rig.editor.open(&file).expect("editor.open");

    let open = call(&rig, "visual.open", json!({ "doc": doc })).expect("open by path");
    assert_eq!(open["adapter"], "markdown");
    assert_eq!(open["revision"], 0);
    assert_eq!(open["len"], 52);
    let session = &open["session"];

    let table = blocks(&rig, session, 0, 1000);
    assert_eq!(table.revision, 0);
    assert_eq!(table.doc_len, 52);
    let k = kinds(&table);
    assert!(k.contains(&kind::HEADING) && k.contains(&kind::STRONG) && k.contains(&kind::LINK));
    let link = table
        .spans
        .iter()
        .find(|s| s.kind == kind::LINK)
        .expect("link span");
    assert_eq!(table.attr_of(link), Some("https://x.org"));

    let outline = call(&rig, "visual.outline", json!({ "session": session })).expect("outline");
    assert_eq!(outline[0]["title"], "Title");
    assert_eq!(outline[0]["level"], 1);

    call(&rig, "visual.close", json!({ "session": session })).expect("close");
    let gone = call(&rig, "visual.outline", json!({ "session": session })).expect_err("closed");
    assert!(gone.starts_with("[visual.no-session]"), "{gone}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn typing_updates_the_table_and_reports_dirty_ranges() {
    let rig = rig();
    let open = call(
        &rig,
        "visual.open",
        json!({ "text": "= Head\n\nplain\n", "adapter": "typst" }),
    )
    .expect("open");
    let session = open["session"].clone();
    let before = blocks(&rig, &session, 0, 100);
    assert!(!kinds(&before).contains(&kind::STRONG));

    // Type `*bold*` at the end of "plain", one character per transaction.
    for (i, ch) in "*bold*".chars().enumerate() {
        let at = 13 + i;
        let len = 14 + i + 1;
        let out = call(
            &rig,
            "visual.edit",
            json!({ "session": session, "changes": [{ "from": at, "to": at, "insert": ch.to_string() }], "len": len }),
        )
        .expect("edit");
        assert_eq!(out["revision"], i + 1);
        assert_eq!(out["len"], len);
        assert!(out["dirty"]["to"].as_u64().expect("dirty") as usize >= at);
    }
    let after = blocks(&rig, &session, 0, 100);
    assert_eq!(after.revision, 6);
    assert!(kinds(&after).contains(&kind::STRONG));

    // A viewport slice holds only spans that intersect it.
    let slice = blocks(&rig, &session, 0, 3);
    assert!(kinds(&slice).contains(&kind::HEADING));
    assert!(!kinds(&slice).contains(&kind::STRONG));
}

#[test]
fn a_length_mismatch_is_a_desync_and_reset_recovers() {
    let rig = rig();
    let open = call(
        &rig,
        "visual.open",
        json!({ "text": "abc", "adapter": "markdown" }),
    )
    .expect("open");
    let session = open["session"].clone();
    let err = call(
        &rig,
        "visual.edit",
        json!({ "session": session, "changes": [{ "from": 0, "to": 0, "insert": "x" }], "len": 9 }),
    )
    .expect_err("desync");
    assert!(err.starts_with("[visual.desync]"), "{err}");

    let reset = call(
        &rig,
        "visual.reset",
        json!({ "session": session, "text": "xabc **d**" }),
    )
    .expect("reset");
    assert_eq!(reset["len"], 10);
    assert_eq!(reset["revision"], 1);
    assert!(kinds(&blocks(&rig, &session, 0, 100)).contains(&kind::STRONG));
}

#[test]
fn toolbar_commands_return_edit_plans_in_utf16() {
    let rig = rig();
    let open = call(
        &rig,
        "visual.open",
        json!({ "text": "😀 word", "adapter": "markdown" }),
    )
    .expect("open");
    let session = open["session"].clone();
    let plan = call(
        &rig,
        "visual.bold",
        json!({ "session": session, "from": 3, "to": 7 }),
    )
    .expect("visual.bold");
    assert_eq!(
        plan["changes"],
        json!([
            { "from": 3, "to": 3, "insert": "**" },
            { "from": 7, "to": 7, "insert": "**" },
        ])
    );
    assert_eq!(plan["selection"], json!({ "from": 5, "to": 9 }));

    let same = call(
        &rig,
        "visual.run-command",
        json!({ "session": session, "command": "bold", "from": 3, "to": 7 }),
    )
    .expect("run-command");
    assert_eq!(
        same, plan,
        "the toolbar command is run-command with a fixed command"
    );

    let heading = call(
        &rig,
        "visual.heading",
        json!({ "session": session, "from": 0, "to": 0, "args": "2" }),
    )
    .expect("heading");
    assert_eq!(heading["changes"][0]["insert"], "## ");

    let state = call(
        &rig,
        "visual.command-state",
        json!({ "session": session, "from": 3, "to": 7 }),
    )
    .expect("command-state");
    assert!(
        state
            .as_array()
            .expect("array")
            .iter()
            .any(|c| c["command"] == "bold" && c["enabled"] == true)
    );
}

#[test]
fn errors_carry_stable_codes() {
    let rig = rig();
    let code = |r: Result<Value, String>| r.expect_err("fails");
    assert!(
        code(call(
            &rig,
            "visual.open",
            json!({ "text": "x", "path": "a.xyz" })
        ))
        .starts_with("[visual.no-adapter]")
    );
    assert!(
        code(call(&rig, "visual.open", json!({ "text": "x" }))).starts_with("[visual.bad-args]")
    );
    assert!(
        code(call(&rig, "visual.open", json!({ "adapter": "markdown" })))
            .starts_with("[visual.bad-args]")
    );
    assert!(
        code(call(
            &rig,
            "visual.open",
            json!({ "doc": 99, "adapter": "markdown" })
        ))
        .starts_with("[visual.bad-args]")
    );
    assert!(
        code(call(
            &rig,
            "visual.open",
            json!({ "text": "x", "adapter": "markdown", "bogus": 1 })
        ))
        .starts_with("[visual.bad-args]")
    );
    let open = call(
        &rig,
        "visual.open",
        json!({ "text": "x", "adapter": "markdown" }),
    )
    .expect("open");
    let session = open["session"].clone();
    assert!(
        code(call(
            &rig,
            "visual.run-command",
            json!({ "session": session, "command": "figure", "from": 0, "to": 0 })
        ))
        .starts_with("[visual.unsupported]")
    );
    assert!(
        code(call(
            &rig,
            "visual.heading",
            json!({ "session": session, "from": 0, "to": 0, "args": "9" })
        ))
        .starts_with("[visual.bad-args]")
    );
    assert!(
        code(call(
            &rig,
            "visual.blocks",
            json!({ "session": 4242, "from": 0, "to": 1 })
        ))
        .contains("no-session")
    );
}

#[test]
fn sessions_run_in_parallel_without_interfering() {
    let rig = Arc::new(rig());
    let handles: Vec<_> = (0..8)
        .map(|n| {
            let rig = Arc::clone(&rig);
            std::thread::spawn(move || {
                let text = format!("# Doc {n}\n\n**strong {n}**\n");
                let len = text.encode_utf16().count();
                let open = call(
                    &rig,
                    "visual.open",
                    json!({ "text": text, "adapter": "markdown" }),
                )
                .expect("open");
                let session = open["session"].clone();
                for i in 0..50 {
                    call(
                        &rig,
                        "visual.edit",
                        json!({ "session": session,
                                "changes": [{ "from": 0, "to": 0, "insert": "x" }],
                                "len": len + i + 1 }),
                    )
                    .expect("edit");
                    let t = blocks(&rig, &session, 0, 10_000);
                    assert_eq!(t.doc_len as usize, len + i + 1);
                }
                let outline =
                    call(&rig, "visual.outline", json!({ "session": session })).expect("outline");
                // The first line is now "xxxx...# Doc n": not a heading any more.
                assert_eq!(outline, json!([]));
                call(&rig, "visual.close", json!({ "session": session })).expect("close");
            })
        })
        .collect();
    for h in handles {
        h.join().expect("thread");
    }
}
