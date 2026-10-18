//! Agent tools (ADR 0012, "Agent tools"): high-level, forgiving commands
//! that build and change figures without a canvas. Every tool is one
//! ordinary operation on the figure (one undo step), addresses shapes by id
//! or label, and fails with a tagged message that says what would work.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::error::FigureError;
use crate::geometry::{node_bounds, node_rect, scene_bounds};
use crate::layout::{Direction, layout_moves};
use crate::ops::FigureOp;
use crate::scene::{
    Binding, ConnectorNode, FigureNode, FigureScene, FontFamily, Heads, Routing, ShapeNode,
    StrokeStyle, Style, Vec2, format_node_id,
};
use crate::svg::{LABEL_PADDING, label_width};
use crate::text::{self, LINE_HEIGHT};

/// The tool declarations, answered to the `register_agent_tool` hook.
pub const AGENT_TOOLS_JSON: &str = include_str!("agent_tools.json");

/// A tool failure with a machine-readable tag.
#[derive(Debug)]
pub struct ToolError {
    /// `not_found`, `ambiguous`, `invalid_argument`, `unavailable`.
    pub code: &'static str,
    /// What went wrong and what would work.
    pub message: String,
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl From<FigureError> for ToolError {
    fn from(e: FigureError) -> Self {
        let code = match e {
            FigureError::NodeNotFound(_) | FigureError::NoSuchFigure(_) => "not_found",
            FigureError::NoActiveFigure => "not_found",
            FigureError::Io { .. } => "io",
            _ => "invalid_argument",
        };
        ToolError {
            code,
            message: e.to_string(),
        }
    }
}

impl From<serde_json::Error> for ToolError {
    fn from(e: serde_json::Error) -> Self {
        invalid(format!("{e}"))
    }
}

fn invalid(message: impl Into<String>) -> ToolError {
    ToolError {
        code: "invalid_argument",
        message: message.into(),
    }
}

/// Parses tool input strictly (unknown fields are errors).
pub fn parse<T: for<'de> Deserialize<'de>>(args: &Value) -> Result<T, ToolError> {
    let value = if args.is_null() {
        json!({})
    } else {
        args.clone()
    };
    serde_json::from_value(value).map_err(|e| invalid(e.to_string()))
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

fn norm(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The id a reference names: an id, else a unique label (case and spacing
/// ignored; shapes win over arrows).
pub fn resolve_ref(scene: &FigureScene, reference: &str) -> Result<String, ToolError> {
    if scene.node_by_id(reference).is_some() {
        return Ok(reference.to_owned());
    }
    let wanted = norm(reference);
    let matches: Vec<&FigureNode> = scene
        .nodes
        .iter()
        .filter(|n| n.label().is_some_and(|l| norm(l) == wanted))
        .collect();
    let shapes: Vec<&&FigureNode> = matches.iter().filter(|n| n.connector().is_none()).collect();
    let pick: Vec<&FigureNode> = if shapes.is_empty() {
        matches.clone()
    } else {
        shapes.into_iter().copied().collect()
    };
    match pick.as_slice() {
        [one] => Ok(one.id().to_owned()),
        [] => {
            let suggestion = scene
                .nodes
                .iter()
                .filter_map(|n| n.label().map(|l| (n.id(), l)))
                .map(|(id, l)| (edit_distance(&norm(l), &wanted), id, l))
                .min_by_key(|(d, _, _)| *d)
                .filter(|(d, _, _)| *d <= (wanted.len() / 3).max(2));
            let hint = match suggestion {
                Some((_, id, label)) => format!("; did you mean \"{label}\" ({id})?"),
                None => "; call figure_describe to see the shapes".to_owned(),
            };
            Err(ToolError {
                code: "not_found",
                message: format!("no shape has the id or label \"{reference}\"{hint}"),
            })
        }
        many => Err(ToolError {
            code: "ambiguous",
            message: format!(
                "\"{reference}\" is the label of {} shapes ({}); use an id",
                many.len(),
                many.iter().map(|n| n.id()).collect::<Vec<_>>().join(", ")
            ),
        }),
    }
}

/// Which figure a tool works on.
#[derive(Debug, Default, Deserialize)]
pub struct Target {
    /// An open figure's id.
    pub figure: Option<u64>,
    /// A `.figure.json` path (opened when needed).
    pub path: Option<String>,
}

/// One shape for `figure.add-shapes`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewShape {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
    #[serde(default)]
    style: Option<Style>,
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

const SHAPE_KINDS: [&str; 6] = ["rect", "diamond", "ellipse", "text", "frame", "mermaid"];

/// A box that fits `label` in `kind`.
fn auto_size(kind: &str, label: Option<&str>, style: &Style) -> (f64, f64) {
    let size = style.font_size();
    let family = style.font_family();
    let label = label.unwrap_or("");
    let (w, h) = text::natural_size(label, size, family);
    match kind {
        "text" => (w.max(8.0), h.max(size * LINE_HEIGHT)),
        "frame" => (320.0, 200.0),
        "mermaid" => (320.0, 200.0),
        _ => {
            let mut width = (w + 2.0 * LABEL_PADDING + 24.0).clamp(120.0, 280.0);
            if kind == "diamond" {
                width = (width * 1.6).max(140.0);
            } else if kind == "ellipse" {
                width *= 1.3;
            }
            let lines = text::wrap(label, label_width(kind, width), size, family).len() as f64;
            let mut height = (lines * size * LINE_HEIGHT + 2.0 * LABEL_PADDING + 16.0).max(60.0);
            if kind == "diamond" {
                height = (height * 1.6).max(90.0);
            }
            (width, height)
        }
    }
}

/// Named style presets.
pub fn preset(name: &str) -> Result<Style, ToolError> {
    let s = Style::default();
    Ok(match name {
        "clean" => Style {
            roughness: Some(0),
            stroke_style: Some(StrokeStyle::Solid),
            font_family: Some(FontFamily::Sans),
            ..s
        },
        "sketch" => Style {
            roughness: Some(1),
            ..s
        },
        "emphasis" => Style {
            stroke: Some("accent".to_owned()),
            stroke_width: Some(2.5),
            ..s
        },
        "muted" => Style {
            stroke: Some("muted".to_owned()),
            opacity: Some(80.0),
            ..s
        },
        "dashed" => Style {
            stroke_style: Some(StrokeStyle::Dashed),
            ..s
        },
        "blueprint" => Style {
            stroke: Some("blue".to_owned()),
            fill: Some("none".to_owned()),
            font_family: Some(FontFamily::Mono),
            roughness: Some(0),
            ..s
        },
        "warning" => Style {
            stroke: Some("vermillion".to_owned()),
            fill: Some("vermillion".to_owned()),
            ..s
        },
        "success" => Style {
            stroke: Some("green".to_owned()),
            fill: Some("green".to_owned()),
            ..s
        },
        other => {
            return Err(invalid(format!(
                "unknown preset `{other}`; use clean, sketch, emphasis, muted, dashed, blueprint, warning or success"
            )));
        }
    })
}

/// A shape summary for tool outputs.
pub fn shape_summary(node: &FigureNode) -> Value {
    let r = node_rect(node);
    let mut v = json!({
        "id": node.id(),
        "kind": node.kind(),
        "x": round2(r.x), "y": round2(r.y),
        "width": round2(r.width), "height": round2(r.height),
    });
    if let Some(l) = node.label() {
        v["label"] = json!(l);
    }
    if !node.groups().is_empty() {
        v["groups"] = json!(node.groups());
    }
    if let Some(s) = node.shape()
        && s.angle != 0.0
    {
        v["angle"] = json!(s.angle);
    }
    v
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// `figure.add-shapes`: the operation and the new ids.
pub fn add_shapes(scene: &FigureScene, args: &Value) -> Result<(FigureOp, Vec<String>), ToolError> {
    #[derive(Deserialize)]
    struct Input {
        shapes: Vec<NewShape>,
    }
    let input: Input = parse(&strip_target(args))?;
    if input.shapes.is_empty() {
        return Err(invalid("`shapes` must list at least one shape"));
    }
    let bounds = scene_bounds(scene);
    let empty = scene.nodes.is_empty();
    // Auto-placement: a row to the right of what is there (or from the
    // top-left of an empty page), wrapping every five shapes.
    let row_x = if empty { 40.0 } else { bounds.right() + 60.0 };
    let row_y = if empty { 40.0 } else { bounds.y };
    let (mut x, mut y, mut row_h, mut in_row) = (row_x, row_y, 0.0f64, 0);
    let mut counter = scene.highest_counter();
    let mut nodes = Vec::new();
    let mut ids = Vec::new();
    for shape in input.shapes {
        let kind = shape.kind.as_deref().unwrap_or("rect");
        if !SHAPE_KINDS.contains(&kind) {
            return Err(invalid(format!(
                "unknown shape kind `{kind}`; use {}",
                SHAPE_KINDS.join(", ")
            )));
        }
        let mut style = match shape.preset.as_deref() {
            Some(p) => preset(p)?,
            None => Style::default(),
        };
        if let Some(extra) = &shape.style {
            style = style.merged(extra);
        }
        if style.fill.is_none() && matches!(kind, "rect" | "diamond" | "ellipse") {
            style.fill = Some("surface".to_owned());
        }
        let (aw, ah) = auto_size(kind, shape.label.as_deref(), &style);
        let (w, h) = (shape.width.unwrap_or(aw), shape.height.unwrap_or(ah));
        if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
            return Err(invalid("width and height must be positive"));
        }
        let (sx, sy) = match (shape.x, shape.y) {
            (Some(px), Some(py)) => (px, py),
            (px, py) => {
                if in_row == 5 {
                    x = row_x;
                    y += row_h + 60.0;
                    row_h = 0.0;
                    in_row = 0;
                }
                let at = (px.unwrap_or(x), py.unwrap_or(y));
                x += w + 60.0;
                row_h = row_h.max(h);
                in_row += 1;
                at
            }
        };
        counter += 1;
        let id = format_node_id(counter);
        let mut s = ShapeNode::new(id.clone(), sx, sy, w, h);
        s.label = shape.label;
        s.style = style;
        let node = match kind {
            "diamond" => FigureNode::Diamond(s),
            "ellipse" => FigureNode::Ellipse(s),
            "text" => FigureNode::Text(s),
            "frame" => FigureNode::Frame(s),
            "mermaid" => {
                s.source =
                    Some(shape.source.ok_or_else(|| {
                        invalid("a mermaid shape needs `source` (the Mermaid text)")
                    })?);
                FigureNode::Mermaid(s)
            }
            _ => FigureNode::Rect(s),
        };
        ids.push(id);
        nodes.push(node);
    }
    Ok((FigureOp::Add { nodes, index: None }, ids))
}

/// Removes the target fields so the tool's own struct can deny unknown ones.
pub fn strip_target(args: &Value) -> Value {
    match args {
        Value::Object(map) => {
            let mut m: Map<String, Value> = map.clone();
            m.remove("figure");
            m.remove("path");
            Value::Object(m)
        }
        _ => json!({}),
    }
}

/// `figure.connect`.
pub fn connect(scene: &FigureScene, args: &Value) -> Result<(FigureOp, Vec<String>), ToolError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Connection {
        from: String,
        to: String,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        routing: Option<Routing>,
        #[serde(default)]
        kind: Option<String>,
        #[serde(default)]
        heads: Option<Heads>,
        #[serde(default)]
        style: Option<Style>,
        #[serde(default)]
        preset: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        connections: Vec<Connection>,
    }
    let input: Input = parse(&strip_target(args))?;
    if input.connections.is_empty() {
        return Err(invalid("`connections` must list at least one connection"));
    }
    let mut counter = scene.highest_counter();
    let mut nodes = Vec::new();
    let mut ids = Vec::new();
    for c in input.connections {
        let from = resolve_ref(scene, &c.from)?;
        let to = resolve_ref(scene, &c.to)?;
        for id in [&from, &to] {
            if scene.node_by_id(id).is_some_and(|n| !n.is_bindable()) {
                return Err(invalid(format!(
                    "`{id}` is an arrow or a drawing; connect shapes"
                )));
            }
        }
        counter += 1;
        let id = format_node_id(counter);
        let a = scene
            .node_by_id(&from)
            .map_or(crate::scene::Rect::EMPTY, node_rect);
        let b = scene
            .node_by_id(&to)
            .map_or(crate::scene::Rect::EMPTY, node_rect);
        let mut conn = ConnectorNode::new(id.clone(), a.center(), b.center());
        conn.start = Some(Binding::to(from));
        conn.end = Some(Binding::to(to));
        conn.label = c.label;
        conn.routing = c.routing.unwrap_or_default();
        conn.heads = c.heads.unwrap_or_default();
        let mut style = match c.preset.as_deref() {
            Some(p) => preset(p)?,
            None => Style::default(),
        };
        if let Some(extra) = &c.style {
            style = style.merged(extra);
        }
        // Connectors are not filled.
        style.fill = None;
        conn.style = style;
        let node = match c.kind.as_deref() {
            None | Some("arrow") => FigureNode::Arrow(conn),
            Some("line") => FigureNode::Line(conn),
            Some(other) => {
                return Err(invalid(format!(
                    "unknown connection kind `{other}`; use arrow or line"
                )));
            }
        };
        ids.push(id);
        nodes.push(node);
    }
    Ok((FigureOp::Add { nodes, index: None }, ids))
}

/// `figure.update`.
pub fn update(scene: &FigureScene, args: &Value) -> Result<(FigureOp, Vec<String>), ToolError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Change {
        #[serde(rename = "ref")]
        reference: String,
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        kind: Option<String>,
        #[serde(default)]
        x: Option<f64>,
        #[serde(default)]
        y: Option<f64>,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
        #[serde(default)]
        angle: Option<f64>,
        #[serde(default)]
        style: Option<Style>,
        #[serde(default)]
        routing: Option<Routing>,
        #[serde(default)]
        source: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        updates: Vec<Change>,
    }
    let input: Input = parse(&strip_target(args))?;
    let mut ops = Vec::new();
    let mut ids = Vec::new();
    for c in input.updates {
        let id = resolve_ref(scene, &c.reference)?;
        let node = scene
            .node_by_id(&id)
            .ok_or_else(|| invalid(format!("`{id}` vanished")))?;
        let mut set = Map::new();
        if let Some(l) = c.label {
            set.insert("label".to_owned(), json!(l));
        }
        if let Some(k) = c.kind {
            if node.shape().is_none() || !SHAPE_KINDS.contains(&k.as_str()) {
                return Err(invalid(format!(
                    "`{id}` cannot become a `{k}`; shapes switch between {}",
                    SHAPE_KINDS.join(", ")
                )));
            }
            set.insert("kind".to_owned(), json!(k));
        }
        if let Some(a) = c.angle {
            set.insert("angle".to_owned(), json!(a));
        }
        if let Some(r) = c.routing {
            set.insert("routing".to_owned(), serde_json::to_value(r)?);
        }
        if let Some(s) = c.source {
            set.insert("source".to_owned(), json!(s));
        }
        if let Some(style) = c.style {
            set.insert("style".to_owned(), serde_json::to_value(style)?);
        }
        if !set.is_empty() {
            ops.push(FigureOp::Update {
                id: id.clone(),
                set,
            });
        }
        if c.x.is_some() || c.y.is_some() || c.width.is_some() || c.height.is_some() {
            let r = node_rect(node);
            ops.push(FigureOp::Resize {
                id: id.clone(),
                x: c.x.unwrap_or(r.x),
                y: c.y.unwrap_or(r.y),
                width: c.width.unwrap_or(r.width),
                height: c.height.unwrap_or(r.height),
            });
        }
        ids.push(id);
    }
    if ops.is_empty() {
        return Err(invalid(
            "nothing to change: give label, kind, x, y, width, height, angle, style, routing or source",
        ));
    }
    Ok((FigureOp::Batch { ops }, ids))
}

fn refs_of(scene: &FigureScene, refs: &[String]) -> Result<Vec<String>, ToolError> {
    refs.iter().map(|r| resolve_ref(scene, r)).collect()
}

/// `figure.remove`.
pub fn remove(scene: &FigureScene, args: &Value) -> Result<(FigureOp, Vec<String>), ToolError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        refs: Vec<String>,
    }
    let input: Input = parse(&strip_target(args))?;
    let mut ids = refs_of(scene, &input.refs)?;
    ids.dedup();
    Ok((FigureOp::Remove { ids: ids.clone() }, ids))
}

/// `figure.layout`.
pub fn layout(scene: &FigureScene, args: &Value) -> Result<(FigureOp, Vec<String>), ToolError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        #[serde(default)]
        direction: Option<Direction>,
        #[serde(default)]
        refs: Option<Vec<String>>,
        #[serde(default)]
        spacing: Option<f64>,
        #[serde(default)]
        routing: Option<Routing>,
    }
    let input: Input = parse(&strip_target(args))?;
    let only = match &input.refs {
        Some(refs) => Some(refs_of(scene, refs)?.into_iter().collect::<BTreeSet<_>>()),
        None => None,
    };
    let spacing = input.spacing.unwrap_or(60.0);
    if !(spacing.is_finite() && (4.0..=1000.0).contains(&spacing)) {
        return Err(invalid("spacing must be between 4 and 1000"));
    }
    let direction = input.direction.unwrap_or_default();
    let mut ops = layout_moves(scene, only.as_ref(), direction, spacing)?;
    let moved: Vec<String> = ops
        .iter()
        .filter_map(|op| match op {
            FigureOp::Move { ids, .. } => ids.first().cloned(),
            _ => None,
        })
        .collect();
    // Optionally set the routing of the arrows between laid-out shapes.
    if let Some(routing) = input.routing {
        for node in &scene.nodes {
            if let Some(c) = node.connector()
                && c.start.is_some()
                && c.end.is_some()
                && c.routing != routing
            {
                let mut set = Map::new();
                set.insert("routing".to_owned(), serde_json::to_value(routing)?);
                ops.push(FigureOp::Update {
                    id: c.id.clone(),
                    set,
                });
            }
        }
    }
    Ok((FigureOp::Batch { ops }, moved))
}

/// `figure.style`.
pub fn style(scene: &FigureScene, args: &Value) -> Result<(FigureOp, Vec<String>), ToolError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        #[serde(default)]
        refs: Option<Vec<String>>,
        #[serde(default)]
        preset: Option<String>,
        #[serde(default)]
        style: Option<Style>,
    }
    let input: Input = parse(&strip_target(args))?;
    let mut patch = match input.preset.as_deref() {
        Some(p) => preset(p)?,
        None => Style::default(),
    };
    if let Some(extra) = &input.style {
        patch = patch.merged(extra);
    }
    if patch.is_empty() {
        return Err(invalid("give a `preset` or a `style`"));
    }
    let ids = match &input.refs {
        Some(refs) => refs_of(scene, refs)?,
        None => scene.nodes.iter().map(|n| n.id().to_owned()).collect(),
    };
    Ok((
        FigureOp::SetStyle {
            ids: ids.clone(),
            style: Some(patch),
            id: None,
            stroke: None,
            fill: None,
            size: None,
            color: None,
        },
        ids,
    ))
}

/// `figure.describe`: a semantic summary.
pub fn describe(scene: &FigureScene) -> Value {
    let mut shapes = Vec::new();
    let mut connections = Vec::new();
    let mut frames = Vec::new();
    let mut other = Vec::new();
    let name = |id: &str| -> String {
        match scene.node_by_id(id).and_then(FigureNode::label) {
            Some(l) => format!("\"{}\" ({id})", l.replace('\n', " ")),
            None => id.to_owned(),
        }
    };
    let mut sentences = Vec::new();
    for node in &scene.nodes {
        match node {
            FigureNode::Arrow(c) | FigureNode::Line(c) => {
                let mut v = json!({ "id": c.id, "kind": node.kind() });
                if let Some(b) = &c.start {
                    v["from"] = json!(b.node);
                }
                if let Some(b) = &c.end {
                    v["to"] = json!(b.node);
                }
                if let Some(l) = &c.label {
                    v["label"] = json!(l);
                }
                if c.routing != Routing::Straight {
                    v["routing"] = serde_json::to_value(c.routing).unwrap_or(Value::Null);
                }
                if let (Some(s), Some(e)) = (&c.start, &c.end) {
                    let via = c
                        .label
                        .as_deref()
                        .map(|l| format!(" labelled \"{l}\""))
                        .unwrap_or_default();
                    sentences.push(format!("{} -> {}{via}", name(&s.node), name(&e.node)));
                }
                connections.push(v);
            }
            FigureNode::Frame(_) => {
                let mut v = shape_summary(node);
                v["contains"] = json!(crate::geometry::frame_children(scene, node.id()));
                frames.push(v);
            }
            FigureNode::Freedraw(_) | FigureNode::Math(_) | FigureNode::Image(_) => {
                other.push(shape_summary(node));
            }
            _ => shapes.push(shape_summary(node)),
        }
    }
    let b = scene_bounds(scene);
    let mut summary = format!(
        "{} shape{}, {} connection{}, {} frame{}, {} other element{}.",
        shapes.len(),
        if shapes.len() == 1 { "" } else { "s" },
        connections.len(),
        if connections.len() == 1 { "" } else { "s" },
        frames.len(),
        if frames.len() == 1 { "" } else { "s" },
        other.len(),
        if other.len() == 1 { "" } else { "s" },
    );
    if !sentences.is_empty() {
        summary.push_str(" Connections: ");
        summary.push_str(&sentences.join("; "));
        summary.push('.');
    }
    let unconnected: Vec<String> = shapes
        .iter()
        .filter_map(|s| s["id"].as_str())
        .filter(|id| {
            !connections
                .iter()
                .any(|c| c["from"] == json!(id) || c["to"] == json!(id))
        })
        .map(name)
        .collect();
    if !unconnected.is_empty() && !connections.is_empty() {
        summary.push_str(&format!(" Not connected: {}.", unconnected.join(", ")));
    }
    json!({
        "page": { "width": scene.width, "height": scene.height },
        "bounds": { "x": round2(b.x), "y": round2(b.y), "width": round2(b.width), "height": round2(b.height) },
        "shapes": shapes,
        "connections": connections,
        "frames": frames,
        "other": other,
        "summary": summary,
    })
}

/// Where `figure.from-mermaid` puts a converted diagram: the given point,
/// else below what is on the page.
pub fn insertion_point(scene: &FigureScene, x: Option<f64>, y: Option<f64>) -> Vec2 {
    if let (Some(x), Some(y)) = (x, y) {
        return Vec2::new(x, y);
    }
    if scene.nodes.is_empty() {
        return Vec2::new(x.unwrap_or(40.0), y.unwrap_or(40.0));
    }
    let b = scene
        .nodes
        .iter()
        .fold(crate::scene::Rect::EMPTY, |acc, n| {
            crate::geometry::union_rect(&acc, &node_bounds(n))
        });
    Vec2::new(x.unwrap_or(b.x), y.unwrap_or(b.bottom() + 60.0))
}
