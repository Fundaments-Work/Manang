//! A whole figure built only through the agent tools, as an AI agent would:
//! through `Kernel::call_agent_tool`, so every input is checked against the
//! declared schema first.

use std::fs;
use std::sync::Arc;

use manang_kernel::{AgentToolError, EventBus, Kernel, Payload};
use serde_json::{Value, json};

fn kernel(events: &EventBus) -> Kernel {
    let k = Kernel::new();
    k.register(
        manang_figure::manifest().expect("manifest"),
        Arc::new(manang_figure::plugin_with_events(events.clone())),
    )
    .expect("figure");
    k.register(
        manang_diagram::manifest().expect("manifest"),
        Arc::new(manang_diagram::plugin()),
    )
    .expect("diagram");
    k
}

fn call(k: &Kernel, tool: &str, input: Value) -> Value {
    match k.call_agent_tool(tool, input.clone()) {
        Ok(Payload::Json(v)) => v,
        other => panic!("{tool} {input}: {other:?}"),
    }
}

fn new_figure(k: &Kernel, dir: &std::path::Path) -> String {
    let path = dir.join("pipeline.figure.json");
    k.invoke(
        "figure.create",
        Payload::Json(json!({ "path": path.to_string_lossy(), "width": 960, "height": 540 })),
    )
    .expect("create");
    path.to_string_lossy().into_owned()
}

#[test]
fn every_figure_tool_is_declared_valid() {
    let k = kernel(&EventBus::new());
    let list = k.agent_tools();
    assert!(list.problems.is_empty(), "{:#?}", list.problems);
    let names: Vec<&str> = list.tools.iter().map(|t| t.tool.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "figure_add_shapes",
            "figure_connect",
            "figure_describe",
            "figure_export",
            "figure_from_mermaid",
            "figure_layout",
            "figure_remove",
            "figure_style",
            "figure_to_mermaid",
            "figure_update",
        ]
    );
}

#[test]
fn an_agent_builds_lays_out_exports_and_describes_a_flowchart() {
    let dir = tempfile::tempdir().expect("tmp");
    let events = EventBus::new();
    let changes = events.subscribe("figure.changed");
    let k = kernel(&events);
    let path = new_figure(&k, dir.path());

    let added = call(
        &k,
        "figure_add_shapes",
        json!({ "path": path, "shapes": [
            { "kind": "ellipse", "label": "Start" },
            { "label": "Load data" },
            { "kind": "diamond", "label": "Valid?" },
            { "label": "Clean data" },
            { "label": "Fit model" },
            { "kind": "ellipse", "label": "Report", "preset": "emphasis" }
        ] }),
    );
    assert_eq!(added["ids"], json!(["n1", "n2", "n3", "n4", "n5", "n6"]));

    let connected = call(
        &k,
        "figure_connect",
        json!({ "path": path, "connections": [
            { "from": "Start", "to": "load data" },
            { "from": "Load Data", "to": "Valid?" },
            { "from": "Valid?", "to": "Fit model", "label": "yes" },
            { "from": "Valid?", "to": "Clean data", "label": "no", "preset": "dashed" },
            { "from": "Clean data", "to": "Load data", "routing": "curved" },
            { "from": "n5", "to": "n6" }
        ] }),
    );
    assert_eq!(connected["ids"].as_array().map(Vec::len), Some(6));

    call(
        &k,
        "figure_layout",
        json!({ "path": path, "direction": "TB", "routing": "elbow" }),
    );
    call(
        &k,
        "figure_style",
        json!({ "path": path, "refs": ["Valid?"], "style": { "fill": "yellow" } }),
    );

    let exported = call(&k, "figure_export", json!({ "path": path }));
    let svg = fs::read_to_string(exported["svg"].as_str().expect("svg path")).expect("svg");
    assert!(svg.contains("Fit model"));
    let scene: Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("json")).expect("scene");
    assert_eq!(scene["version"], 2);
    assert_eq!(scene["nodes"].as_array().map(Vec::len), Some(12));

    let description = call(&k, "figure_describe", json!({ "path": path }));
    assert!(
        description["summary"]
            .as_str()
            .is_some_and(|s| s.contains("\"Valid?\" (n3) -> \"Fit model\" (n5) labelled \"yes\"")),
        "{}",
        description["summary"]
    );
    // Laid out top-down: Start above Load data above Valid?.
    let y = |id: &str| {
        description["shapes"]
            .as_array()
            .and_then(|s| s.iter().find(|s| s["id"] == id))
            .and_then(|s| s["y"].as_f64())
            .unwrap_or(f64::NAN)
    };
    assert!(y("n1") < y("n2") && y("n2") < y("n3"), "{description}");

    let mut snapshot = description.clone();
    snapshot["figure"]["path"] = json!("[path]");
    insta::assert_json_snapshot!("describe", snapshot);
    insta::assert_snapshot!("svg", svg);

    // Each tool call is one undoable step and was announced.
    assert_eq!(changes.drain().len(), 4);
    let mermaid = call(&k, "figure_to_mermaid", json!({ "path": path }));
    assert!(
        mermaid["source"]
            .as_str()
            .is_some_and(|s| s.contains("n3 -->|\"yes\"| n5"))
    );
}

#[test]
fn each_tool_call_is_one_undo_step() {
    let dir = tempfile::tempdir().expect("tmp");
    let k = kernel(&EventBus::new());
    let path = new_figure(&k, dir.path());
    call(
        &k,
        "figure_add_shapes",
        json!({ "path": path, "shapes": [{ "label": "A" }, { "label": "B" }] }),
    );
    call(
        &k,
        "figure_connect",
        json!({ "path": path, "connections": [{ "from": "A", "to": "B" }] }),
    );
    k.invoke("figure.apply", Payload::Json(json!({ "op": "undo" })))
        .expect("undo");
    let d = call(&k, "figure_describe", json!({ "path": path }));
    assert_eq!(d["shapes"].as_array().map(Vec::len), Some(2));
    assert_eq!(d["connections"].as_array().map(Vec::len), Some(0));
}

#[test]
fn mistakes_come_back_as_actionable_errors() {
    let dir = tempfile::tempdir().expect("tmp");
    let k = kernel(&EventBus::new());
    let path = new_figure(&k, dir.path());
    call(
        &k,
        "figure_add_shapes",
        json!({ "path": path, "shapes": [{ "label": "Start" }, { "label": "Start" }, { "label": "End" }] }),
    );

    // Schema problems, before the command runs.
    match k.call_agent_tool(
        "figure_add_shapes",
        json!({ "path": path, "shapes": [{ "kind": "hexagon", "colour": "red" }] }),
    ) {
        Err(AgentToolError::InvalidInput { issues, .. }) => {
            let paths: Vec<&str> = issues.iter().map(|i| i.path.as_str()).collect();
            assert!(
                paths.contains(&"/shapes/0/kind") && paths.contains(&"/shapes/0/colour"),
                "{issues:?}"
            );
        }
        other => panic!("{other:?}"),
    }
    let message = |input: Value, tool: &str| match k.call_agent_tool(tool, input) {
        Err(e) => e.to_string(),
        Ok(v) => panic!("expected an error, got {v:?}"),
    };
    let typo = message(
        json!({ "path": path, "connections": [{ "from": "Strat", "to": "End" }] }),
        "figure_connect",
    );
    assert!(
        typo.contains("[not_found]") && typo.contains("did you mean \"Start\""),
        "{typo}"
    );
    let twice = message(
        json!({ "path": path, "connections": [{ "from": "Start", "to": "End" }] }),
        "figure_connect",
    );
    assert!(
        twice.contains("[ambiguous]") && twice.contains("n1, n2"),
        "{twice}"
    );
    let colour = message(
        json!({ "path": path, "refs": ["End"], "style": { "stroke": "red" } }),
        "figure_style",
    );
    assert!(colour.contains("vermillion"), "{colour}");
    let nothing = message(
        json!({ "path": path, "updates": [{ "ref": "End" }] }),
        "figure_update",
    );
    assert!(nothing.contains("nothing to change"), "{nothing}");
}

#[test]
fn mermaid_becomes_shapes_or_a_block() {
    let dir = tempfile::tempdir().expect("tmp");
    let k = kernel(&EventBus::new());
    let path = new_figure(&k, dir.path());
    let flow = call(
        &k,
        "figure_from_mermaid",
        json!({ "path": path, "source": "flowchart LR\n  A[Collect] --> B{Enough?}\n  B -->|yes| C[Analyse]\n  B -.->|no| A\n", "x": 40, "y": 40 }),
    );
    assert_eq!(flow["mode"], "shapes");
    assert_eq!(flow["family"], "flowchart");
    let d = call(&k, "figure_describe", json!({ "path": path }));
    assert_eq!(d["shapes"].as_array().map(Vec::len), Some(3));
    assert!(
        d["summary"]
            .as_str()
            .is_some_and(|s| s.contains("\"Enough?\" (n2) -> \"Analyse\" (n3) labelled \"yes\"")),
        "{}",
        d["summary"]
    );

    // Round trip: back to Mermaid, the same graph.
    let back = call(&k, "figure_to_mermaid", json!({ "path": path }));
    let source = back["source"].as_str().unwrap_or_default();
    for line in [
        "n1[\"Collect\"]",
        "n2{\"Enough?\"}",
        "n2 -->|\"yes\"| n3",
        "n2 -.->|\"no\"| n1",
    ] {
        assert!(source.contains(line), "{line} in {source}");
    }

    let seq = call(
        &k,
        "figure_from_mermaid",
        json!({ "path": path, "source": "sequenceDiagram\n  Alice->>Bob: Hi\n" }),
    );
    assert_eq!(seq["family"], "sequence");
    let pie = call(
        &k,
        "figure_from_mermaid",
        json!({ "path": path, "source": "pie\n  \"a\" : 1\n" }),
    );
    assert_eq!(pie["mode"], "block");
    assert!(
        pie["fallback"]
            .as_str()
            .is_some_and(|f| f.contains("unsupported_layout"))
    );
    let strict = k.call_agent_tool(
        "figure_from_mermaid",
        json!({ "path": path, "source": "pie\n  \"a\" : 1\n", "mode": "shapes" }),
    );
    assert!(strict.is_err());

    // The block renders through the diagram plugin on export.
    let exported = call(&k, "figure_export", json!({ "path": path }));
    let svg = fs::read_to_string(exported["svg"].as_str().unwrap_or_default()).expect("svg");
    assert!(
        svg.contains("<svg x="),
        "the Mermaid block is embedded as a nested SVG"
    );
}

#[test]
fn frames_export_on_their_own() {
    let dir = tempfile::tempdir().expect("tmp");
    let k = kernel(&EventBus::new());
    let path = new_figure(&k, dir.path());
    call(
        &k,
        "figure_add_shapes",
        json!({ "path": path, "shapes": [
        { "kind": "frame", "label": "Panel A", "x": 0, "y": 0, "width": 300, "height": 200 },
        { "label": "inside", "x": 40, "y": 40 }
    ] }),
    );
    let out = call(
        &k,
        "figure_export",
        json!({ "path": path, "frame": "Panel A" }),
    );
    let frame_svg = out["frame_svg"].as_str().expect("frame svg");
    assert!(frame_svg.ends_with("pipeline-n1.svg"));
    let svg = fs::read_to_string(frame_svg).expect("reads");
    assert!(svg.contains("viewBox=\"0 0 300 200\""));
}
