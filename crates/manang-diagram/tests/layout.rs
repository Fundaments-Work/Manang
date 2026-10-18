//! `diagram.layout`: merman's geometry joined with labels and shapes, the
//! input of the figure studio's Mermaid-to-shapes conversion.

use std::sync::Arc;

use manang_diagram::{manifest, plugin};
use manang_kernel::{Kernel, Payload};
use serde_json::{Value, json};

fn layout(source: &str) -> Result<Value, String> {
    let k = Kernel::new();
    k.register(manifest().expect("manifest"), Arc::new(plugin()))
        .expect("registers");
    match k.invoke("diagram.layout", Payload::Json(json!({ "source": source }))) {
        Ok(Payload::Json(v)) => Ok(v),
        Ok(other) => Err(format!("{other:?}")),
        Err(e) => Err(e.to_string()),
    }
}

/// Rounds coordinates so the snapshot is about structure, not float noise.
fn rounded(mut v: Value) -> Value {
    match &mut v {
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                return json!((f * 10.0).round() / 10.0);
            }
        }
        Value::Array(items) => {
            for item in items.iter_mut() {
                *item = rounded(item.take());
            }
        }
        Value::Object(map) => {
            for value in map.values_mut() {
                *value = rounded(value.take());
            }
        }
        _ => {}
    }
    v
}

#[test]
fn flowchart_nodes_edges_and_subgraphs() {
    let out = layout(
        "flowchart TD\n  A[Start] --> B{Is it?}\n  B -->|Yes| C(Do it)\n  B -.-> D((End))\n  C <--> D\n  subgraph S [Group]\n    C\n  end\n",
    )
    .expect("lays out");
    assert_eq!(out["family"], "flowchart");
    let shapes: Vec<(&str, &str)> = out["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|n| {
            (
                n["id"].as_str().unwrap_or(""),
                n["shape"].as_str().unwrap_or(""),
            )
        })
        .collect();
    assert!(shapes.contains(&("A", "rect")));
    assert!(shapes.contains(&("B", "diamond")));
    assert!(shapes.contains(&("C", "round")));
    assert!(shapes.contains(&("D", "circle")));
    let edges = out["edges"].as_array().expect("edges");
    let dotted = edges
        .iter()
        .find(|e| e["from"] == "B" && e["to"] == "D")
        .expect("B-D");
    assert_eq!(dotted["dashed"], true);
    let both = edges
        .iter()
        .find(|e| e["from"] == "C" && e["to"] == "D")
        .expect("C-D");
    assert_eq!(
        (both["start"].as_str(), both["end"].as_str()),
        (Some("arrow"), Some("arrow"))
    );
    assert_eq!(out["clusters"][0]["title"], "Group");
    insta::assert_json_snapshot!(rounded(out));
}

#[test]
fn state_class_and_sequence_have_geometry() {
    let state =
        layout("stateDiagram-v2\n  [*] --> Idle\n  Idle --> Running : start\n  Running --> [*]\n")
            .expect("state");
    assert_eq!(state["family"], "state");
    assert!(
        state["nodes"]
            .as_array()
            .is_some_and(|n| n.iter().any(|n| n["shape"] == "start"))
    );
    assert!(
        state["edges"]
            .as_array()
            .is_some_and(|e| e.iter().any(|e| e["label"] == "start"))
    );

    let class = layout("classDiagram\n  Animal <|-- Duck\n  Animal : +int age\n").expect("class");
    assert_eq!(class["family"], "class");
    assert!(
        class["nodes"]
            .as_array()
            .is_some_and(|n| n.iter().any(|n| n["label"] == "Animal\n+int age"))
    );

    let seq =
        layout("sequenceDiagram\n  Alice->>Bob: Hello\n  Bob-->>Alice: Hi\n").expect("sequence");
    assert_eq!(seq["family"], "sequence");
    let msgs: Vec<&Value> = seq["edges"]
        .as_array()
        .expect("edges")
        .iter()
        .filter(|e| e["label"].is_string())
        .collect();
    assert_eq!(msgs.len(), 2);
    assert_eq!(msgs[1]["dashed"], true);
}

#[test]
fn other_families_are_refused_with_a_reason() {
    let err = layout("pie title Pets\n  \"Dogs\" : 3\n  \"Cats\" : 2\n").expect_err("refused");
    assert!(err.contains("[unsupported_layout]"), "{err}");
    let err = layout("flowchart TD\n  A -->").expect_err("syntax");
    assert!(
        err.contains("[render_error]") || err.contains("[diagram_error]"),
        "{err}"
    );
}
