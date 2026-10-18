//! Arrow binding and routing (ADR 0012, "Arrow binding"). Mirrored by
//! `src/lib/figure/binding.ts`.

use crate::geometry::{anchor_point, distance, node_rect, outline_point, rotate_point};
use crate::scene::{Binding, ConnectorNode, FigureNode, FigureScene, Routing, Vec2};

/// Space left between a bound end and the shape's outline.
pub const BIND_GAP: f64 = 4.0;

/// The bound shape, if the binding names a bindable node in the scene.
fn target<'a>(scene: &'a FigureScene, binding: Option<&Binding>) -> Option<&'a FigureNode> {
    let b = binding?;
    scene.node_by_id(&b.node).filter(|n| n.is_bindable())
}

fn unit(v: Vec2) -> Option<Vec2> {
    let len = v.x.hypot(v.y);
    (len > 1e-9).then(|| Vec2::new(v.x / len, v.y / len))
}

/// The point a bound end sits at, given the point it aims from.
fn bound_point(node: &FigureNode, binding: &Binding, toward: Vec2) -> Vec2 {
    let c = node_rect(node).center();
    let p = match binding.anchor {
        Some(anchor) => anchor_point(node, anchor),
        None => outline_point(node, toward),
    };
    match unit(Vec2::new(p.x - c.x, p.y - c.y)) {
        Some(u) => Vec2::new(p.x + u.x * BIND_GAP, p.y + u.y * BIND_GAP),
        None => p,
    }
}

/// The point the other end aims at: a fixed anchor, else the shape centre.
fn aim(node: &FigureNode, binding: &Binding) -> Vec2 {
    match binding.anchor {
        Some(anchor) => anchor_point(node, anchor),
        None => node_rect(node).center(),
    }
}

/// The outward direction of the side a point sits on (for elbow and curved
/// routing). Unbound ends have none.
pub fn side_normal(node: &FigureNode, p: Vec2) -> Vec2 {
    let r = node_rect(node);
    let c = r.center();
    let angle = crate::geometry::node_angle(node);
    let local = rotate_point(p, c, -angle);
    let (hw, hh) = ((r.width / 2.0).max(1e-9), (r.height / 2.0).max(1e-9));
    let (nx, ny) = ((local.x - c.x) / hw, (local.y - c.y) / hh);
    let n = if nx.abs() >= ny.abs() {
        Vec2::new(nx.signum(), 0.0)
    } else {
        Vec2::new(0.0, ny.signum())
    };
    let q = rotate_point(Vec2::new(c.x + n.x, c.y + n.y), c, angle);
    Vec2::new(q.x - c.x, q.y - c.y)
}

/// Exit directions of both ends: the bound side's normal, else none.
pub fn end_directions(scene: &FigureScene, c: &ConnectorNode) -> (Option<Vec2>, Option<Vec2>) {
    let first = c.points.first().copied();
    let last = c.points.last().copied();
    let ds = target(scene, c.start.as_ref())
        .zip(first)
        .map(|(n, p)| side_normal(n, p));
    let de = target(scene, c.end.as_ref())
        .zip(last)
        .map(|(n, p)| side_normal(n, p));
    (ds, de)
}

/// The connector's points after re-routing its bound ends. Unbound ends
/// stay where they are; interior points are kept except for elbow routing,
/// which is always recomputed.
pub fn route(scene: &FigureScene, c: &ConnectorNode) -> Vec<Vec2> {
    let n = c.points.len();
    if n < 2 {
        return c.points.clone();
    }
    let raw_start = c.points[0];
    let raw_end = c.points[n - 1];
    let interior: Vec<Vec2> = if c.routing == Routing::Elbow {
        Vec::new()
    } else {
        c.points[1..n - 1].to_vec()
    };
    let st = target(scene, c.start.as_ref());
    let et = target(scene, c.end.as_ref());

    // A shape bound to itself: a loop out of its right side.
    if let (Some(s), Some(e), Some(sb), Some(eb)) = (st, et, c.start.as_ref(), c.end.as_ref())
        && s.id() == e.id()
        && sb.anchor.is_none()
        && eb.anchor.is_none()
    {
        let r = node_rect(s);
        let cc = r.center();
        let a = bound_point(s, sb, Vec2::new(cc.x + r.width, cc.y - r.height / 4.0));
        let b = bound_point(s, eb, Vec2::new(cc.x + r.width, cc.y + r.height / 4.0));
        let reach = r.right() + 32.0;
        return vec![a, Vec2::new(reach, a.y), Vec2::new(reach, b.y), b];
    }

    let start_aim_other = match (et, c.end.as_ref()) {
        (Some(node), Some(b)) => aim(node, b),
        _ => raw_end,
    };
    let end_aim_other = match (st, c.start.as_ref()) {
        (Some(node), Some(b)) => aim(node, b),
        _ => raw_start,
    };
    let start = match (st, c.start.as_ref()) {
        (Some(node), Some(b)) => bound_point(
            node,
            b,
            interior.first().copied().unwrap_or(start_aim_other),
        ),
        _ => raw_start,
    };
    let end = match (et, c.end.as_ref()) {
        (Some(node), Some(b)) => {
            bound_point(node, b, interior.last().copied().unwrap_or(end_aim_other))
        }
        _ => raw_end,
    };

    if c.routing == Routing::Elbow {
        let ds = st.map(|node| side_normal(node, start));
        let de = et.map(|node| side_normal(node, end));
        return elbow(start, end, ds, de);
    }
    let mut out = Vec::with_capacity(interior.len() + 2);
    out.push(start);
    out.extend(interior);
    out.push(end);
    out
}

fn horizontal(d: Option<Vec2>, from: Vec2, to: Vec2) -> bool {
    match d {
        Some(v) => v.x.abs() >= v.y.abs(),
        None => (to.x - from.x).abs() >= (to.y - from.y).abs(),
    }
}

/// An orthogonal path from `s` to `e` leaving and entering along the given
/// directions.
pub fn elbow(s: Vec2, e: Vec2, ds: Option<Vec2>, de: Option<Vec2>) -> Vec<Vec2> {
    let hs = horizontal(ds, s, e);
    let he = horizontal(de, e, s);
    let raw = match (hs, he) {
        (true, true) => {
            let mx = (s.x + e.x) / 2.0;
            vec![s, Vec2::new(mx, s.y), Vec2::new(mx, e.y), e]
        }
        (false, false) => {
            let my = (s.y + e.y) / 2.0;
            vec![s, Vec2::new(s.x, my), Vec2::new(e.x, my), e]
        }
        (true, false) => vec![s, Vec2::new(e.x, s.y), e],
        (false, true) => vec![s, Vec2::new(s.x, e.y), e],
    };
    simplify(raw)
}

/// Drops repeated points and the middle of collinear runs.
fn simplify(points: Vec<Vec2>) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_some_and(|q| distance(*q, p) < 1e-9) {
            continue;
        }
        if out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
            if cross.abs() < 1e-9 {
                out.pop();
            }
        }
        out.push(p);
    }
    if out.len() == 1 {
        out.push(out[0]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::ShapeNode;

    fn scene() -> FigureScene {
        let mut s = FigureScene::new(800.0, 600.0);
        s.nodes
            .push(FigureNode::Rect(ShapeNode::new("a", 0.0, 0.0, 100.0, 50.0)));
        s.nodes.push(FigureNode::Rect(ShapeNode::new(
            "b", 300.0, 0.0, 100.0, 50.0,
        )));
        s
    }

    #[test]
    fn floating_ends_meet_the_outlines_with_a_gap() {
        let s = scene();
        let mut c = ConnectorNode::new("c", Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0));
        c.start = Some(Binding::to("a"));
        c.end = Some(Binding::to("b"));
        let pts = route(&s, &c);
        assert_eq!(pts, vec![Vec2::new(104.0, 25.0), Vec2::new(296.0, 25.0)]);
    }

    #[test]
    fn elbow_routes_are_orthogonal() {
        let pts = elbow(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0), None, None);
        assert_eq!(pts.len(), 4);
        for w in pts.windows(2) {
            assert!(w[0].x == w[1].x || w[0].y == w[1].y);
        }
        let straight = elbow(Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), None, None);
        assert_eq!(straight.len(), 2);
    }

    #[test]
    fn a_missing_target_leaves_the_end_alone() {
        let s = scene();
        let mut c = ConnectorNode::new("c", Vec2::new(5.0, 5.0), Vec2::new(9.0, 9.0));
        c.end = Some(Binding::to("ghost"));
        assert_eq!(route(&s, &c), c.points);
    }
}
