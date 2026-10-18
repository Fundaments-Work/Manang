//! Reading older scene formats (ADR 0012, "Reading v1").
//!
//! Migration works on the JSON value before it is typed, so the typed model
//! only knows the current format and stays strict (`deny_unknown_fields`).

use serde_json::{Map, Value, json};

use crate::error::FigureError;
use crate::scene::SCENE_VERSION;

/// Brings a parsed scene of any supported version to the current one.
pub fn to_current(mut value: Value) -> Result<Value, FigureError> {
    let version = match value.get("version") {
        None | Some(Value::Null) => 1,
        Some(v) => v.as_u64().ok_or_else(|| {
            FigureError::InvalidArgument("`version` must be a whole number".to_owned())
        })?,
    };
    if version > u64::from(SCENE_VERSION) {
        return Err(FigureError::InvalidArgument(format!(
            "this figure is format version {version}; this Manang reads up to {SCENE_VERSION}. Update Manang to open it."
        )));
    }
    if version < 2 {
        value = v1_to_v2(value)?;
    }
    migrate_math_nodes(&mut value);
    Ok(value)
}

/// Formulas written before real math rendering kept `tex` and a baseline
/// `y`. They become LaTeX `source` (that is what `tex` meant) with a
/// top-left `y`, so the box they occupied is unchanged. This runs on every
/// version: the first v2 files had `tex` too.
fn migrate_math_nodes(value: &mut Value) {
    let Some(nodes) = value.get_mut("nodes").and_then(Value::as_array_mut) else {
        return;
    };
    for node in nodes {
        let Some(map) = node.as_object_mut() else {
            continue;
        };
        if map.get("kind").and_then(Value::as_str) != Some("math") {
            continue;
        }
        let Some(tex) = map.remove("tex") else {
            continue;
        };
        map.insert("source".to_owned(), tex);
        map.insert("language".to_owned(), json!("latex"));
        let size = num(map, "size");
        let y = num(map, "y");
        map.insert("y".to_owned(), json!(y - size));
    }
}

fn take(map: &mut Map<String, Value>, key: &str) -> Option<Value> {
    map.remove(key)
}

fn num(map: &Map<String, Value>, key: &str) -> f64 {
    map.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

/// v1 scene → v2 scene (see the module docs of the ADR for the mapping).
fn v1_to_v2(value: Value) -> Result<Value, FigureError> {
    let Value::Object(mut scene) = value else {
        return Err(FigureError::InvalidArgument(
            "a figure must be a JSON object".to_owned(),
        ));
    };
    let nodes = match take(&mut scene, "nodes") {
        Some(Value::Array(nodes)) => nodes,
        None => Vec::new(),
        Some(_) => {
            return Err(FigureError::InvalidArgument(
                "`nodes` must be an array".to_owned(),
            ));
        }
    };
    let migrated = nodes
        .into_iter()
        .map(v1_node)
        .collect::<Result<Vec<_>, _>>()?;
    scene.insert("version".to_owned(), json!(SCENE_VERSION));
    scene.insert("nodes".to_owned(), Value::Array(migrated));
    Ok(Value::Object(scene))
}

fn style_of(stroke: Option<Value>, fill: Option<Value>) -> Option<Value> {
    let mut style = Map::new();
    if let Some(s) = stroke.filter(|v| !v.is_null()) {
        style.insert("stroke".to_owned(), s);
    }
    if let Some(f) = fill.filter(|v| !v.is_null()) {
        style.insert("fill".to_owned(), f);
    }
    (!style.is_empty()).then_some(Value::Object(style))
}

fn v1_node(node: Value) -> Result<Value, FigureError> {
    let Value::Object(mut n) = node else {
        return Err(FigureError::InvalidArgument(
            "every node must be a JSON object".to_owned(),
        ));
    };
    let kind = n
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let id = take(&mut n, "id").unwrap_or(Value::Null);
    let mut out = Map::new();
    match kind.as_str() {
        "stroke" => {
            out.insert("kind".to_owned(), json!("freedraw"));
            out.insert("id".to_owned(), id);
            out.insert(
                "points".to_owned(),
                take(&mut n, "points").unwrap_or(json!([])),
            );
            out.insert(
                "size".to_owned(),
                take(&mut n, "size").unwrap_or(json!(2.5)),
            );
            if let Some(style) = style_of(take(&mut n, "color"), None) {
                out.insert("style".to_owned(), style);
            }
        }
        "rect" => {
            let (x, y, w, h) = (
                num(&n, "x"),
                num(&n, "y"),
                num(&n, "width"),
                num(&n, "height"),
            );
            // v1 kept a negative size from an upward drag.
            out.insert("kind".to_owned(), json!("rect"));
            out.insert("id".to_owned(), id);
            out.insert("x".to_owned(), json!(if w < 0.0 { x + w } else { x }));
            out.insert("y".to_owned(), json!(if h < 0.0 { y + h } else { y }));
            out.insert("width".to_owned(), json!(w.abs()));
            out.insert("height".to_owned(), json!(h.abs()));
            if let Some(style) = style_of(take(&mut n, "stroke"), take(&mut n, "fill")) {
                out.insert("style".to_owned(), style);
            }
        }
        "ellipse" => {
            let (cx, cy, rx, ry) = (num(&n, "cx"), num(&n, "cy"), num(&n, "rx"), num(&n, "ry"));
            out.insert("kind".to_owned(), json!("ellipse"));
            out.insert("id".to_owned(), id);
            out.insert("x".to_owned(), json!(cx - rx.abs()));
            out.insert("y".to_owned(), json!(cy - ry.abs()));
            out.insert("width".to_owned(), json!(rx.abs() * 2.0));
            out.insert("height".to_owned(), json!(ry.abs() * 2.0));
            if let Some(style) = style_of(take(&mut n, "stroke"), take(&mut n, "fill")) {
                out.insert("style".to_owned(), style);
            }
        }
        "arrow" => {
            out.insert("kind".to_owned(), json!("arrow"));
            out.insert("id".to_owned(), id);
            let from = take(&mut n, "from").unwrap_or(json!({ "x": 0, "y": 0 }));
            let to = take(&mut n, "to").unwrap_or(json!({ "x": 0, "y": 0 }));
            out.insert("points".to_owned(), json!([from, to]));
            if let Some(label) = take(&mut n, "label").filter(|v| !v.is_null()) {
                out.insert("label".to_owned(), label);
            }
            if let Some(style) = style_of(take(&mut n, "color"), None) {
                out.insert("style".to_owned(), style);
            }
        }
        "math" => {
            out.insert("kind".to_owned(), json!("math"));
            out.insert("id".to_owned(), id);
            for key in ["x", "y", "tex", "size"] {
                if let Some(v) = take(&mut n, key) {
                    out.insert(key.to_owned(), v);
                }
            }
            if let Some(style) = style_of(take(&mut n, "color"), None) {
                out.insert("style".to_owned(), style);
            }
        }
        other => {
            return Err(FigureError::InvalidArgument(format!(
                "unknown node kind `{other}` in a version 1 figure"
            )));
        }
    }
    Ok(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{FigureNode, FigureScene};

    #[test]
    fn v1_scene_migrates_every_kind() {
        let v1 = json!({
            "version": 1, "width": 800, "height": 600,
            "nodes": [
                { "kind": "stroke", "id": "n1", "points": [{ "x": 1, "y": 2, "pressure": 0.5 }], "size": 2.5, "color": "#e8e6e1" },
                { "kind": "rect", "id": "n2", "x": 100, "y": 100, "width": -40, "height": 30, "stroke": "#e8e6e1", "fill": "#1c1c20" },
                { "kind": "ellipse", "id": "n3", "cx": 50, "cy": 60, "rx": 10, "ry": 20, "stroke": "#e8e6e1" },
                { "kind": "arrow", "id": "n4", "from": { "x": 0, "y": 0 }, "to": { "x": 10, "y": 0 }, "color": "#e8e6e1", "label": "go" },
                { "kind": "math", "id": "n5", "x": 3, "y": 4, "tex": "x^2", "size": 26, "color": "#b3d17a" }
            ]
        });
        let scene = FigureScene::from_value(v1).expect("migrates");
        assert_eq!(scene.version, 2);
        let FigureNode::Rect(r) = &scene.nodes[1] else {
            panic!("rect")
        };
        assert_eq!((r.x, r.width), (60.0, 40.0));
        assert_eq!(r.style.fill.as_deref(), Some("#1c1c20"));
        let FigureNode::Ellipse(e) = &scene.nodes[2] else {
            panic!("ellipse")
        };
        assert_eq!((e.x, e.y, e.width, e.height), (40.0, 40.0, 20.0, 40.0));
        let FigureNode::Arrow(a) = &scene.nodes[3] else {
            panic!("arrow")
        };
        assert_eq!(a.points.len(), 2);
        assert_eq!(a.label.as_deref(), Some("go"));
        assert!(matches!(scene.nodes[0], FigureNode::Freedraw(_)));
        assert!(matches!(scene.nodes[4], FigureNode::Math(_)));
    }

    #[test]
    fn newer_versions_are_refused_with_a_reason() {
        let err =
            FigureScene::from_value(json!({ "version": 9, "width": 1, "height": 1, "nodes": [] }))
                .expect_err("refused");
        assert!(err.to_string().contains("version 9"));
    }

    #[test]
    fn unknown_v1_kinds_are_errors() {
        let err = FigureScene::from_value(
            json!({ "version": 1, "width": 1, "height": 1, "nodes": [{ "kind": "blob", "id": "a" }] }),
        )
        .expect_err("refused");
        assert!(err.to_string().contains("blob"));
    }

    #[test]
    fn old_formulas_become_latex_sources_in_the_same_box() {
        // A v2 file from before real math rendering: `tex` and a baseline y.
        let v2 = json!({
            "version": 2, "width": 800, "height": 600,
            "nodes": [{ "kind": "math", "id": "n1", "x": 10, "y": 50, "tex": "E = mc^2", "size": 20,
                        "style": { "stroke": "#B3D17A" } }]
        });
        let scene = FigureScene::from_value(v2).expect("reads");
        let FigureNode::Math(m) = &scene.nodes[0] else {
            panic!("a formula")
        };
        assert_eq!(m.source, "E = mc^2");
        assert_eq!(m.language.as_str(), "latex");
        assert_eq!((m.x, m.y, m.size), (10.0, 30.0, 20.0));
        // Hex colours are normalised on read, and the file form is stable.
        assert_eq!(m.style.stroke.as_deref(), Some("#b3d17a"));
        let text = scene.to_file_json().expect("writes");
        assert!(text.contains("\"source\"") && text.contains("\"language\": \"latex\""));
        assert!(!text.contains("\"tex\""));
        assert_eq!(FigureScene::from_json(&text).expect("reads"), scene);
        // v1 formulas follow the same road.
        let v1 = json!({ "version": 1, "width": 100, "height": 100, "nodes": [{ "kind": "math", "id": "n1", "x": 1, "y": 40, "tex": "x", "size": 16 }] });
        let scene = FigureScene::from_value(v1).expect("reads");
        assert!(matches!(&scene.nodes[0], FigureNode::Math(m) if m.y == 24.0));
    }

    #[test]
    fn malformed_hex_is_a_read_error_not_a_silent_fallback() {
        let bad = json!({ "version": 2, "width": 10, "height": 10, "nodes": [
            { "kind": "rect", "id": "n1", "x": 0, "y": 0, "width": 5, "height": 5, "style": { "stroke": "#12345" } }
        ]});
        let err = FigureScene::from_value(bad).expect_err("refused");
        assert!(err.to_string().contains("#12345"), "{err}");
        let bad_bg =
            json!({ "version": 2, "width": 10, "height": 10, "background": "#zzz", "nodes": [] });
        assert!(FigureScene::from_value(bad_bg).is_err());
    }
}
