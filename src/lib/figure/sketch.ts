/**
 * Outline path data for every shape, clean or hand-drawn: a line-for-line
 * mirror of `sketch.rs` (same arithmetic in the same order), so the canvas
 * draws exactly the strokes the Rust export writes. The hand-drawn method
 * follows rough.js (MIT): two jittered passes per edge with a bowed middle.
 */

import { corners, formatCoord } from "./geometry";
import type { Rect, Routing, Vec } from "./scene";

const TAU = Math.PI * 2;

/** xorshift32 seeded with FNV-1a over the id's UTF-8 bytes. */
export class Rng {
  #state: number;

  constructor(state: number) {
    this.#state = state >>> 0;
  }

  static forId(id: string): Rng {
    let h = 0x811c9dc5;
    for (const b of new TextEncoder().encode(id)) {
      h = (h ^ b) >>> 0;
      h = Math.imul(h, 16777619) >>> 0;
    }
    return new Rng(h === 0 ? 1 : h);
  }

  /** The next value in [0, 1). */
  unit(): number {
    let x = this.#state;
    x = (x ^ (x << 13)) >>> 0;
    x = (x ^ (x >>> 17)) >>> 0;
    x = (x ^ (x << 5)) >>> 0;
    this.#state = x;
    return x / 4294967296;
  }

  /** A value in [-amount, amount). */
  jitter(amount: number): number {
    return amount * (this.unit() * 2 - 1);
  }
}

function pt(cmd: string, p: Vec): string {
  return `${cmd}${formatCoord(p.x)} ${formatCoord(p.y)}`;
}

function roughPass(a: Vec, b: Vec, roughness: number, scale: number, rng: Rng): string {
  const len = Math.hypot(b.x - a.x, b.y - a.y);
  const offset = Math.min(roughness * 1.5, len / 8) * scale;
  const bow = roughness * len * 0.015 * scale;
  const a2x = a.x + rng.jitter(offset);
  const a2y = a.y + rng.jitter(offset);
  const b2x = b.x + rng.jitter(offset);
  const b2y = b.y + rng.jitter(offset);
  const [nx, ny] = len > 0 ? [-(b.y - a.y) / len, (b.x - a.x) / len] : [0, 0];
  const k = rng.jitter(bow);
  const m = { x: (a2x + b2x) / 2 + nx * k, y: (a2y + b2y) / 2 + ny * k };
  return `${pt("M", { x: a2x, y: a2y })}${pt("Q", m)} ${formatCoord(b2x)} ${formatCoord(b2y)}`;
}

/** A line, clean or hand-drawn (two passes). */
export function line(a: Vec, b: Vec, roughness: number, rng: Rng): string {
  if (roughness === 0) return pt("M", a) + pt("L", b);
  return roughPass(a, b, roughness, 1, rng) + roughPass(a, b, roughness, 1.6, rng);
}

/** A closed polygon outline. */
export function polygon(points: readonly Vec[], roughness: number, rng: Rng): string {
  if (points.length === 0) return "";
  if (roughness === 0) {
    return points.map((p, i) => pt(i === 0 ? "M" : "L", p)).join("") + "Z";
  }
  let d = "";
  for (let i = 0; i < points.length; i += 1) {
    const a = points[i];
    const b = points[(i + 1) % points.length];
    if (a !== undefined && b !== undefined) d += line(a, b, roughness, rng);
  }
  return d;
}

export function rectPoints(r: Rect): Vec[] {
  return corners(r);
}

export function diamondPoints(r: Rect): Vec[] {
  const cx = r.x + r.width / 2;
  const cy = r.y + r.height / 2;
  return [
    { x: cx, y: r.y },
    { x: r.x + r.width, y: cy },
    { x: cx, y: r.y + r.height },
    { x: r.x, y: cy },
  ];
}

/** The ellipse inscribed in a box. */
export function ellipse(r: Rect, roughness: number, rng: Rng): string {
  const cx = r.x + r.width / 2;
  const cy = r.y + r.height / 2;
  const rx = r.width / 2;
  const ry = r.height / 2;
  if (roughness === 0) {
    let d = pt("M", { x: cx - rx, y: cy });
    for (const end of [
      { x: cx + rx, y: cy },
      { x: cx - rx, y: cy },
    ]) {
      d += `A${formatCoord(rx)} ${formatCoord(ry)} 0 1 0 ${formatCoord(end.x)} ${formatCoord(end.y)}`;
    }
    return `${d}Z`;
  }
  let d = "";
  for (let pass = 0; pass < 2; pass += 1) {
    const steps = 12;
    const start = rng.unit() * TAU;
    const spread = 0.04 * roughness * (pass === 0 ? 1 : 1.6);
    const pts: Vec[] = [];
    for (let i = 0; i <= steps + 1; i += 1) {
      const t = start + (TAU * i) / steps;
      const k = 1 + rng.jitter(spread);
      pts.push({ x: cx + rx * k * Math.cos(t), y: cy + ry * k * Math.sin(t) });
    }
    d += catmullRom(pts);
  }
  return d;
}

/** A smooth curve through `points` (Catmull-Rom as cubic Béziers). */
export function catmullRom(points: readonly Vec[]): string {
  const first = points[0];
  if (first === undefined) return "";
  let d = pt("M", first);
  const last = points.length - 1;
  for (let i = 0; i < points.length - 1; i += 1) {
    const p0 = points[Math.max(i - 1, 0)] as Vec;
    const p1 = points[i] as Vec;
    const p2 = points[i + 1] as Vec;
    const p3 = points[Math.min(i + 2, last)] as Vec;
    const c1 = { x: p1.x + (p2.x - p0.x) / 6, y: p1.y + (p2.y - p0.y) / 6 };
    const c2 = { x: p2.x - (p3.x - p1.x) / 6, y: p2.y - (p3.y - p1.y) / 6 };
    d += cubic(c1, c2, p2);
  }
  return d;
}

function cubic(c1: Vec, c2: Vec, p: Vec): string {
  return `${pt("C", c1)} ${formatCoord(c2.x)} ${formatCoord(c2.y)} ${formatCoord(p.x)} ${formatCoord(p.y)}`;
}

/** Control points of a two-point curved connector. */
export function curveControls(
  s: Vec,
  e: Vec,
  ds: Vec | undefined,
  de: Vec | undefined,
): [Vec, Vec] {
  const reach = Math.hypot(e.x - s.x, e.y - s.y) / 2.5;
  const c1 =
    ds !== undefined
      ? { x: s.x + ds.x * reach, y: s.y + ds.y * reach }
      : { x: s.x + (e.x - s.x) / 3, y: s.y + (e.y - s.y) / 3 };
  const c2 =
    de !== undefined
      ? { x: e.x + de.x * reach, y: e.y + de.y * reach }
      : { x: e.x + (s.x - e.x) / 3, y: e.y + (s.y - e.y) / 3 };
  return [c1, c2];
}

export interface ConnectorPath {
  readonly d: string;
  /** Direction out of the path at the first point (for heads). */
  readonly startDir: Vec;
  /** Direction out of the path at the last point. */
  readonly endDir: Vec;
}

/** Path data of a connector and its end directions. */
export function connectorPath(
  points: readonly Vec[],
  routing: Routing,
  ds: Vec | undefined,
  de: Vec | undefined,
  roughness: number,
  rng: Rng,
): ConnectorPath {
  const n = points.length;
  const s = points[0];
  const e = points[n - 1];
  if (n < 2 || s === undefined || e === undefined) {
    return { d: "", startDir: { x: 0, y: 0 }, endDir: { x: 0, y: 0 } };
  }
  const p1 = points[1] as Vec;
  const pn = points[n - 2] as Vec;
  if (routing === "curved") {
    if (n === 2) {
      const [c1, c2] = curveControls(s, e, ds, de);
      const passes = roughness === 0 ? 1 : 2;
      let d = "";
      for (let pass = 0; pass < passes; pass += 1) {
        const j = roughness * 2 * (pass + 1);
        const c1j = { x: c1.x + rng.jitter(j), y: c1.y + rng.jitter(j) };
        const c2j = { x: c2.x + rng.jitter(j), y: c2.y + rng.jitter(j) };
        d += pt("M", s) + cubic(c1j, c2j, e);
      }
      return {
        d,
        startDir: { x: s.x - c1.x, y: s.y - c1.y },
        endDir: { x: e.x - c2.x, y: e.y - c2.y },
      };
    }
    let d = catmullRom(points);
    if (roughness > 0) {
      const jittered = points.map((p) => {
        const j = roughness * 1.5;
        return { x: p.x + rng.jitter(j), y: p.y + rng.jitter(j) };
      });
      d += catmullRom(jittered);
    }
    return {
      d,
      startDir: { x: s.x - p1.x, y: s.y - p1.y },
      endDir: { x: e.x - pn.x, y: e.y - pn.y },
    };
  }
  let d = "";
  if (roughness === 0) {
    d = points.map((p, i) => pt(i === 0 ? "M" : "L", p)).join("");
  } else {
    for (let i = 0; i + 1 < n; i += 1) {
      d += line(points[i] as Vec, points[i + 1] as Vec, roughness, rng);
    }
  }
  return {
    d,
    startDir: { x: s.x - p1.x, y: s.y - p1.y },
    endDir: { x: e.x - pn.x, y: e.y - pn.y },
  };
}
