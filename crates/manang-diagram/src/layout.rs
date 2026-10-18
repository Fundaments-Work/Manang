//! `diagram.layout`: merman's own laid-out geometry for graph-like Mermaid
//! families, joined with the labels and shapes of the semantic model, so
//! the figure studio can turn a diagram into editable shapes (ADR 0012).
//!
//! Output, in Mermaid's coordinate space (top-left origin):
//!
//! ```json
//! { "family": "flowchart", "direction": "TB",
//!   "nodes": [{ "id", "label", "shape", "x", "y", "width", "height" }],
//!   "edges": [{ "id", "from", "to", "label", "points": [{ "x", "y" }],
//!               "dashed", "start", "end" }],
//!   "clusters": [{ "id", "title", "x", "y", "width", "height" }] }
//! ```
//!
//! `x`/`y` are top-left corners. `shape` is one of `rect`, `round`,
//! `diamond`, `circle`, `stadium`, `start`, `end`, `note`, `actor`. `from` and
//! `to` are node ids or null (sequence messages run between lifelines);
//! `start`/`end` are heads: `none`, `arrow`, `triangle`, `dot`, `bar`.

use std::collections::HashMap;

use merman::{OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest};
use serde_json::{Value, json};

use crate::error::DiagramError;

/// Families whose layout is a graph of boxes and edges.
const FAMILIES: [(&str, &str); 4] = [
    ("FlowchartV2", "flowchart"),
    ("StateDiagramV2", "state"),
    ("ClassDiagramV2", "class"),
    ("SequenceDiagram", "sequence"),
];

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|s| !s.is_empty())
}

/// Lays out `source` with merman and returns the joined geometry.
pub fn layout(renderer: &Renderer, source: &str) -> Result<Value, DiagramError> {
    let semantic = match renderer.render(RenderRequest::semantic(source, OperationControl::new())) {
        Ok(RenderOutput::Semantic(Some(s))) => s
            .compatibility_json()
            .map_err(|e| DiagramError::Render(e.to_string()))?
            .as_value()
            .clone(),
        Ok(_) => return Err(DiagramError::Render("no Mermaid diagram found".to_owned())),
        Err(e) => return Err(DiagramError::Render(e.to_string())),
    };
    let laid_out = match renderer.render(RenderRequest::layout_json(
        source,
        OperationControl::new(),
        SvgRequest::default(),
    )) {
        Ok(RenderOutput::LayoutJson(Some(l))) => l.layout().as_value().clone(),
        Ok(_) => return Err(DiagramError::Render("no Mermaid diagram found".to_owned())),
        Err(e) => return Err(DiagramError::Render(e.to_string())),
    };
    let layout = laid_out.get("layout").cloned().unwrap_or(Value::Null);
    let Some((key, family)) = FAMILIES
        .iter()
        .find(|(key, _)| layout.get(*key).is_some())
        .copied()
    else {
        let kind = layout
            .as_object()
            .and_then(|m| m.keys().next().cloned())
            .unwrap_or_else(|| "unknown".to_owned());
        return Err(DiagramError::InvalidArgument(format!(
            "[unsupported_layout] {kind} diagrams have no shape layout; insert them as a Mermaid block"
        )));
    };
    let geometry = &layout[key];
    let labels = Labels::from_semantic(family, &semantic);

    let mut ids = std::collections::HashSet::new();
    let nodes: Vec<Value> = geometry
        .get("nodes")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|n| n.get("is_cluster").and_then(Value::as_bool) != Some(true))
        .filter_map(|n| {
            let id = text(n, "id")?;
            let (w, h) = (num(n, "width"), num(n, "height"));
            let (label, shape) = labels.node(&id);
            ids.insert(id.clone());
            Some(json!({
                "id": id, "label": label, "shape": shape,
                "x": num(n, "x") - w / 2.0, "y": num(n, "y") - h / 2.0,
                "width": w, "height": h,
            }))
        })
        .collect();
    let edges: Vec<Value> = geometry
        .get("edges")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|e| {
            let id = text(e, "id")?;
            let style = labels.edge(&id);
            let end = |key: &str| text(e, key).filter(|n| ids.contains(n));
            Some(json!({
                "id": id,
                "from": end("from"),
                "to": end("to"),
                "label": style.label,
                "points": e.get("points").cloned().unwrap_or(json!([])),
                "dashed": style.dashed || e.get("stroke_dasharray").is_some_and(|d| !d.is_null()),
                "start": style.start,
                "end": style.end,
            }))
        })
        .collect();
    let clusters: Vec<Value> = geometry
        .get("clusters")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|c| {
            let (w, h) = (num(c, "width"), num(c, "height"));
            Some(json!({
                "id": text(c, "id")?,
                "title": text(c, "title"),
                "x": num(c, "x") - w / 2.0, "y": num(c, "y") - h / 2.0,
                "width": w, "height": h,
            }))
        })
        .collect();
    Ok(json!({
        "family": family,
        "direction": semantic.get("direction").and_then(Value::as_str).unwrap_or("TB"),
        "nodes": nodes,
        "edges": edges,
        "clusters": clusters,
    }))
}

#[derive(Default)]
struct EdgeStyle {
    label: Option<String>,
    dashed: bool,
    start: &'static str,
    end: &'static str,
}

#[derive(Default)]
struct Labels {
    nodes: HashMap<String, (Option<String>, &'static str)>,
    edges: HashMap<String, EdgeStyle>,
    family: &'static str,
}

fn flow_shape(shape: &str) -> &'static str {
    match shape {
        "diamond" | "question" | "hexagon" => "diamond",
        "circle" | "doublecircle" | "ellipse" => "circle",
        "stadium" => "stadium",
        "round" | "roundedRect" => "round",
        _ => "rect",
    }
}

fn head(kind: &str) -> &'static str {
    match kind {
        "arrow_point" | "arrow_barb" | "arrow_barb_neo" => "arrow",
        "arrow_circle" => "dot",
        "arrow_cross" => "bar",
        _ => "none",
    }
}

impl Labels {
    fn from_semantic(family: &'static str, s: &Value) -> Self {
        let mut out = Labels {
            family,
            ..Labels::default()
        };
        let list = |key: &str| {
            s.get(key)
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        };
        match family {
            "flowchart" => {
                for n in list("nodes") {
                    if let Some(id) = text(&n, "id") {
                        let shape = text(&n, "shape").unwrap_or_default();
                        out.nodes
                            .insert(id, (text(&n, "label"), flow_shape(&shape)));
                    }
                }
                for e in list("edges") {
                    if let Some(id) = text(&e, "id") {
                        let kind = text(&e, "type").unwrap_or_default();
                        let double = kind.starts_with("double_");
                        let end = head(kind.trim_start_matches("double_"));
                        out.edges.insert(
                            id,
                            EdgeStyle {
                                label: text(&e, "label"),
                                dashed: text(&e, "stroke").as_deref() == Some("dotted"),
                                start: if double { end } else { "none" },
                                end,
                            },
                        );
                    }
                }
            }
            "state" => {
                for n in list("nodes") {
                    if let Some(id) = text(&n, "id") {
                        let shape = match text(&n, "shape").as_deref() {
                            Some("stateStart") => "start",
                            Some("stateEnd") => "end",
                            Some("choice") => "diamond",
                            Some("note") => "note",
                            _ => "round",
                        };
                        let label = if matches!(shape, "start" | "end") {
                            None
                        } else {
                            text(&n, "label")
                        };
                        out.nodes.insert(id, (label, shape));
                    }
                }
                for e in list("edges") {
                    if let Some(id) = text(&e, "id") {
                        out.edges.insert(
                            id,
                            EdgeStyle {
                                label: text(&e, "label"),
                                end: "arrow",
                                start: "none",
                                dashed: false,
                            },
                        );
                    }
                }
            }
            "class" => {
                if let Some(classes) = s.get("classes").and_then(Value::as_object) {
                    for (id, c) in classes {
                        let mut lines = vec![text(c, "label").unwrap_or_else(|| id.clone())];
                        for key in ["members", "methods"] {
                            for m in c.get(key).and_then(Value::as_array).into_iter().flatten() {
                                if let Some(t) = text(m, "displayText") {
                                    lines.push(t);
                                }
                            }
                        }
                        out.nodes
                            .insert(id.clone(), (Some(lines.join("\n")), "rect"));
                    }
                }
                for r in list("relations") {
                    if let Some(id) = text(&r, "id") {
                        let rel = r.get("relation").cloned().unwrap_or(Value::Null);
                        let ty = |k: &str| rel.get(k).and_then(Value::as_i64).unwrap_or(-1);
                        let class_head = |t: i64| match t {
                            1 => "triangle",
                            0 | 2 => "dot",
                            3 => "arrow",
                            _ => "none",
                        };
                        out.edges.insert(
                            id,
                            EdgeStyle {
                                label: text(&r, "title"),
                                dashed: rel.get("lineType").and_then(Value::as_i64) == Some(1),
                                start: class_head(ty("type1")),
                                end: class_head(ty("type2")),
                            },
                        );
                    }
                }
            }
            _ => {
                if let Some(actors) = s.get("actors").and_then(Value::as_object) {
                    for (name, a) in actors {
                        let label = text(a, "description").or_else(|| Some(name.clone()));
                        for side in ["top", "bottom"] {
                            out.nodes
                                .insert(format!("actor-{side}-{name}"), (label.clone(), "actor"));
                        }
                        out.edges.insert(
                            format!("lifeline-{name}"),
                            EdgeStyle {
                                dashed: true,
                                start: "none",
                                end: "none",
                                label: None,
                            },
                        );
                    }
                }
                for m in list("messages") {
                    if let Some(id) = text(&m, "id") {
                        // Mermaid's LINETYPE: odd numbers are dotted; 0/1
                        // filled arrow, 3/4 cross, 5/6 open.
                        let ty = m.get("type").and_then(Value::as_i64).unwrap_or(0);
                        let end = match ty {
                            3 | 4 => "bar",
                            5 | 6 => "none",
                            _ => "arrow",
                        };
                        out.edges.insert(
                            format!("msg-{id}"),
                            EdgeStyle {
                                label: text(&m, "message"),
                                dashed: ty % 2 == 1,
                                start: "none",
                                end,
                            },
                        );
                    }
                }
            }
        }
        out
    }

    fn node(&self, id: &str) -> (Option<String>, &'static str) {
        match self.nodes.get(id) {
            Some((label, shape)) => (label.clone(), shape),
            None if self.family == "state" && id.ends_with("_start") => (None, "start"),
            None if self.family == "state" && id.ends_with("_end") => (None, "end"),
            None => (Some(id.to_owned()), "rect"),
        }
    }

    fn edge(&self, id: &str) -> EdgeStyle {
        match self.edges.get(id) {
            Some(e) => EdgeStyle {
                label: e.label.clone(),
                dashed: e.dashed,
                start: e.start,
                end: e.end,
            },
            None => EdgeStyle {
                end: "arrow",
                start: "none",
                ..EdgeStyle::default()
            },
        }
    }
}
