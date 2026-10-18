//! Scene operations and their deltas (ADR 0012, "Operations and history").
//!
//! Every operation addresses nodes by id and runs inside a transaction that
//! records the before value of each node it touches (and the paint order,
//! when it changes). The resulting [`Delta`] is what undo and redo replay,
//! so the history holds what changed, never copies of the scene. Arrows bound
//! to a shape that moved are re-routed inside the same transaction.
//! `src/lib/figure/ops.ts` mirrors this module.

use std::collections::{BTreeSet, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::FigureError;
use crate::geometry::{node_bounds, node_rect, normalize_rect, union_rect};
use crate::palette;
use crate::route::route;
use crate::scene::{Binding, FigureNode, FigureScene, Rect, Style, Vec2, base36};

/// Where `reorder` moves nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ZOrder {
    /// To the top.
    Front,
    /// To the bottom.
    Back,
    /// One step up.
    Forward,
    /// One step down.
    Backward,
}

/// Which end of a connector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum End {
    /// The first point.
    Start,
    /// The last point.
    End,
}

/// Which edge or centre line `align` lines nodes up on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlignEdge {
    /// Left edges.
    Left,
    /// Vertical centre lines.
    Center,
    /// Right edges.
    Right,
    /// Top edges.
    Top,
    /// Horizontal centre lines.
    Middle,
    /// Bottom edges.
    Bottom,
}

/// The axis `distribute` spaces nodes along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    /// Left to right.
    Horizontal,
    /// Top to bottom.
    Vertical,
}

/// One operation on a scene, as `figure.apply` takes it (`op` tag).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
pub enum FigureOp {
    /// Insert nodes (at `index`, else on top).
    Add {
        /// New nodes; ids must be unused.
        nodes: Vec<FigureNode>,
        /// Paint position of the first node.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    /// Shallow-merge fields into a node (`style` merges one level deeper;
    /// `null` clears an optional field).
    Update {
        /// Target node.
        id: String,
        /// Fields to set.
        set: Map<String, Value>,
    },
    /// Remove nodes; arrows bound to them keep their place and lose the binding.
    Remove {
        /// Nodes to remove.
        ids: Vec<String>,
    },
    /// Move nodes by a delta.
    Move {
        /// Nodes to move.
        ids: Vec<String>,
        /// Horizontal offset.
        dx: f64,
        /// Vertical offset.
        dy: f64,
    },
    /// Give a node a new box; point-based nodes scale.
    Resize {
        /// Target node.
        id: String,
        /// New left edge.
        x: f64,
        /// New top edge.
        y: f64,
        /// New width.
        width: f64,
        /// New height.
        height: f64,
    },
    /// Set the rotation of box-shaped nodes.
    Rotate {
        /// Nodes to rotate.
        ids: Vec<String>,
        /// Absolute angle in degrees.
        angle: f64,
    },
    /// Change paint order.
    Reorder {
        /// Nodes to move.
        ids: Vec<String>,
        /// Where to.
        to: ZOrder,
    },
    /// Put nodes in a (new) group.
    Group {
        /// Members (two or more).
        ids: Vec<String>,
        /// Group id; generated when absent.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        group: Option<String>,
    },
    /// Remove the outermost group of each node.
    Ungroup {
        /// Members.
        ids: Vec<String>,
    },
    /// Bind (or unbind, with `node: null`) one end of a connector.
    Bind {
        /// The connector.
        id: String,
        /// Which end.
        end: End,
        /// The shape, or null to unbind.
        node: Option<String>,
        /// Fixed anchor (fractions of the box); absent = floating.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        anchor: Option<Vec2>,
    },
    /// Line nodes up on an edge or centre line.
    Align {
        /// Two or more nodes.
        ids: Vec<String>,
        /// Which line.
        edge: AlignEdge,
    },
    /// Space nodes evenly.
    Distribute {
        /// Three or more nodes.
        ids: Vec<String>,
        /// Along which axis.
        axis: Axis,
    },
    /// Merge style fields into nodes. The v1 form (`id`, `stroke`, `fill`,
    /// `size`, `color`) is still accepted.
    SetStyle {
        /// Targets.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        ids: Vec<String>,
        /// Fields to merge.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<Style>,
        /// v1: one target.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// v1: stroke colour.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stroke: Option<String>,
        /// v1: fill colour.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fill: Option<String>,
        /// v1: stroke diameter (freehand) or font size (formula).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        size: Option<f64>,
        /// v1: colour.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
    },
    /// Resize the page.
    SetCanvas {
        /// New width.
        width: f64,
        /// New height.
        height: f64,
        /// Page background: a palette token or `#rrggbb`; `none` makes it
        /// transparent; absent leaves it as it is.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        background: Option<String>,
    },
    /// Several operations as one undo step.
    Batch {
        /// The operations, in order.
        ops: Vec<FigureOp>,
    },
    /// Remove every node.
    Clear,
    /// Undo the latest step.
    Undo,
    /// Redo the latest undone step.
    Redo,
    /// v1: add one node.
    AddNode {
        /// The node.
        node: FigureNode,
    },
    /// v1: replace a node.
    UpdateNode {
        /// The replacement (same id).
        node: FigureNode,
    },
    /// v1: remove one node.
    RemoveNode {
        /// The node id.
        id: String,
    },
    /// v1: move one node.
    Translate {
        /// The node id.
        id: String,
        /// Horizontal offset.
        dx: f64,
        /// Vertical offset.
        dy: f64,
    },
}

impl FigureOp {
    /// Parses an operation from JSON, also accepting the older spellings:
    /// a bare string (`"undo"`) and a `type` or `kind` tag instead of `op`.
    pub fn from_value(value: Value) -> Result<Self, FigureError> {
        let value = match value {
            Value::String(s) => {
                let mut map = Map::new();
                map.insert("op".to_owned(), Value::String(s.to_ascii_lowercase()));
                Value::Object(map)
            }
            Value::Object(mut map) if !map.contains_key("op") => {
                for alt in ["type", "kind"] {
                    if let Some(tag) = map.remove(alt) {
                        map.insert("op".to_owned(), tag);
                        break;
                    }
                }
                Value::Object(map)
            }
            other => other,
        };
        serde_json::from_value(value)
            .map_err(|e| FigureError::InvalidArgument(format!("invalid `op`: {e}")))
    }
}

/// What one applied operation changed, for undo, redo and the editor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Delta {
    before: Vec<(String, Option<FigureNode>)>,
    after: Vec<(String, Option<FigureNode>)>,
    order_before: Option<Vec<String>>,
    order_after: Option<Vec<String>>,
    canvas_before: Option<Canvas>,
    canvas_after: Option<Canvas>,
}

/// The page: width, height, background.
type Canvas = (f64, f64, Option<String>);

impl Delta {
    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        self.before.is_empty() && self.order_before.is_none() && self.canvas_before.is_none()
    }

    /// Ids of the nodes it touched, sorted.
    pub fn changed_ids(&self) -> Vec<String> {
        let set: BTreeSet<&String> = self.before.iter().map(|(id, _)| id).collect();
        set.into_iter().cloned().collect()
    }

    /// Puts the scene back as it was before.
    pub fn revert(&self, scene: &mut FigureScene) {
        apply_side(
            scene,
            &self.before,
            self.order_before.as_deref(),
            self.canvas_before.as_ref(),
        );
    }

    /// Re-applies the change.
    pub fn reapply(&self, scene: &mut FigureScene) {
        apply_side(
            scene,
            &self.after,
            self.order_after.as_deref(),
            self.canvas_after.as_ref(),
        );
    }
}

fn apply_side(
    scene: &mut FigureScene,
    nodes: &[(String, Option<FigureNode>)],
    order: Option<&[String]>,
    canvas: Option<&Canvas>,
) {
    for (id, value) in nodes {
        match value {
            Some(node) => {
                if !scene.replace_node(node.clone()) {
                    scene.nodes.push(node.clone());
                }
            }
            None => {
                scene.remove_node(id);
            }
        }
    }
    if let Some(order) = order {
        reorder_to(scene, order);
    }
    if let Some((w, h, bg)) = canvas {
        scene.width = *w;
        scene.height = *h;
        scene.background = bg.clone();
    }
}

/// Rearranges nodes to `order`; nodes it does not name go on top.
fn reorder_to(scene: &mut FigureScene, order: &[String]) {
    let mut rest = std::mem::take(&mut scene.nodes);
    let mut out = Vec::with_capacity(rest.len());
    for id in order {
        if let Some(i) = rest.iter().position(|n| n.id() == id) {
            out.push(rest.remove(i));
        }
    }
    out.extend(rest);
    scene.nodes = out;
}

fn ids_of(scene: &FigureScene) -> Vec<String> {
    scene.nodes.iter().map(|n| n.id().to_owned()).collect()
}

/// A transaction: changes the scene and remembers how to undo them.
struct Tx<'a> {
    scene: &'a mut FigureScene,
    before: Vec<(String, Option<FigureNode>)>,
    seen: HashSet<String>,
    order_before: Option<Vec<String>>,
    canvas_before: Option<Canvas>,
}

impl<'a> Tx<'a> {
    fn new(scene: &'a mut FigureScene) -> Self {
        Self {
            scene,
            before: Vec::new(),
            seen: HashSet::new(),
            order_before: None,
            canvas_before: None,
        }
    }

    fn touch(&mut self, id: &str) {
        if self.seen.insert(id.to_owned()) {
            self.before
                .push((id.to_owned(), self.scene.node_by_id(id).cloned()));
        }
    }

    fn snapshot_order(&mut self) {
        if self.order_before.is_none() {
            self.order_before = Some(ids_of(self.scene));
        }
    }

    fn require(&self, id: &str) -> Result<&FigureNode, FigureError> {
        self.scene
            .node_by_id(id)
            .ok_or_else(|| FigureError::NodeNotFound(id.to_owned()))
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut FigureNode, FigureError> {
        self.require(id)?;
        self.touch(id);
        self.scene
            .node_mut(id)
            .ok_or_else(|| FigureError::NodeNotFound(id.to_owned()))
    }

    fn insert(&mut self, index: Option<usize>, mut node: FigureNode) -> Result<(), FigureError> {
        node.normalise_colours();
        node.validate()?;
        validate_colours(node.style())?;
        if self.scene.node_by_id(node.id()).is_some() {
            return Err(FigureError::DuplicateNode(node.id().to_owned()));
        }
        self.snapshot_order();
        self.touch(node.id());
        match index {
            Some(i) => {
                let i = i.min(self.scene.nodes.len());
                self.scene.nodes.insert(i, node);
            }
            None => self.scene.nodes.push(node),
        }
        Ok(())
    }

    fn remove(&mut self, id: &str) -> Result<FigureNode, FigureError> {
        self.require(id)?;
        self.snapshot_order();
        self.touch(id);
        self.scene
            .remove_node(id)
            .ok_or_else(|| FigureError::NodeNotFound(id.to_owned()))
    }

    fn set_order(&mut self, order: &[String]) {
        self.snapshot_order();
        reorder_to(self.scene, order);
    }

    fn set_canvas(&mut self, width: f64, height: f64, background: Option<&str>) {
        if self.canvas_before.is_none() {
            self.canvas_before = Some((
                self.scene.width,
                self.scene.height,
                self.scene.background.clone(),
            ));
        }
        self.scene.width = width;
        self.scene.height = height;
        match background {
            Some("none") => self.scene.background = None,
            Some(bg) => self.scene.background = Some(palette::normalize_colour(bg)),
            None => {}
        }
    }

    fn rollback(self) {
        apply_side(
            self.scene,
            &self.before,
            self.order_before.as_deref(),
            self.canvas_before.as_ref(),
        );
    }

    fn finish(self) -> Delta {
        let mut before = Vec::new();
        let mut after = Vec::new();
        for (id, old) in self.before {
            let new = self.scene.node_by_id(&id).cloned();
            if old != new {
                after.push((id.clone(), new));
                before.push((id, old));
            }
        }
        let order_after = ids_of(self.scene);
        let (order_before, order_after) = match self.order_before {
            Some(old) if old != order_after => (Some(old), Some(order_after)),
            _ => (None, None),
        };
        let current = (
            self.scene.width,
            self.scene.height,
            self.scene.background.clone(),
        );
        let (canvas_before, canvas_after) = match self.canvas_before {
            Some(old) if old != current => (Some(old), Some(current)),
            _ => (None, None),
        };
        Delta {
            before,
            after,
            order_before,
            order_after,
            canvas_before,
            canvas_after,
        }
    }
}

fn validate_colours(style: &Style) -> Result<(), FigureError> {
    for value in style.colours() {
        if !palette::is_colour(value) {
            return Err(FigureError::InvalidArgument(format!(
                "unknown colour `{value}`; use none, a palette token ({}) or #rrggbb",
                palette::token_names().join(", ")
            )));
        }
    }
    if let Some(o) = style.fill_opacity
        && !(0.0..=100.0).contains(&o)
    {
        return Err(FigureError::InvalidArgument(
            "fillOpacity must be between 0 and 100".to_owned(),
        ));
    }
    if let Some(o) = style.opacity
        && !(0.0..=100.0).contains(&o)
    {
        return Err(FigureError::InvalidArgument(
            "opacity must be between 0 and 100".to_owned(),
        ));
    }
    if let Some(w) = style.stroke_width
        && !(w.is_finite() && w > 0.0 && w <= 64.0)
    {
        return Err(FigureError::InvalidArgument(
            "strokeWidth must be between 0 and 64".to_owned(),
        ));
    }
    if let Some(s) = style.font_size
        && !(s.is_finite() && s > 0.0 && s <= 512.0)
    {
        return Err(FigureError::InvalidArgument(
            "fontSize must be between 0 and 512".to_owned(),
        ));
    }
    Ok(())
}

fn finite(values: &[f64]) -> Result<(), FigureError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(FigureError::InvalidArgument(
            "every number must be finite".to_owned(),
        ))
    }
}

/// Applies `op` to `scene`. On error the scene is left exactly as it was.
/// `Undo` and `Redo` are history operations and are refused here.
pub fn apply(scene: &mut FigureScene, op: &FigureOp) -> Result<Delta, FigureError> {
    let mut tx = Tx::new(scene);
    match run(&mut tx, op).and_then(|()| reroute(&mut tx)) {
        Ok(()) => Ok(tx.finish()),
        Err(e) => {
            tx.rollback();
            Err(e)
        }
    }
}

fn run(tx: &mut Tx<'_>, op: &FigureOp) -> Result<(), FigureError> {
    match op {
        FigureOp::Undo | FigureOp::Redo => Err(FigureError::InvalidArgument(
            "undo and redo cannot be part of a batch".to_owned(),
        )),
        FigureOp::Batch { ops } => ops.iter().try_for_each(|op| run(tx, op)),
        FigureOp::Add { nodes, index } => {
            for (k, node) in nodes.iter().enumerate() {
                tx.insert(index.map(|i| i + k), node.clone())?;
            }
            Ok(())
        }
        FigureOp::AddNode { node } => tx.insert(None, node.clone()),
        FigureOp::UpdateNode { node } => {
            let mut node = node.clone();
            node.normalise_colours();
            node.validate()?;
            validate_colours(node.style())?;
            let slot = tx.get_mut(node.id())?;
            *slot = node;
            Ok(())
        }
        FigureOp::Update { id, set } => update(tx, id, set),
        FigureOp::Remove { ids } => remove(tx, ids),
        FigureOp::RemoveNode { id } => remove(tx, std::slice::from_ref(id)),
        FigureOp::Move { ids, dx, dy } => move_nodes(tx, ids, *dx, *dy),
        FigureOp::Translate { id, dx, dy } => move_nodes(tx, std::slice::from_ref(id), *dx, *dy),
        FigureOp::Resize {
            id,
            x,
            y,
            width,
            height,
        } => {
            finite(&[*x, *y, *width, *height])?;
            resize(tx, id, normalize_rect(*x, *y, *width, *height))
        }
        FigureOp::Rotate { ids, angle } => {
            finite(&[*angle])?;
            let a = angle.rem_euclid(360.0);
            for id in ids {
                if let Some(s) = tx.get_mut(id)?.shape_mut() {
                    s.angle = a;
                }
            }
            Ok(())
        }
        FigureOp::Reorder { ids, to } => reorder(tx, ids, *to),
        FigureOp::Group { ids, group } => {
            if ids.len() < 2 {
                return Err(FigureError::InvalidArgument(
                    "a group needs at least two nodes".to_owned(),
                ));
            }
            let gid = match group {
                Some(g) if !g.is_empty() => g.clone(),
                _ => next_group_id(tx.scene),
            };
            for id in ids {
                let groups = tx.get_mut(id)?.groups_mut();
                if !groups.contains(&gid) {
                    groups.push(gid.clone());
                }
            }
            Ok(())
        }
        FigureOp::Ungroup { ids } => {
            for id in ids {
                tx.get_mut(id)?.groups_mut().pop();
            }
            Ok(())
        }
        FigureOp::Bind {
            id,
            end,
            node,
            anchor,
        } => bind(tx, id, *end, node.as_deref(), *anchor),
        FigureOp::Align { ids, edge } => align(tx, ids, *edge),
        FigureOp::Distribute { ids, axis } => distribute(tx, ids, *axis),
        FigureOp::SetStyle {
            ids,
            style,
            id,
            stroke,
            fill,
            size,
            color,
        } => {
            let mut targets = ids.clone();
            targets.extend(id.iter().cloned());
            let legacy = Style {
                stroke: stroke.clone().or_else(|| color.clone()),
                fill: fill.clone(),
                ..Style::default()
            };
            let mut patch = style.clone().unwrap_or_default().merged(&legacy);
            validate_colours(&patch)?;
            patch.normalise_colours();
            if let Some(s) = size {
                finite(&[*s])?;
            }
            for target in &targets {
                let node = tx.get_mut(target)?;
                let merged = node.style().merged(&patch);
                *node.style_mut() = merged;
                if let Some(s) = size {
                    match node {
                        FigureNode::Freedraw(f) => f.size = *s,
                        FigureNode::Math(m) => m.size = *s,
                        _ => {}
                    }
                }
            }
            Ok(())
        }
        FigureOp::SetCanvas {
            width,
            height,
            background,
        } => {
            if let Some(bg) = background
                && !palette::is_colour(bg)
            {
                return Err(FigureError::InvalidArgument(format!(
                    "the background `{bg}` is not none, a palette token or #rrggbb"
                )));
            }
            if !(width.is_finite() && height.is_finite() && *width > 0.0 && *height > 0.0) {
                return Err(FigureError::InvalidArgument(
                    "the page size must be positive and finite".to_owned(),
                ));
            }
            tx.set_canvas(*width, *height, background.as_deref());
            Ok(())
        }
        FigureOp::Clear => {
            let ids = ids_of(tx.scene);
            for id in ids {
                tx.remove(&id)?;
            }
            Ok(())
        }
    }
}

/// `g<base36>` one past the highest group id in use.
pub fn next_group_id(scene: &FigureScene) -> String {
    let highest = scene
        .nodes
        .iter()
        .flat_map(|n| n.groups().iter())
        .filter_map(|g| g.strip_prefix('g'))
        .filter_map(|d| usize::from_str_radix(d, 36).ok())
        .max()
        .unwrap_or(0);
    format!("g{}", base36(highest + 1))
}

fn update(tx: &mut Tx<'_>, id: &str, set: &Map<String, Value>) -> Result<(), FigureError> {
    let current = tx.require(id)?;
    let mut value = serde_json::to_value(current)?;
    let Value::Object(fields) = &mut value else {
        return Err(FigureError::InvalidArgument(
            "node is not an object".to_owned(),
        ));
    };
    for (key, v) in set {
        match (key.as_str(), v) {
            ("id", Value::String(new)) if new == id => {}
            ("id", _) => {
                return Err(FigureError::InvalidArgument(
                    "a node's id cannot change".to_owned(),
                ));
            }
            ("style", Value::Object(patch)) => {
                let style = fields
                    .entry("style".to_owned())
                    .or_insert_with(|| Value::Object(Map::new()));
                if let Value::Object(style) = style {
                    for (k, sv) in patch {
                        if sv.is_null() {
                            style.remove(k);
                        } else {
                            style.insert(k.clone(), sv.clone());
                        }
                    }
                }
            }
            (_, Value::Null) => {
                fields.remove(key);
            }
            _ => {
                fields.insert(key.clone(), v.clone());
            }
        }
    }
    let mut node: FigureNode = serde_json::from_value(value)
        .map_err(|e| FigureError::InvalidArgument(format!("cannot update `{id}`: {e}")))?;
    node.normalise_colours();
    node.validate()?;
    validate_colours(node.style())?;
    *tx.get_mut(id)? = node;
    Ok(())
}

fn remove(tx: &mut Tx<'_>, ids: &[String]) -> Result<(), FigureError> {
    for id in ids {
        tx.remove(id)?;
    }
    let removed: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let bound: Vec<String> = tx
        .scene
        .nodes
        .iter()
        .filter_map(FigureNode::connector)
        .filter(|c| {
            [&c.start, &c.end]
                .into_iter()
                .flatten()
                .any(|b| removed.contains(b.node.as_str()))
        })
        .map(|c| c.id.clone())
        .collect();
    for cid in bound {
        if let Some(c) = tx.get_mut(&cid)?.connector_mut() {
            if c.start
                .as_ref()
                .is_some_and(|b| removed.contains(b.node.as_str()))
            {
                c.start = None;
            }
            if c.end
                .as_ref()
                .is_some_and(|b| removed.contains(b.node.as_str()))
            {
                c.end = None;
            }
        }
    }
    Ok(())
}

fn move_nodes(tx: &mut Tx<'_>, ids: &[String], dx: f64, dy: f64) -> Result<(), FigureError> {
    finite(&[dx, dy])?;
    for id in ids {
        tx.require(id)?;
    }
    if dx == 0.0 && dy == 0.0 {
        return Ok(());
    }
    let moving: HashSet<&str> = ids.iter().map(String::as_str).collect();
    for id in ids {
        let node = tx.get_mut(id)?;
        node.translate(dx, dy);
        // An arrow dragged on its own lets go of shapes that stay put.
        if let Some(c) = node.connector_mut() {
            if c.start
                .as_ref()
                .is_some_and(|b| !moving.contains(b.node.as_str()))
            {
                c.start = None;
            }
            if c.end
                .as_ref()
                .is_some_and(|b| !moving.contains(b.node.as_str()))
            {
                c.end = None;
            }
        }
    }
    Ok(())
}

fn scale_point(p: Vec2, from: &Rect, to: &Rect) -> Vec2 {
    let sx = if from.width > 0.0 {
        to.width / from.width
    } else {
        1.0
    };
    let sy = if from.height > 0.0 {
        to.height / from.height
    } else {
        1.0
    };
    Vec2::new(to.x + (p.x - from.x) * sx, to.y + (p.y - from.y) * sy)
}

fn resize(tx: &mut Tx<'_>, id: &str, to: Rect) -> Result<(), FigureError> {
    let from = node_rect(tx.require(id)?);
    let node = tx.get_mut(id)?;
    match node {
        FigureNode::Arrow(c) | FigureNode::Line(c) => {
            for p in &mut c.points {
                *p = scale_point(*p, &from, &to);
            }
        }
        FigureNode::Freedraw(f) => {
            for p in &mut f.points {
                let q = scale_point(Vec2::new(p.x, p.y), &from, &to);
                p.x = q.x;
                p.y = q.y;
            }
        }
        FigureNode::Math(m) => {
            // A formula scales as a whole: by whichever side moved more.
            let kw = if from.width > 0.0 && to.width > 0.0 {
                to.width / from.width
            } else {
                1.0
            };
            let kh = if from.height > 0.0 && to.height > 0.0 {
                to.height / from.height
            } else {
                1.0
            };
            let k = if (kw - 1.0).abs() > (kh - 1.0).abs() {
                kw
            } else {
                kh
            };
            m.size *= k;
            m.width = from.width * k;
            m.height = from.height * k;
            m.x = to.x;
            m.y = to.y;
        }
        other => {
            if let Some(s) = other.shape_mut() {
                s.x = to.x;
                s.y = to.y;
                s.width = to.width;
                s.height = to.height;
            }
        }
    }
    Ok(())
}

fn reorder(tx: &mut Tx<'_>, ids: &[String], to: ZOrder) -> Result<(), FigureError> {
    for id in ids {
        tx.require(id)?;
    }
    let chosen: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let mut order = ids_of(tx.scene);
    match to {
        ZOrder::Front | ZOrder::Back => {
            let (picked, rest): (Vec<String>, Vec<String>) = order
                .into_iter()
                .partition(|id| chosen.contains(id.as_str()));
            order = if to == ZOrder::Front {
                rest.into_iter().chain(picked).collect()
            } else {
                picked.into_iter().chain(rest).collect()
            };
        }
        ZOrder::Forward => {
            for i in (0..order.len().saturating_sub(1)).rev() {
                if chosen.contains(order[i].as_str()) && !chosen.contains(order[i + 1].as_str()) {
                    order.swap(i, i + 1);
                }
            }
        }
        ZOrder::Backward => {
            for i in 1..order.len() {
                if chosen.contains(order[i].as_str()) && !chosen.contains(order[i - 1].as_str()) {
                    order.swap(i, i - 1);
                }
            }
        }
    }
    tx.set_order(&order);
    Ok(())
}

fn bind(
    tx: &mut Tx<'_>,
    id: &str,
    end: End,
    target: Option<&str>,
    anchor: Option<Vec2>,
) -> Result<(), FigureError> {
    if tx.require(id)?.connector().is_none() {
        return Err(FigureError::InvalidArgument(format!(
            "`{id}` is not an arrow or a line"
        )));
    }
    if let Some(t) = target {
        let node = tx.require(t)?;
        if !node.is_bindable() || t == id {
            return Err(FigureError::InvalidArgument(format!(
                "an arrow cannot attach to `{t}` (a {})",
                node.kind()
            )));
        }
    }
    if let Some(a) = anchor
        && !((0.0..=1.0).contains(&a.x) && (0.0..=1.0).contains(&a.y))
    {
        return Err(FigureError::InvalidArgument(
            "an anchor is a fraction of the box: x and y between 0 and 1".to_owned(),
        ));
    }
    let binding = target.map(|t| Binding {
        node: t.to_owned(),
        anchor,
    });
    if let Some(c) = tx.get_mut(id)?.connector_mut() {
        match end {
            End::Start => c.start = binding,
            End::End => c.end = binding,
        }
    }
    Ok(())
}

fn bounds_of(tx: &Tx<'_>, ids: &[String]) -> Result<Vec<(String, Rect)>, FigureError> {
    ids.iter()
        .map(|id| Ok((id.clone(), node_bounds(tx.require(id)?))))
        .collect()
}

fn align(tx: &mut Tx<'_>, ids: &[String], edge: AlignEdge) -> Result<(), FigureError> {
    if ids.len() < 2 {
        return Err(FigureError::InvalidArgument(
            "align needs at least two nodes".to_owned(),
        ));
    }
    let boxes = bounds_of(tx, ids)?;
    let all = boxes
        .iter()
        .fold(Rect::EMPTY, |acc, (_, b)| union_rect(&acc, b));
    for (id, b) in boxes {
        let (dx, dy) = match edge {
            AlignEdge::Left => (all.x - b.x, 0.0),
            AlignEdge::Center => (all.center().x - b.center().x, 0.0),
            AlignEdge::Right => (all.right() - b.right(), 0.0),
            AlignEdge::Top => (0.0, all.y - b.y),
            AlignEdge::Middle => (0.0, all.center().y - b.center().y),
            AlignEdge::Bottom => (0.0, all.bottom() - b.bottom()),
        };
        if dx != 0.0 || dy != 0.0 {
            tx.get_mut(&id)?.translate(dx, dy);
        }
    }
    Ok(())
}

fn distribute(tx: &mut Tx<'_>, ids: &[String], axis: Axis) -> Result<(), FigureError> {
    if ids.len() < 3 {
        return Err(FigureError::InvalidArgument(
            "distribute needs at least three nodes".to_owned(),
        ));
    }
    let mut boxes = bounds_of(tx, ids)?;
    let key = |r: &Rect| if axis == Axis::Horizontal { r.x } else { r.y };
    boxes.sort_by(|a, b| key(&a.1).total_cmp(&key(&b.1)).then_with(|| a.0.cmp(&b.0)));
    let size = |r: &Rect| {
        if axis == Axis::Horizontal {
            r.width
        } else {
            r.height
        }
    };
    let first = boxes[0].1;
    let last = boxes[boxes.len() - 1].1;
    let span = key(&last) + size(&last) - key(&first);
    let total: f64 = boxes.iter().map(|(_, b)| size(b)).sum();
    let gap = (span - total) / (boxes.len() - 1) as f64;
    let mut at = key(&first);
    for (id, b) in boxes {
        let d = at - key(&b);
        at += size(&b) + gap;
        if d.abs() > 1e-9 {
            let (dx, dy) = if axis == Axis::Horizontal {
                (d, 0.0)
            } else {
                (0.0, d)
            };
            tx.get_mut(&id)?.translate(dx, dy);
        }
    }
    Ok(())
}

/// Re-routes every connector touched in this transaction or bound to a
/// shape touched in it.
fn reroute(tx: &mut Tx<'_>) -> Result<(), FigureError> {
    let touched: HashSet<String> = tx.seen.clone();
    if touched.is_empty() {
        return Ok(());
    }
    let mut updates = Vec::new();
    for node in &tx.scene.nodes {
        let Some(c) = node.connector() else {
            continue;
        };
        if c.start.is_none() && c.end.is_none() {
            continue;
        }
        let relevant = touched.contains(&c.id)
            || [&c.start, &c.end]
                .into_iter()
                .flatten()
                .any(|b| touched.contains(&b.node));
        if !relevant {
            continue;
        }
        let points = route(tx.scene, c);
        if points != c.points {
            updates.push((c.id.clone(), points));
        }
    }
    for (id, points) in updates {
        if let Some(c) = tx.get_mut(&id)?.connector_mut() {
            c.points = points;
        }
    }
    Ok(())
}
