/**
 * Arrow binding and routing; mirrors `crates/manang-figure/src/route.rs`.
 * The canvas runs this during a drag so bound arrows follow live; the plugin
 * runs the Rust version on release.
 */

import {
  anchorPoint,
  distance,
  nodeAngle,
  nodeRect,
  outlinePoint,
  rectCenter,
  rotatePoint,
} from "./geometry";
import {
  isBindable,
  type Binding,
  type ConnectorNode,
  type FigureNode,
  type FigureScene,
  type Vec,
} from "./scene";

/** Space between a bound end and the outline. */
export const BIND_GAP = 4;

/** Rust's `f64::signum`: +0 is positive. */
function signum(x: number): number {
  if (Number.isNaN(x)) return Number.NaN;
  return x > 0 || Object.is(x, 0) ? 1 : -1;
}

function target(
  scene: FigureScene,
  binding: Binding | undefined,
): FigureNode | undefined {
  if (binding === undefined) return undefined;
  const node = scene.nodes.find((n) => n.id === binding.node);
  return node !== undefined && isBindable(node) ? node : undefined;
}

function unit(v: Vec): Vec | undefined {
  const len = Math.hypot(v.x, v.y);
  return len > 1e-9 ? { x: v.x / len, y: v.y / len } : undefined;
}

function boundPoint(node: FigureNode, binding: Binding, toward: Vec): Vec {
  const c = rectCenter(nodeRect(node));
  const p =
    binding.anchor !== undefined
      ? anchorPoint(node, binding.anchor)
      : outlinePoint(node, toward);
  const u = unit({ x: p.x - c.x, y: p.y - c.y });
  return u === undefined ? p : { x: p.x + u.x * BIND_GAP, y: p.y + u.y * BIND_GAP };
}

function aim(node: FigureNode, binding: Binding): Vec {
  return binding.anchor !== undefined
    ? anchorPoint(node, binding.anchor)
    : rectCenter(nodeRect(node));
}

/** The outward normal of the side of `node` that `p` sits on. */
export function sideNormal(node: FigureNode, p: Vec): Vec {
  const r = nodeRect(node);
  const c = rectCenter(r);
  const angle = nodeAngle(node);
  const local = rotatePoint(p, c, -angle);
  const hw = Math.max(r.width / 2, 1e-9);
  const hh = Math.max(r.height / 2, 1e-9);
  const nx = (local.x - c.x) / hw;
  const ny = (local.y - c.y) / hh;
  const n =
    Math.abs(nx) >= Math.abs(ny) ? { x: signum(nx), y: 0 } : { x: 0, y: signum(ny) };
  const q = rotatePoint({ x: c.x + n.x, y: c.y + n.y }, c, angle);
  return { x: q.x - c.x, y: q.y - c.y };
}

/** Exit directions of both ends: the bound side's normal, else undefined. */
export function endDirections(
  scene: FigureScene,
  c: ConnectorNode,
): [Vec | undefined, Vec | undefined] {
  const first = c.points[0];
  const last = c.points[c.points.length - 1];
  const st = target(scene, c.start);
  const et = target(scene, c.end);
  return [
    st !== undefined && first !== undefined ? sideNormal(st, first) : undefined,
    et !== undefined && last !== undefined ? sideNormal(et, last) : undefined,
  ];
}

/** The connector's points after re-routing its bound ends. */
export function route(scene: FigureScene, c: ConnectorNode): Vec[] {
  const n = c.points.length;
  if (n < 2) return [...c.points];
  const rawStart = c.points[0] as Vec;
  const rawEnd = c.points[n - 1] as Vec;
  const routing = c.routing ?? "straight";
  const interior: Vec[] = routing === "elbow" ? [] : c.points.slice(1, n - 1);
  const st = target(scene, c.start);
  const et = target(scene, c.end);
  const sb = c.start;
  const eb = c.end;

  if (
    st !== undefined &&
    et !== undefined &&
    sb !== undefined &&
    eb !== undefined &&
    st.id === et.id &&
    sb.anchor === undefined &&
    eb.anchor === undefined
  ) {
    const r = nodeRect(st);
    const cc = rectCenter(r);
    const a = boundPoint(st, sb, { x: cc.x + r.width, y: cc.y - r.height / 4 });
    const b = boundPoint(st, eb, { x: cc.x + r.width, y: cc.y + r.height / 4 });
    const reach = r.x + r.width + 32;
    return [a, { x: reach, y: a.y }, { x: reach, y: b.y }, b];
  }

  const startAimOther = et !== undefined && eb !== undefined ? aim(et, eb) : rawEnd;
  const endAimOther = st !== undefined && sb !== undefined ? aim(st, sb) : rawStart;
  const start =
    st !== undefined && sb !== undefined
      ? boundPoint(st, sb, interior[0] ?? startAimOther)
      : rawStart;
  const end =
    et !== undefined && eb !== undefined
      ? boundPoint(et, eb, interior[interior.length - 1] ?? endAimOther)
      : rawEnd;

  if (routing === "elbow") {
    const ds = st !== undefined ? sideNormal(st, start) : undefined;
    const de = et !== undefined ? sideNormal(et, end) : undefined;
    return elbow(start, end, ds, de);
  }
  return [start, ...interior, end];
}

function horizontal(d: Vec | undefined, from: Vec, to: Vec): boolean {
  return d !== undefined
    ? Math.abs(d.x) >= Math.abs(d.y)
    : Math.abs(to.x - from.x) >= Math.abs(to.y - from.y);
}

/** An orthogonal path from `s` to `e` along the given exit directions. */
export function elbow(s: Vec, e: Vec, ds: Vec | undefined, de: Vec | undefined): Vec[] {
  const hs = horizontal(ds, s, e);
  const he = horizontal(de, e, s);
  let raw: Vec[];
  if (hs && he) {
    const mx = (s.x + e.x) / 2;
    raw = [s, { x: mx, y: s.y }, { x: mx, y: e.y }, e];
  } else if (!hs && !he) {
    const my = (s.y + e.y) / 2;
    raw = [s, { x: s.x, y: my }, { x: e.x, y: my }, e];
  } else if (hs) {
    raw = [s, { x: e.x, y: s.y }, e];
  } else {
    raw = [s, { x: s.x, y: e.y }, e];
  }
  return simplify(raw);
}

function simplify(points: readonly Vec[]): Vec[] {
  const out: Vec[] = [];
  for (const p of points) {
    const last = out[out.length - 1];
    if (last !== undefined && distance(last, p) < 1e-9) continue;
    if (out.length >= 2) {
      const a = out[out.length - 2] as Vec;
      const b = out[out.length - 1] as Vec;
      const cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
      if (Math.abs(cross) < 1e-9) out.pop();
    }
    out.push(p);
  }
  if (out.length === 1) out.push(out[0] as Vec);
  return out;
}
