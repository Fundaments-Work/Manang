use perfect_freehand::{InputPoint, StrokeOptions, get_stroke};
use std::collections::HashSet;

use crate::scene::{Anchor, AnchorSide, FigureNode, FigureScene, Rect, StrokeNode, Vec2};

/// Default magnetic snap threshold in scene units.
pub const SNAP_DISTANCE: f64 = 12.0;

/// Returns the center point of a rectangle.
pub fn rect_center(rect: &Rect) -> Vec2 {
    Vec2::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

/// Calculates Euclidean distance between two points.
pub fn distance(a: Vec2, b: Vec2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// Rounds a float coordinate to 1 decimal place matching JavaScript `Math.round(val * 10) / 10`.
pub fn round_coord(value: f64) -> f64 {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

/// Formats a float coordinate with stable, deterministic precision (e.g. "10" or "10.5").
pub fn format_coord(value: f64) -> String {
    let rounded = round_coord(value);
    if (rounded - rounded.round()).abs() < 1e-6 {
        format!("{:.0}", rounded)
    } else {
        format!("{:.1}", rounded)
    }
}

/// Normalizes a rectangle ensuring width and height are non-negative.
pub fn normalize_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x: if width < 0.0 { x + width } else { x },
        y: if height < 0.0 { y + height } else { y },
        width: width.abs(),
        height: height.abs(),
    }
}

/// Computes the bounding rectangle encompassing both rectangles.
pub fn union_rect(a: &Rect, b: &Rect) -> Rect {
    if a.width == 0.0 && a.height == 0.0 {
        return *b;
    }
    if b.width == 0.0 && b.height == 0.0 {
        return *a;
    }

    let min_x = a.x.min(b.x);
    let min_y = a.y.min(b.y);
    let max_x = (a.x + a.width).max(b.x + b.width);
    let max_y = (a.y + a.height).max(b.y + b.height);

    Rect {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

/// Computes the bounding box of a single figure node.
pub fn node_bounds(node: &FigureNode) -> Rect {
    match node {
        FigureNode::Rect(r) => normalize_rect(r.x, r.y, r.width, r.height),
        FigureNode::Ellipse(e) => normalize_rect(e.cx - e.rx, e.cy - e.ry, e.rx * 2.0, e.ry * 2.0),
        FigureNode::Stroke(s) => {
            if s.points.is_empty() {
                return Rect::EMPTY;
            }
            let pad = s.size / 2.0;
            let mut min_x = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_y = f64::NEG_INFINITY;

            for pt in &s.points {
                if pt.x < min_x {
                    min_x = pt.x;
                }
                if pt.x > max_x {
                    max_x = pt.x;
                }
                if pt.y < min_y {
                    min_y = pt.y;
                }
                if pt.y > max_y {
                    max_y = pt.y;
                }
            }

            min_x -= pad;
            max_x += pad;
            min_y -= pad;
            max_y += pad;

            Rect {
                x: min_x,
                y: min_y,
                width: max_x - min_x,
                height: max_y - min_y,
            }
        }
        FigureNode::Arrow(a) => {
            let pad = if a.label.is_some() { 8.0 } else { 0.0 };
            normalize_rect(
                a.from.x.min(a.to.x) - pad,
                a.from.y.min(a.to.y) - pad,
                (a.to.x - a.from.x).abs() + pad * 2.0,
                (a.to.y - a.from.y).abs() + pad * 2.0,
            )
        }
        FigureNode::Math(m) => normalize_rect(
            m.x,
            m.y,
            m.size * (m.tex.chars().count() as f64) * 0.62,
            m.size * 1.4,
        ),
    }
}

/// Computes the collective bounding box of all nodes in the scene.
pub fn scene_bounds(scene: &FigureScene) -> Rect {
    scene.nodes.iter().fold(Rect::EMPTY, |acc, node| {
        union_rect(&acc, &node_bounds(node))
    })
}

/// Tests which topmost node contains the given point, if any.
pub fn hit_test(scene: &FigureScene, point: Vec2) -> Option<&FigureNode> {
    for node in scene.nodes.iter().rev() {
        let b = node_bounds(node);
        if point.x >= b.x && point.x <= b.x + b.width && point.y >= b.y && point.y <= b.y + b.height
        {
            return Some(node);
        }
    }
    None
}

/// Computes the 4 perimeter anchor points of a node (top, right, bottom, left).
pub fn anchors_of(node: &FigureNode) -> Vec<Anchor> {
    let b = node_bounds(node);
    let cx = b.x + b.width / 2.0;
    let cy = b.y + b.height / 2.0;
    let id = node.id().to_owned();

    vec![
        Anchor {
            x: cx,
            y: b.y,
            side: AnchorSide::Top,
            node_id: id.clone(),
        },
        Anchor {
            x: b.x + b.width,
            y: cy,
            side: AnchorSide::Right,
            node_id: id.clone(),
        },
        Anchor {
            x: cx,
            y: b.y + b.height,
            side: AnchorSide::Bottom,
            node_id: id.clone(),
        },
        Anchor {
            x: b.x,
            y: cy,
            side: AnchorSide::Left,
            node_id: id,
        },
    ]
}

/// Collects all perimeter anchors across the scene, optionally excluding specific node IDs.
pub fn all_anchors(scene: &FigureScene, exclude: &[&str]) -> Vec<Anchor> {
    let excluded: HashSet<&str> = exclude.iter().copied().collect();
    scene
        .nodes
        .iter()
        .filter(|node| !excluded.contains(node.id()))
        .flat_map(anchors_of)
        .collect()
}

/// The result of an anchor snapping query.
#[derive(Debug, Clone, PartialEq)]
pub struct SnapResult {
    /// The snapped point (or original point if no anchor was close enough).
    pub point: Vec2,
    /// The matching anchor if snapped.
    pub anchor: Option<Anchor>,
}

/// Snaps a dragged endpoint to the nearest anchor within `threshold`.
pub fn snap_to_anchors(point: Vec2, anchors: &[Anchor], threshold: f64) -> SnapResult {
    let mut best: Option<&Anchor> = None;
    let mut best_dist = threshold;

    for anchor in anchors {
        let d = distance(point, Vec2::new(anchor.x, anchor.y));
        if d <= best_dist {
            best_dist = d;
            best = Some(anchor);
        }
    }

    match best {
        Some(a) => SnapResult {
            point: Vec2::new(a.x, a.y),
            anchor: Some(a.clone()),
        },
        None => SnapResult {
            point,
            anchor: None,
        },
    }
}

/// Computes the polygon vertices for an arrow head pointing from `from` to `to`.
pub fn arrow_head(from: Vec2, to: Vec2, size: f64) -> [Vec2; 4] {
    let angle = (to.y - from.y).atan2(to.x - from.x);
    let spread = std::f64::consts::PI / 7.0;

    [
        to,
        Vec2::new(
            to.x - size * (angle - spread).cos(),
            to.y - size * (angle - spread).sin(),
        ),
        Vec2::new(
            to.x - size * 0.72 * angle.cos(),
            to.y - size * 0.72 * angle.sin(),
        ),
        Vec2::new(
            to.x - size * (angle + spread).cos(),
            to.y - size * (angle + spread).sin(),
        ),
    ]
}

/// Converts a polygon vertex list to a closed SVG path definition `M...L...Z`.
pub fn polygon_to_path(points: &[Vec2]) -> String {
    if points.is_empty() {
        return String::new();
    }
    let mut d = String::new();
    for (i, pt) in points.iter().enumerate() {
        if i == 0 {
            d.push('M');
        } else {
            d.push('L');
        }
        d.push_str(&format_coord(pt.x));
        d.push(' ');
        d.push_str(&format_coord(pt.y));
    }
    d.push('Z');
    d
}

/// Generates a pressure-tapered SVG outline path for a freehand stroke using `perfect-freehand`.
pub fn stroke_to_path(node: &StrokeNode) -> String {
    if node.points.is_empty() {
        return String::new();
    }

    let input_points: Vec<InputPoint> = node
        .points
        .iter()
        .map(|p| InputPoint::Array([p.x, p.y], Some(p.pressure)))
        .collect();

    let options = StrokeOptions {
        size: Some(node.size),
        thinning: Some(0.6),
        smoothing: Some(0.5),
        streamline: Some(0.4),
        simulate_pressure: Some(true),
        last: Some(true),
        ..Default::default()
    };

    let outline = get_stroke(&input_points, &options);
    if outline.len() < 2 {
        return String::new();
    }

    let mut d = String::new();
    for (i, pt) in outline.iter().enumerate() {
        if i == 0 {
            d.push('M');
        } else {
            d.push('L');
        }
        d.push_str(&format_coord(pt[0]));
        d.push(' ');
        d.push_str(&format_coord(pt[1]));
    }
    d.push('Z');
    d
}
