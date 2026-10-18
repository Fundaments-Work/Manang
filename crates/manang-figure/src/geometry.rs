//! Figure geometry: bounds, rotation, hit testing, outlines and freehand
//! outlines. Mirrored by `src/lib/figure/geometry.ts`.

use perfect_freehand::{InputPoint, StrokeOptions, get_stroke};

use crate::scene::{FigureNode, FigureScene, FreedrawNode, Rect, ShapeNode, Vec2};
use crate::text;

/// Hit tolerance for thin things (lines, outlines of frames), in scene units.
pub const HIT_TOLERANCE: f64 = 6.0;

/// Euclidean distance.
pub fn distance(a: Vec2, b: Vec2) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// Rounds to one decimal, without negative zero. Halves round up, like
/// JavaScript's `Math.round`, so both sides format the same digits.
pub fn round_coord(value: f64) -> f64 {
    let rounded = (value * 10.0 + 0.5).floor() / 10.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

/// Formats a coordinate with stable precision ("10" or "10.5").
pub fn format_coord(value: f64) -> String {
    let rounded = round_coord(value);
    if (rounded - rounded.round()).abs() < 1e-6 {
        format!("{rounded:.0}")
    } else {
        format!("{rounded:.1}")
    }
}

/// A rectangle with non-negative size.
pub fn normalize_rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x: if width < 0.0 { x + width } else { x },
        y: if height < 0.0 { y + height } else { y },
        width: width.abs(),
        height: height.abs(),
    }
}

/// The smallest rectangle holding both; an empty rectangle is ignored.
pub fn union_rect(a: &Rect, b: &Rect) -> Rect {
    if a.width == 0.0 && a.height == 0.0 {
        return *b;
    }
    if b.width == 0.0 && b.height == 0.0 {
        return *a;
    }
    let min_x = a.x.min(b.x);
    let min_y = a.y.min(b.y);
    Rect::new(
        min_x,
        min_y,
        a.right().max(b.right()) - min_x,
        a.bottom().max(b.bottom()) - min_y,
    )
}

/// The bounding box of points.
pub fn points_bounds(points: impl IntoIterator<Item = Vec2>) -> Rect {
    let mut iter = points.into_iter();
    let Some(first) = iter.next() else {
        return Rect::EMPTY;
    };
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
    for p in iter {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// `p` rotated by `degrees` (clockwise in screen coordinates) about `c`.
pub fn rotate_point(p: Vec2, c: Vec2, degrees: f64) -> Vec2 {
    if degrees == 0.0 {
        return p;
    }
    let (s, cos) = degrees.to_radians().sin_cos();
    let (dx, dy) = (p.x - c.x, p.y - c.y);
    Vec2::new(c.x + dx * cos - dy * s, c.y + dx * s + dy * cos)
}

/// A node's own box: the unrotated box of box kinds, the points' bounds of
/// connectors and freehand strokes, the text box of formulas.
pub fn node_rect(node: &FigureNode) -> Rect {
    match node {
        FigureNode::Arrow(c) | FigureNode::Line(c) => points_bounds(c.points.iter().copied()),
        FigureNode::Freedraw(f) => freedraw_bounds(f),
        FigureNode::Math(m) => {
            let width = text::measure(&m.tex, m.size, crate::scene::FontFamily::Mono);
            Rect::new(m.x, m.y - m.size, width, m.size * 1.4)
        }
        other => match other.shape() {
            Some(s) => normalize_rect(s.x, s.y, s.width, s.height),
            None => Rect::EMPTY,
        },
    }
}

fn freedraw_bounds(f: &FreedrawNode) -> Rect {
    let b = points_bounds(f.points.iter().map(|p| Vec2::new(p.x, p.y)));
    if f.points.is_empty() {
        return b;
    }
    let pad = f.size / 2.0;
    Rect::new(
        b.x - pad,
        b.y - pad,
        b.width + pad * 2.0,
        b.height + pad * 2.0,
    )
}

/// The rotation of a node in degrees (box kinds only).
pub fn node_angle(node: &FigureNode) -> f64 {
    node.shape().map_or(0.0, |s| s.angle)
}

/// Axis-aligned bounds, rotation included.
pub fn node_bounds(node: &FigureNode) -> Rect {
    let rect = node_rect(node);
    let angle = node_angle(node);
    if angle == 0.0 {
        return rect;
    }
    let c = rect.center();
    points_bounds(
        corners(&rect)
            .into_iter()
            .map(|p| rotate_point(p, c, angle)),
    )
}

/// Corners clockwise from top-left.
pub fn corners(r: &Rect) -> [Vec2; 4] {
    [
        Vec2::new(r.x, r.y),
        Vec2::new(r.right(), r.y),
        Vec2::new(r.right(), r.bottom()),
        Vec2::new(r.x, r.bottom()),
    ]
}

/// Bounds of every node.
pub fn scene_bounds(scene: &FigureScene) -> Rect {
    scene
        .nodes
        .iter()
        .fold(Rect::EMPTY, |acc, n| union_rect(&acc, &node_bounds(n)))
}

/// Distance from `p` to the segment `a`-`b`.
pub fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return distance(p, a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0);
    distance(p, Vec2::new(a.x + t * dx, a.y + t * dy))
}

/// Whether `p` is inside the shape's outline (in its own, unrotated frame).
fn inside_outline(node: &FigureNode, s: &ShapeNode, p: Vec2) -> bool {
    let r = normalize_rect(s.x, s.y, s.width, s.height);
    let c = r.center();
    let (hw, hh) = (r.width / 2.0, r.height / 2.0);
    match node {
        FigureNode::Diamond(_) => {
            hw > 0.0 && hh > 0.0 && (p.x - c.x).abs() / hw + (p.y - c.y).abs() / hh <= 1.0
        }
        FigureNode::Ellipse(_) => {
            hw > 0.0 && hh > 0.0 && {
                let (nx, ny) = ((p.x - c.x) / hw, (p.y - c.y) / hh);
                nx * nx + ny * ny <= 1.0
            }
        }
        _ => r.contains(p),
    }
}

/// Whether `p` hits `node`, with `tolerance` for thin parts.
pub fn hits(node: &FigureNode, p: Vec2, tolerance: f64) -> bool {
    match node {
        FigureNode::Arrow(c) | FigureNode::Line(c) => c.points.windows(2).any(|w| {
            distance_to_segment(p, w[0], w[1]) <= tolerance + c.style.stroke_width() / 2.0
        }),
        FigureNode::Freedraw(f) => {
            let reach = tolerance + f.size / 2.0;
            f.points.windows(2).any(|w| {
                distance_to_segment(p, Vec2::new(w[0].x, w[0].y), Vec2::new(w[1].x, w[1].y))
                    <= reach
            }) || (f.points.len() == 1
                && distance(p, Vec2::new(f.points[0].x, f.points[0].y)) <= reach)
        }
        FigureNode::Math(_) => node_rect(node).contains(p),
        FigureNode::Frame(s) => {
            // A frame is picked by its edge or its title, so what it holds
            // stays clickable.
            let local = rotate_point(p, s.rect().center(), -s.angle);
            let r = normalize_rect(s.x, s.y, s.width, s.height);
            let title = Rect::new(r.x, r.y - 22.0, r.width.max(40.0), 22.0);
            let outer = Rect::new(
                r.x - tolerance,
                r.y - tolerance,
                r.width + 2.0 * tolerance,
                r.height + 2.0 * tolerance,
            );
            let inner = Rect::new(
                r.x + tolerance,
                r.y + tolerance,
                (r.width - 2.0 * tolerance).max(0.0),
                (r.height - 2.0 * tolerance).max(0.0),
            );
            title.contains(local) || (outer.contains(local) && !inner.contains(local))
        }
        other => match other.shape() {
            Some(s) => {
                let local = rotate_point(p, s.rect().center(), -s.angle);
                inside_outline(other, s, local)
            }
            None => false,
        },
    }
}

/// The topmost node at `p`. Frames lose to anything they contain.
pub fn hit_test(scene: &FigureScene, point: Vec2) -> Option<&FigureNode> {
    let mut frame_hit = None;
    for node in scene.nodes.iter().rev() {
        if !hits(node, point, HIT_TOLERANCE) {
            continue;
        }
        if matches!(node, FigureNode::Frame(_)) {
            frame_hit.get_or_insert(node);
            continue;
        }
        return Some(node);
    }
    frame_hit
}

/// Every node entirely inside `rect` (marquee selection), in paint order.
pub fn nodes_in_rect<'a>(scene: &'a FigureScene, rect: &Rect) -> Vec<&'a FigureNode> {
    scene
        .nodes
        .iter()
        .filter(|n| rect.contains_rect(&node_bounds(n)))
        .collect()
}

/// Ids of the nodes a frame holds: everything (but frames) whose bounds lie
/// inside it.
pub fn frame_children(scene: &FigureScene, frame_id: &str) -> Vec<String> {
    let Some(frame) = scene.node_by_id(frame_id) else {
        return Vec::new();
    };
    let area = node_bounds(frame);
    scene
        .nodes
        .iter()
        .filter(|n| n.id() != frame_id && !matches!(n, FigureNode::Frame(_)))
        .filter(|n| area.contains_rect(&node_bounds(n)))
        .map(|n| n.id().to_owned())
        .collect()
}

/// Where the ray from the shape's centre toward `toward` leaves its
/// outline. Rotation is honoured. For a degenerate direction, the centre.
pub fn outline_point(node: &FigureNode, toward: Vec2) -> Vec2 {
    let Some(s) = node.shape() else {
        return node_rect(node).center();
    };
    let r = normalize_rect(s.x, s.y, s.width, s.height);
    let c = r.center();
    let local = rotate_point(toward, c, -s.angle);
    let (dx, dy) = (local.x - c.x, local.y - c.y);
    let (hw, hh) = (r.width / 2.0, r.height / 2.0);
    if (dx == 0.0 && dy == 0.0) || hw <= 0.0 || hh <= 0.0 {
        return c;
    }
    // Scale t so that c + t*(dx,dy) lies on the outline.
    let t = match node {
        FigureNode::Diamond(_) => 1.0 / (dx.abs() / hw + dy.abs() / hh),
        FigureNode::Ellipse(_) => 1.0 / ((dx / hw).powi(2) + (dy / hh).powi(2)).sqrt(),
        _ => {
            let tx = if dx == 0.0 {
                f64::INFINITY
            } else {
                hw / dx.abs()
            };
            let ty = if dy == 0.0 {
                f64::INFINITY
            } else {
                hh / dy.abs()
            };
            tx.min(ty)
        }
    };
    rotate_point(Vec2::new(c.x + dx * t, c.y + dy * t), c, s.angle)
}

/// The point a fixed anchor (fractions of the box) names, projected onto
/// the outline from the centre. The centre anchor stays the centre.
pub fn anchor_point(node: &FigureNode, anchor: Vec2) -> Vec2 {
    let rect = node_rect(node);
    let c = rect.center();
    let inner = Vec2::new(
        rect.x + anchor.x * rect.width,
        rect.y + anchor.y * rect.height,
    );
    if distance(inner, c) < 1e-9 {
        return c;
    }
    let angle = node_angle(node);
    outline_point(node, rotate_point(inner, c, angle))
}

/// The anchor fractions of a point relative to a shape's (unrotated) box.
pub fn anchor_of(node: &FigureNode, p: Vec2) -> Vec2 {
    let rect = node_rect(node);
    let local = rotate_point(p, rect.center(), -node_angle(node));
    let fx = if rect.width > 0.0 {
        (local.x - rect.x) / rect.width
    } else {
        0.5
    };
    let fy = if rect.height > 0.0 {
        (local.y - rect.y) / rect.height
    } else {
        0.5
    };
    Vec2::new(fx.clamp(0.0, 1.0), fy.clamp(0.0, 1.0))
}

/// The SVG path of a freehand stroke's outline (perfect-freehand).
pub fn freedraw_path(node: &FreedrawNode) -> String {
    if node.points.is_empty() {
        return String::new();
    }
    let input: Vec<InputPoint> = node
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
        ..StrokeOptions::default()
    };
    let outline = get_stroke(&input, &options);
    if outline.len() < 2 {
        return String::new();
    }
    let mut d = String::new();
    for (i, p) in outline.iter().enumerate() {
        d.push(if i == 0 { 'M' } else { 'L' });
        d.push_str(&format_coord(p[0]));
        d.push(' ');
        d.push_str(&format_coord(p[1]));
    }
    d.push('Z');
    d
}

/// A closed polygon as SVG path data.
pub fn polygon_to_path(points: &[Vec2]) -> String {
    if points.is_empty() {
        return String::new();
    }
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        d.push(if i == 0 { 'M' } else { 'L' });
        d.push_str(&format_coord(p.x));
        d.push(' ');
        d.push_str(&format_coord(p.y));
    }
    d.push('Z');
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::ShapeNode;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> FigureNode {
        FigureNode::Rect(ShapeNode::new("r", x, y, w, h))
    }

    #[test]
    fn outline_points_land_on_each_outline() {
        let r = rect(0.0, 0.0, 100.0, 50.0);
        assert_eq!(
            outline_point(&r, Vec2::new(500.0, 25.0)),
            Vec2::new(100.0, 25.0)
        );
        let d = FigureNode::Diamond(ShapeNode::new("d", 0.0, 0.0, 100.0, 50.0));
        assert_eq!(
            outline_point(&d, Vec2::new(50.0, -100.0)),
            Vec2::new(50.0, 0.0)
        );
        let e = FigureNode::Ellipse(ShapeNode::new("e", 0.0, 0.0, 100.0, 100.0));
        let p = outline_point(&e, Vec2::new(200.0, 200.0));
        assert!((distance(p, Vec2::new(50.0, 50.0)) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn rotation_changes_bounds_and_hits() {
        let mut s = ShapeNode::new("r", 0.0, 0.0, 100.0, 20.0);
        s.angle = 90.0;
        let n = FigureNode::Rect(s);
        let b = node_bounds(&n);
        assert!((b.width - 20.0).abs() < 1e-9 && (b.height - 100.0).abs() < 1e-9);
        assert!(hits(&n, Vec2::new(50.0, -30.0), 0.0));
        assert!(!hits(&n, Vec2::new(5.0, 10.0), 0.0));
    }

    #[test]
    fn frames_lose_to_their_contents() {
        let mut scene = FigureScene::new(100.0, 100.0);
        scene.nodes.push(FigureNode::Rect(ShapeNode::new(
            "inner", 20.0, 20.0, 20.0, 20.0,
        )));
        scene
            .nodes
            .push(FigureNode::Frame(ShapeNode::new("f", 0.0, 0.0, 90.0, 90.0)));
        assert_eq!(
            hit_test(&scene, Vec2::new(30.0, 30.0)).map(FigureNode::id),
            Some("inner")
        );
        assert_eq!(
            hit_test(&scene, Vec2::new(1.0, 50.0)).map(FigureNode::id),
            Some("f")
        );
        assert_eq!(hit_test(&scene, Vec2::new(60.0, 60.0)), None);
        assert_eq!(frame_children(&scene, "f"), vec!["inner".to_owned()]);
    }

    #[test]
    fn anchors_round_trip() {
        let r = rect(10.0, 10.0, 100.0, 50.0);
        let a = anchor_of(&r, Vec2::new(110.0, 35.0));
        assert_eq!(a, Vec2::new(1.0, 0.5));
        assert_eq!(anchor_point(&r, a), Vec2::new(110.0, 35.0));
    }
}
