//! Mermaid in figures (ADR 0012, "Mermaid"): `diagram.layout` output to
//! native shapes and bound arrows, and a best-effort flowchart back.

use std::collections::HashMap;

use serde_json::Value;

use crate::error::FigureError;
use crate::geometry::{node_bounds, scene_bounds};
use crate::scene::{
    Binding, ConnectorNode, FigureNode, FigureScene, Head, Routing, ShapeNode, StrokeStyle, Style,
    Vec2, format_node_id,
};

fn num(v: &Value, key: &str) -> f64 {
    v.get(key).and_then(Value::as_f64).unwrap_or(0.0)
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn head(name: Option<&str>) -> Head {
    match name {
        Some("arrow") => Head::Arrow,
        Some("triangle") => Head::Triangle,
        Some("dot") => Head::Dot,
        Some("bar") => Head::Bar,
        _ => Head::None,
    }
}

/// Native nodes for a `diagram.layout` result, placed with the diagram's
/// top-left corner at `origin`. Ids continue from `counter`.
pub fn nodes_from_layout(
    layout: &Value,
    origin: Vec2,
    counter: &mut usize,
) -> Result<Vec<FigureNode>, FigureError> {
    let list = |key: &str| {
        layout
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let (nodes, edges, clusters) = (list("nodes"), list("edges"), list("clusters"));
    if nodes.is_empty() {
        return Err(FigureError::InvalidArgument(
            "the diagram has no nodes to convert".to_owned(),
        ));
    }
    let min_x = nodes
        .iter()
        .chain(&clusters)
        .map(|n| num(n, "x"))
        .fold(f64::INFINITY, f64::min);
    let min_y = nodes
        .iter()
        .chain(&clusters)
        .map(|n| num(n, "y"))
        .fold(f64::INFINITY, f64::min);
    let (dx, dy) = (origin.x - min_x, origin.y - min_y);
    let mut next = || {
        *counter += 1;
        format_node_id(*counter)
    };

    let mut out = Vec::new();
    for c in &clusters {
        let mut frame = ShapeNode::new(
            next(),
            num(c, "x") + dx,
            num(c, "y") + dy,
            num(c, "width"),
            num(c, "height"),
        );
        frame.label = text(c, "title");
        out.push(FigureNode::Frame(frame));
    }
    let mut ids: HashMap<String, String> = HashMap::new();
    for n in &nodes {
        let Some(source_id) = text(n, "id") else {
            continue;
        };
        let id = next();
        let mut s = ShapeNode::new(
            id.clone(),
            num(n, "x") + dx,
            num(n, "y") + dy,
            num(n, "width").max(1.0),
            num(n, "height").max(1.0),
        );
        s.label = text(n, "label");
        let shape = text(n, "shape").unwrap_or_default();
        s.style = match shape.as_str() {
            "start" | "end" => Style {
                fill: Some("ink".to_owned()),
                ..Style::default()
            },
            "note" => Style {
                fill: Some("yellow".to_owned()),
                ..Style::default()
            },
            _ => Style {
                fill: Some("surface".to_owned()),
                ..Style::default()
            },
        };
        let node = match shape.as_str() {
            "diamond" => FigureNode::Diamond(s),
            "circle" | "start" | "end" => FigureNode::Ellipse(s),
            _ => FigureNode::Rect(s),
        };
        ids.insert(source_id, id);
        out.push(node);
    }
    for e in &edges {
        let points: Vec<Vec2> = e
            .get("points")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|p| Vec2::new(num(p, "x") + dx, num(p, "y") + dy))
            .collect();
        if points.len() < 2 {
            continue;
        }
        let mut c = ConnectorNode::new(next(), points[0], points[points.len() - 1]);
        c.points = points;
        c.start = text(e, "from")
            .and_then(|f| ids.get(&f))
            .map(|id| Binding::to(id.clone()));
        c.end = text(e, "to")
            .and_then(|t| ids.get(&t))
            .map(|id| Binding::to(id.clone()));
        c.label = text(e, "label");
        if c.points.len() > 2 {
            c.routing = Routing::Curved;
        }
        if e.get("dashed").and_then(Value::as_bool) == Some(true) {
            c.style.stroke_style = Some(StrokeStyle::Dashed);
        }
        let start = head(e.get("start").and_then(Value::as_str));
        let end = head(e.get("end").and_then(Value::as_str));
        if start == Head::None && end == Head::None {
            out.push(FigureNode::Line(c));
        } else {
            if start != Head::None {
                c.heads.start = Some(start);
            }
            if end != Head::Arrow {
                c.heads.end = Some(end);
            }
            out.push(FigureNode::Arrow(c));
        }
    }
    Ok(out)
}

fn mermaid_label(label: &str) -> String {
    label
        .replace('"', "#quot;")
        .replace('\n', "<br/>")
        .replace(['[', ']', '{', '}', '(', ')', '|'], " ")
        .trim()
        .to_owned()
}

/// A Mermaid flowchart of the scene's boxes, diamonds and ellipses and the
/// bound arrows and lines between them; the ids of everything left out.
pub fn to_mermaid(scene: &FigureScene) -> (String, Vec<String>) {
    let bounds = scene_bounds(scene);
    let direction = if bounds.width > bounds.height * 1.3 {
        "LR"
    } else {
        "TD"
    };
    let mut lines = vec![format!("flowchart {direction}")];
    let mut skipped = Vec::new();
    let mut included = std::collections::HashSet::new();
    let mut shapes: Vec<&FigureNode> = scene
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                n,
                FigureNode::Rect(_) | FigureNode::Diamond(_) | FigureNode::Ellipse(_)
            )
        })
        .collect();
    // Reading order: top to bottom, then left to right.
    shapes.sort_by(|a, b| {
        let (ra, rb) = (node_bounds(a), node_bounds(b));
        ra.y.total_cmp(&rb.y).then(ra.x.total_cmp(&rb.x))
    });
    for node in shapes {
        let label = mermaid_label(node.label().unwrap_or(node.id()));
        let id = node.id();
        let line = match node {
            FigureNode::Diamond(_) => format!("  {id}{{\"{label}\"}}"),
            FigureNode::Ellipse(_) => format!("  {id}((\"{label}\"))"),
            _ => format!("  {id}[\"{label}\"]"),
        };
        lines.push(line);
        included.insert(id.to_owned());
    }
    for node in &scene.nodes {
        match node {
            FigureNode::Rect(_) | FigureNode::Diamond(_) | FigureNode::Ellipse(_) => {}
            FigureNode::Arrow(c) | FigureNode::Line(c) => {
                let ends = c.start.as_ref().zip(c.end.as_ref());
                match ends {
                    Some((s, e)) if included.contains(&s.node) && included.contains(&e.node) => {
                        let arrow = matches!(node, FigureNode::Arrow(_));
                        let dashed = c.style.stroke_style() != StrokeStyle::Solid;
                        let link = match (arrow, dashed) {
                            (true, false) => "-->",
                            (true, true) => "-.->",
                            (false, false) => "---",
                            (false, true) => "-.-",
                        };
                        let label = c
                            .label
                            .as_deref()
                            .map(|l| format!("|\"{}\"|", mermaid_label(l)))
                            .unwrap_or_default();
                        lines.push(format!("  {} {link}{label} {}", s.node, e.node));
                    }
                    _ => skipped.push(c.id.clone()),
                }
            }
            other => skipped.push(other.id().to_owned()),
        }
    }
    let mut source = lines.join("\n");
    source.push('\n');
    (source, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn layout_becomes_bound_shapes() {
        let layout = json!({
            "family": "flowchart",
            "nodes": [
                { "id": "A", "label": "Start", "shape": "rect", "x": 10, "y": 10, "width": 100, "height": 40 },
                { "id": "B", "label": "Ok?", "shape": "diamond", "x": 10, "y": 100, "width": 80, "height": 80 }
            ],
            "edges": [{ "id": "e", "from": "A", "to": "B", "label": "go", "points": [{ "x": 60, "y": 50 }, { "x": 50, "y": 100 }], "dashed": true, "start": "none", "end": "arrow" }],
            "clusters": []
        });
        let mut counter = 4;
        let nodes =
            nodes_from_layout(&layout, Vec2::new(100.0, 100.0), &mut counter).expect("converts");
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[0].id(), "n5");
        assert_eq!(crate::geometry::node_rect(&nodes[0]).x, 100.0);
        let c = nodes[2].connector().expect("arrow");
        assert_eq!(c.start.as_ref().map(|b| b.node.as_str()), Some("n5"));
        assert_eq!(c.end.as_ref().map(|b| b.node.as_str()), Some("n6"));
        assert_eq!(c.style.stroke_style, Some(StrokeStyle::Dashed));
        assert!(matches!(nodes[1], FigureNode::Diamond(_)));
    }

    #[test]
    fn scene_becomes_a_flowchart() {
        let mut scene = FigureScene::new(800.0, 600.0);
        let mut a = ShapeNode::new("n1", 0.0, 0.0, 100.0, 40.0);
        a.label = Some("Say \"hi\"".into());
        scene.nodes.push(FigureNode::Rect(a));
        scene.nodes.push(FigureNode::Diamond(ShapeNode::new(
            "n2", 0.0, 100.0, 80.0, 80.0,
        )));
        let mut c = ConnectorNode::new("n3", Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0));
        c.start = Some(Binding::to("n1"));
        c.end = Some(Binding::to("n2"));
        c.label = Some("yes".into());
        scene.nodes.push(FigureNode::Arrow(c));
        scene
            .nodes
            .push(FigureNode::Text(ShapeNode::new("n4", 0.0, 0.0, 1.0, 1.0)));
        let (source, skipped) = to_mermaid(&scene);
        assert_eq!(
            source,
            "flowchart TD\n  n1[\"Say #quot;hi#quot;\"]\n  n2{\"n2\"}\n  n1 -->|\"yes\"| n2\n"
        );
        assert_eq!(skipped, vec!["n4".to_owned()]);
    }
}
