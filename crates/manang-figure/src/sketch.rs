//! Outline path data for every shape, clean or hand-drawn.
//!
//! The hand-drawn style follows rough.js's method (MIT): each edge is drawn
//! twice with jittered endpoints and a bowed middle; ellipses are closed
//! curves through jittered points. Randomness comes from a xorshift32
//! generator seeded with a hash of the node id, so a shape looks the same
//! every time it is drawn. `src/lib/figure/sketch.ts` is a line-for-line
//! mirror (same arithmetic, same order) so the canvas shows what is exported.

use crate::geometry::format_coord;
use crate::scene::{Rect, Routing, Vec2};

/// A small deterministic random source.
#[derive(Debug, Clone)]
pub struct Rng(u32);

impl Rng {
    /// Seeded from a node id (FNV-1a over its UTF-8 bytes).
    pub fn for_id(id: &str) -> Self {
        let mut h: u32 = 0x811c_9dc5;
        for b in id.bytes() {
            h ^= u32::from(b);
            h = h.wrapping_mul(16_777_619);
        }
        Self(if h == 0 { 1 } else { h })
    }

    /// The next value in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        f64::from(x) / 4_294_967_296.0
    }

    /// A value in `[-amount, amount)`.
    pub fn jitter(&mut self, amount: f64) -> f64 {
        amount * (self.unit() * 2.0 - 1.0)
    }
}

fn pt(d: &mut String, cmd: char, p: Vec2) {
    d.push(cmd);
    d.push_str(&format_coord(p.x));
    d.push(' ');
    d.push_str(&format_coord(p.y));
}

/// One jittered pass of a line from `a` to `b`.
fn rough_pass(d: &mut String, a: Vec2, b: Vec2, roughness: f64, scale: f64, rng: &mut Rng) {
    let len = (b.x - a.x).hypot(b.y - a.y);
    let offset = (roughness * 1.5).min(len / 8.0) * scale;
    let bow = roughness * len * 0.015 * scale;
    let a2 = Vec2::new(a.x + rng.jitter(offset), a.y + rng.jitter(offset));
    let b2 = Vec2::new(b.x + rng.jitter(offset), b.y + rng.jitter(offset));
    let (nx, ny) = if len > 0.0 {
        (-(b.y - a.y) / len, (b.x - a.x) / len)
    } else {
        (0.0, 0.0)
    };
    let k = rng.jitter(bow);
    let m = Vec2::new((a2.x + b2.x) / 2.0 + nx * k, (a2.y + b2.y) / 2.0 + ny * k);
    pt(d, 'M', a2);
    pt(d, 'Q', m);
    d.push(' ');
    d.push_str(&format_coord(b2.x));
    d.push(' ');
    d.push_str(&format_coord(b2.y));
}

/// A line, clean or hand-drawn (two passes).
pub fn line(d: &mut String, a: Vec2, b: Vec2, roughness: u8, rng: &mut Rng) {
    if roughness == 0 {
        pt(d, 'M', a);
        pt(d, 'L', b);
        return;
    }
    let r = f64::from(roughness);
    rough_pass(d, a, b, r, 1.0, rng);
    rough_pass(d, a, b, r, 1.6, rng);
}

/// A closed polygon outline.
pub fn polygon(points: &[Vec2], roughness: u8, rng: &mut Rng) -> String {
    let mut d = String::new();
    if points.is_empty() {
        return d;
    }
    if roughness == 0 {
        for (i, p) in points.iter().enumerate() {
            pt(&mut d, if i == 0 { 'M' } else { 'L' }, *p);
        }
        d.push('Z');
        return d;
    }
    for i in 0..points.len() {
        line(
            &mut d,
            points[i],
            points[(i + 1) % points.len()],
            roughness,
            rng,
        );
    }
    d
}

/// The four corners of a box, clockwise from top-left.
pub fn rect_points(r: &Rect) -> [Vec2; 4] {
    crate::geometry::corners(r)
}

/// The four vertices of the diamond inscribed in a box.
pub fn diamond_points(r: &Rect) -> [Vec2; 4] {
    let c = r.center();
    [
        Vec2::new(c.x, r.y),
        Vec2::new(r.right(), c.y),
        Vec2::new(c.x, r.bottom()),
        Vec2::new(r.x, c.y),
    ]
}

/// The ellipse inscribed in a box.
pub fn ellipse(r: &Rect, roughness: u8, rng: &mut Rng) -> String {
    let c = r.center();
    let (rx, ry) = (r.width / 2.0, r.height / 2.0);
    let mut d = String::new();
    if roughness == 0 {
        pt(&mut d, 'M', Vec2::new(c.x - rx, c.y));
        for end in [Vec2::new(c.x + rx, c.y), Vec2::new(c.x - rx, c.y)] {
            d.push_str(&format!(
                "A{} {} 0 1 0 {} {}",
                format_coord(rx),
                format_coord(ry),
                format_coord(end.x),
                format_coord(end.y)
            ));
        }
        d.push('Z');
        return d;
    }
    let rough = f64::from(roughness);
    for pass in 0..2 {
        let steps = 12;
        let start = rng.unit() * std::f64::consts::TAU;
        let spread = 0.04 * rough * if pass == 0 { 1.0 } else { 1.6 };
        let mut pts = Vec::with_capacity(steps + 3);
        // One step past the start, so the two ends overlap like a pen.
        for i in 0..=steps + 1 {
            let t = start + std::f64::consts::TAU * (i as f64) / (steps as f64);
            let k = 1.0 + rng.jitter(spread);
            pts.push(Vec2::new(c.x + rx * k * t.cos(), c.y + ry * k * t.sin()));
        }
        catmull_rom(&mut d, &pts);
    }
    d
}

/// A smooth curve through `points` (Catmull-Rom as cubic Béziers).
pub fn catmull_rom(d: &mut String, points: &[Vec2]) {
    let Some(first) = points.first() else {
        return;
    };
    pt(d, 'M', *first);
    for i in 0..points.len().saturating_sub(1) {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(points.len() - 1)];
        let c1 = Vec2::new(p1.x + (p2.x - p0.x) / 6.0, p1.y + (p2.y - p0.y) / 6.0);
        let c2 = Vec2::new(p2.x - (p3.x - p1.x) / 6.0, p2.y - (p3.y - p1.y) / 6.0);
        cubic(d, c1, c2, p2);
    }
}

fn cubic(d: &mut String, c1: Vec2, c2: Vec2, p: Vec2) {
    pt(d, 'C', c1);
    for q in [c2, p] {
        d.push(' ');
        d.push_str(&format_coord(q.x));
        d.push(' ');
        d.push_str(&format_coord(q.y));
    }
}

/// The control points of a two-point curved connector: along the exit and
/// entry directions when the ends are bound, else a straight line.
pub fn curve_controls(s: Vec2, e: Vec2, ds: Option<Vec2>, de: Option<Vec2>) -> (Vec2, Vec2) {
    let reach = (e.x - s.x).hypot(e.y - s.y) / 2.5;
    let c1 = match ds {
        Some(v) => Vec2::new(s.x + v.x * reach, s.y + v.y * reach),
        None => Vec2::new(s.x + (e.x - s.x) / 3.0, s.y + (e.y - s.y) / 3.0),
    };
    let c2 = match de {
        Some(v) => Vec2::new(e.x + v.x * reach, e.y + v.y * reach),
        None => Vec2::new(e.x + (s.x - e.x) / 3.0, e.y + (s.y - e.y) / 3.0),
    };
    (c1, c2)
}

/// Path data of a connector, and the direction it arrives at each end
/// (pointing out of the path), for heads.
pub fn connector(
    points: &[Vec2],
    routing: Routing,
    ds: Option<Vec2>,
    de: Option<Vec2>,
    roughness: u8,
    rng: &mut Rng,
) -> (String, Vec2, Vec2) {
    let mut d = String::new();
    let n = points.len();
    if n < 2 {
        return (d, Vec2::new(0.0, 0.0), Vec2::new(0.0, 0.0));
    }
    let (s, e) = (points[0], points[n - 1]);
    if routing == Routing::Curved {
        if n == 2 {
            let (c1, c2) = curve_controls(s, e, ds, de);
            let passes = if roughness == 0 { 1 } else { 2 };
            for pass in 0..passes {
                let j = f64::from(roughness) * 2.0 * f64::from(pass + 1);
                let c1j = Vec2::new(c1.x + rng.jitter(j), c1.y + rng.jitter(j));
                let c2j = Vec2::new(c2.x + rng.jitter(j), c2.y + rng.jitter(j));
                pt(&mut d, 'M', s);
                cubic(&mut d, c1j, c2j, e);
            }
            return (
                d,
                Vec2::new(s.x - c1.x, s.y - c1.y),
                Vec2::new(e.x - c2.x, e.y - c2.y),
            );
        }
        catmull_rom(&mut d, points);
        if roughness > 0 {
            let jittered: Vec<Vec2> = points
                .iter()
                .map(|p| {
                    let j = f64::from(roughness) * 1.5;
                    Vec2::new(p.x + rng.jitter(j), p.y + rng.jitter(j))
                })
                .collect();
            catmull_rom(&mut d, &jittered);
        }
        let (p1, pn) = (points[1], points[n - 2]);
        return (
            d,
            Vec2::new(s.x - p1.x, s.y - p1.y),
            Vec2::new(e.x - pn.x, e.y - pn.y),
        );
    }
    if roughness == 0 {
        for (i, p) in points.iter().enumerate() {
            pt(&mut d, if i == 0 { 'M' } else { 'L' }, *p);
        }
    } else {
        for w in points.windows(2) {
            line(&mut d, w[0], w[1], roughness, rng);
        }
    }
    let (p1, pn) = (points[1], points[n - 2]);
    (
        d,
        Vec2::new(s.x - p1.x, s.y - p1.y),
        Vec2::new(e.x - pn.x, e.y - pn.y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic_and_in_range() {
        let mut a = Rng::for_id("n1");
        let mut b = Rng::for_id("n1");
        for _ in 0..100 {
            let x = a.unit();
            assert_eq!(x, b.unit());
            assert!((0.0..1.0).contains(&x));
        }
        assert_ne!(Rng::for_id("n1").unit(), Rng::for_id("n2").unit());
    }

    #[test]
    fn clean_shapes_are_plain_paths() {
        let mut rng = Rng::for_id("x");
        let r = Rect::new(0.0, 0.0, 10.0, 20.0);
        assert_eq!(
            polygon(&rect_points(&r), 0, &mut rng),
            "M0 0L10 0L10 20L0 20Z"
        );
        assert_eq!(
            ellipse(&r, 0, &mut rng),
            "M0 10A5 10 0 1 0 10 10A5 10 0 1 0 0 10Z"
        );
        let rough = polygon(&rect_points(&r), 1, &mut rng);
        assert_eq!(rough.matches('M').count(), 8);
    }
}
