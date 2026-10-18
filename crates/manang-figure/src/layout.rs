//! Layered auto-layout of the shapes and the arrows between them, with
//! `dugong` (a port of dagre, MIT OR Apache-2.0). Produces moves, so a
//! layout is one ordinary, undoable operation and bound arrows re-route.

use std::collections::{BTreeMap, BTreeSet};

use dugong::graphlib::{Graph, GraphOptions};
use dugong::{EdgeLabel, GraphLabel, NodeLabel, RankDir};
use serde::{Deserialize, Serialize};

use crate::error::FigureError;
use crate::geometry::node_rect;
use crate::ops::FigureOp;
use crate::scene::{FigureNode, FigureScene, Rect};

/// Which way ranks run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    /// Top to bottom.
    #[default]
    TB,
    /// Left to right.
    LR,
    /// Bottom to top.
    BT,
    /// Right to left.
    RL,
}

/// Whether a node takes part in layout: shapes, not frames or connectors.
fn laid_out(node: &FigureNode) -> bool {
    node.is_bindable() && !matches!(node, FigureNode::Frame(_)) && !node.locked()
}

/// The moves that lay out `only` (or every shape) as a layered graph,
/// keeping the group's top-left corner where it was. Empty when there is
/// nothing to move.
pub fn layout_moves(
    scene: &FigureScene,
    only: Option<&BTreeSet<String>>,
    direction: Direction,
    spacing: f64,
) -> Result<Vec<FigureOp>, FigureError> {
    let members: Vec<(&str, Rect)> = scene
        .nodes
        .iter()
        .filter(|n| laid_out(n))
        .filter(|n| only.is_none_or(|set| set.contains(n.id())))
        .map(|n| (n.id(), node_rect(n)))
        .collect();
    if members.is_empty() {
        return Ok(Vec::new());
    }
    let ids: BTreeSet<&str> = members.iter().map(|(id, _)| *id).collect();
    let mut g: Graph<NodeLabel, EdgeLabel, GraphLabel> = Graph::new(GraphOptions {
        multigraph: false,
        compound: false,
        directed: true,
    });
    g.set_graph(GraphLabel {
        rankdir: match direction {
            Direction::TB => RankDir::TB,
            Direction::LR => RankDir::LR,
            Direction::BT => RankDir::BT,
            Direction::RL => RankDir::RL,
        },
        nodesep: spacing,
        ranksep: spacing * 1.2,
        edgesep: spacing / 3.0,
        ..GraphLabel::default()
    });
    for (id, r) in &members {
        g.set_node(
            *id,
            NodeLabel {
                width: r.width.max(1.0),
                height: r.height.max(1.0),
                ..NodeLabel::default()
            },
        );
    }
    let mut seen = BTreeSet::new();
    for node in &scene.nodes {
        let Some(c) = node.connector() else { continue };
        let (Some(s), Some(e)) = (&c.start, &c.end) else {
            continue;
        };
        if s.node == e.node || !ids.contains(s.node.as_str()) || !ids.contains(e.node.as_str()) {
            continue;
        }
        if seen.insert((s.node.clone(), e.node.clone())) {
            g.set_edge_with_label(
                s.node.as_str(),
                e.node.as_str(),
                EdgeLabel {
                    weight: 1.0,
                    ..EdgeLabel::default()
                },
            );
        }
    }
    dugong::layout(&mut g)
        .map_err(|e| FigureError::InvalidArgument(format!("layout failed: {e}")))?;

    let mut placed: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for (id, r) in &members {
        if let Some(label) = g.node(id)
            && let (Some(cx), Some(cy)) = (label.x, label.y)
        {
            placed.insert(id, (cx - r.width / 2.0, cy - r.height / 2.0));
        }
    }
    // Keep the laid-out group where it was.
    let old_x = members
        .iter()
        .map(|(_, r)| r.x)
        .fold(f64::INFINITY, f64::min);
    let old_y = members
        .iter()
        .map(|(_, r)| r.y)
        .fold(f64::INFINITY, f64::min);
    let new_x = placed.values().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let new_y = placed.values().map(|p| p.1).fold(f64::INFINITY, f64::min);
    if !(old_x.is_finite() && new_x.is_finite()) {
        return Ok(Vec::new());
    }
    let mut ops = Vec::new();
    for (id, r) in &members {
        let Some((x, y)) = placed.get(id) else {
            continue;
        };
        let dx = (x - new_x + old_x) - r.x;
        let dy = (y - new_y + old_y) - r.y;
        if dx.abs() > 1e-6 || dy.abs() > 1e-6 {
            ops.push(FigureOp::Move {
                ids: vec![(*id).to_owned()],
                dx,
                dy,
            });
        }
    }
    Ok(ops)
}
