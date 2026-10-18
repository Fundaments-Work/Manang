/**
 * Figure geometry: bounds (rotation included), hit testing, outlines,
 * anchors and freehand outlines. Mirrors `crates/manang-figure/src/geometry.rs`.
 */

import { getStroke } from "perfect-freehand";

import { measure } from "./text";
import {
  isShape,
  type FigureNode,
  type FigureScene,
  type FreedrawNode,
  type Rect,
  type ShapeNode,
  type Vec,
} from "./scene";

export const EMPTY_RECT: Rect = { x: 0, y: 0, width: 0, height: 0 };

/** Hit tolerance for thin things, in scene units. */
export const HIT_TOLERANCE = 6;

export function distance(a: Vec, b: Vec): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

/** One decimal, halves up (JavaScript `Math.round`), no negative zero. */
export function roundCoord(value: number): number {
  const rounded = Math.floor(value * 10 + 0.5) / 10;
  return rounded === 0 ? 0 : rounded;
}

/** Stable coordinate text: "10" or "10.5", as Rust's `format_coord`. */
export function formatCoord(value: number): string {
  const rounded = roundCoord(value);
  return Math.abs(rounded - Math.round(rounded)) < 1e-6
    ? rounded.toFixed(0)
    : rounded.toFixed(1);
}

export function rectCenter(rect: Rect): Vec {
  return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
}

/** Normalises a possibly inverted rect so width and height are non-negative. */
export function normalizeRect(
  x: number,
  y: number,
  width: number,
  height: number,
): Rect {
  return {
    x: width < 0 ? x + width : x,
    y: height < 0 ? y + height : y,
    width: Math.abs(width),
    height: Math.abs(height),
  };
}

export function unionRect(a: Rect, b: Rect): Rect {
  if (a.width === 0 && a.height === 0) return b;
  if (b.width === 0 && b.height === 0) return a;
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return {
    x,
    y,
    width: Math.max(a.x + a.width, b.x + b.width) - x,
    height: Math.max(a.y + a.height, b.y + b.height) - y,
  };
}

export function rectContains(r: Rect, p: Vec): boolean {
  return p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height;
}

export function rectContainsRect(outer: Rect, inner: Rect): boolean {
  return (
    inner.x >= outer.x &&
    inner.y >= outer.y &&
    inner.x + inner.width <= outer.x + outer.width &&
    inner.y + inner.height <= outer.y + outer.height
  );
}

export function rectsIntersect(a: Rect, b: Rect): boolean {
  return (
    a.x <= b.x + b.width &&
    b.x <= a.x + a.width &&
    a.y <= b.y + b.height &&
    b.y <= a.y + a.height
  );
}

export function pointsBounds(points: readonly Vec[]): Rect {
  const first = points[0];
  if (first === undefined) return EMPTY_RECT;
  let [x0, y0, x1, y1] = [first.x, first.y, first.x, first.y];
  for (const p of points) {
    x0 = Math.min(x0, p.x);
    y0 = Math.min(y0, p.y);
    x1 = Math.max(x1, p.x);
    y1 = Math.max(y1, p.y);
  }
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

/** `p` rotated by `degrees` (clockwise on screen) about `c`. */
export function rotatePoint(p: Vec, c: Vec, degrees: number): Vec {
  if (degrees === 0) return p;
  const rad = (degrees * Math.PI) / 180;
  const s = Math.sin(rad);
  const cos = Math.cos(rad);
  const dx = p.x - c.x;
  const dy = p.y - c.y;
  return { x: c.x + dx * cos - dy * s, y: c.y + dx * s + dy * cos };
}

/** Corners clockwise from top-left. */
export function corners(r: Rect): Vec[] {
  return [
    { x: r.x, y: r.y },
    { x: r.x + r.width, y: r.y },
    { x: r.x + r.width, y: r.y + r.height },
    { x: r.x, y: r.y + r.height },
  ];
}

function freedrawBounds(f: FreedrawNode): Rect {
  const b = pointsBounds(f.points);
  if (f.points.length === 0) return b;
  const pad = f.size / 2;
  return {
    x: b.x - pad,
    y: b.y - pad,
    width: b.width + pad * 2,
    height: b.height + pad * 2,
  };
}

/** A node's own (unrotated) box. */
export function nodeRect(node: FigureNode): Rect {
  switch (node.kind) {
    case "arrow":
    case "line":
      return pointsBounds(node.points);
    case "freedraw":
      return freedrawBounds(node);
    case "math":
      return {
        x: node.x,
        y: node.y - node.size,
        width: measure(node.tex, node.size, "mono"),
        height: node.size * 1.4,
      };
    default:
      return normalizeRect(node.x, node.y, node.width, node.height);
  }
}

export function nodeAngle(node: FigureNode): number {
  return isShape(node) ? (node.angle ?? 0) : 0;
}

/** Axis-aligned bounds, rotation included. */
export function nodeBounds(node: FigureNode): Rect {
  const rect = nodeRect(node);
  const angle = nodeAngle(node);
  if (angle === 0) return rect;
  const c = rectCenter(rect);
  return pointsBounds(corners(rect).map((p) => rotatePoint(p, c, angle)));
}

export function sceneBounds(scene: FigureScene): Rect {
  return scene.nodes.reduce<Rect>(
    (acc, n) => unionRect(acc, nodeBounds(n)),
    EMPTY_RECT,
  );
}

export function distanceToSegment(p: Vec, a: Vec, b: Vec): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len2 = dx * dx + dy * dy;
  if (len2 === 0) return distance(p, a);
  const t = Math.min(Math.max(((p.x - a.x) * dx + (p.y - a.y) * dy) / len2, 0), 1);
  return distance(p, { x: a.x + t * dx, y: a.y + t * dy });
}

function insideOutline(s: ShapeNode, p: Vec): boolean {
  const r = normalizeRect(s.x, s.y, s.width, s.height);
  const c = rectCenter(r);
  const hw = r.width / 2;
  const hh = r.height / 2;
  switch (s.kind) {
    case "diamond":
      return (
        hw > 0 && hh > 0 && Math.abs(p.x - c.x) / hw + Math.abs(p.y - c.y) / hh <= 1
      );
    case "ellipse": {
      if (!(hw > 0 && hh > 0)) return false;
      const nx = (p.x - c.x) / hw;
      const ny = (p.y - c.y) / hh;
      return nx * nx + ny * ny <= 1;
    }
    default:
      return rectContains(r, p);
  }
}

function shapeCenter(s: ShapeNode): Vec {
  return { x: s.x + s.width / 2, y: s.y + s.height / 2 };
}

/** Whether `p` hits `node`, with `tolerance` for thin parts. */
export function hits(node: FigureNode, p: Vec, tolerance: number): boolean {
  switch (node.kind) {
    case "arrow":
    case "line": {
      const reach = tolerance + (node.style?.strokeWidth ?? 1.5) / 2;
      for (let i = 0; i + 1 < node.points.length; i += 1) {
        if (
          distanceToSegment(p, node.points[i] as Vec, node.points[i + 1] as Vec) <=
          reach
        ) {
          return true;
        }
      }
      return false;
    }
    case "freedraw": {
      const reach = tolerance + node.size / 2;
      for (let i = 0; i + 1 < node.points.length; i += 1) {
        if (
          distanceToSegment(p, node.points[i] as Vec, node.points[i + 1] as Vec) <=
          reach
        ) {
          return true;
        }
      }
      const only = node.points[0];
      return (
        node.points.length === 1 && only !== undefined && distance(p, only) <= reach
      );
    }
    case "math":
      return rectContains(nodeRect(node), p);
    case "frame": {
      const local = rotatePoint(p, shapeCenter(node), -(node.angle ?? 0));
      const r = normalizeRect(node.x, node.y, node.width, node.height);
      const title = { x: r.x, y: r.y - 22, width: Math.max(r.width, 40), height: 22 };
      const outer = {
        x: r.x - tolerance,
        y: r.y - tolerance,
        width: r.width + 2 * tolerance,
        height: r.height + 2 * tolerance,
      };
      const inner = {
        x: r.x + tolerance,
        y: r.y + tolerance,
        width: Math.max(r.width - 2 * tolerance, 0),
        height: Math.max(r.height - 2 * tolerance, 0),
      };
      return (
        rectContains(title, local) ||
        (rectContains(outer, local) && !rectContains(inner, local))
      );
    }
    default: {
      const local = rotatePoint(p, shapeCenter(node), -(node.angle ?? 0));
      return insideOutline(node, local);
    }
  }
}

/** The topmost node at `point`; frames lose to anything they contain. */
export function hitTest(scene: FigureScene, point: Vec): FigureNode | undefined {
  let frameHit: FigureNode | undefined;
  for (let i = scene.nodes.length - 1; i >= 0; i -= 1) {
    const node = scene.nodes[i];
    if (node === undefined || !hits(node, point, HIT_TOLERANCE)) continue;
    if (node.kind === "frame") {
      frameHit ??= node;
      continue;
    }
    return node;
  }
  return frameHit;
}

/** Every node entirely inside `rect` (marquee selection), in paint order. */
export function nodesInRect(scene: FigureScene, rect: Rect): FigureNode[] {
  return scene.nodes.filter((n) => rectContainsRect(rect, nodeBounds(n)));
}

/** Ids of the nodes (not frames) whose bounds lie inside the frame. */
export function frameChildren(scene: FigureScene, frameId: string): string[] {
  const frame = scene.nodes.find((n) => n.id === frameId);
  if (frame === undefined) return [];
  const area = nodeBounds(frame);
  return scene.nodes
    .filter((n) => n.id !== frameId && n.kind !== "frame")
    .filter((n) => rectContainsRect(area, nodeBounds(n)))
    .map((n) => n.id);
}

/** Where the ray from the shape's centre toward `toward` leaves its outline. */
export function outlinePoint(node: FigureNode, toward: Vec): Vec {
  if (!isShape(node)) return rectCenter(nodeRect(node));
  const r = normalizeRect(node.x, node.y, node.width, node.height);
  const c = rectCenter(r);
  const angle = node.angle ?? 0;
  const local = rotatePoint(toward, c, -angle);
  const dx = local.x - c.x;
  const dy = local.y - c.y;
  const hw = r.width / 2;
  const hh = r.height / 2;
  if ((dx === 0 && dy === 0) || hw <= 0 || hh <= 0) return c;
  let t: number;
  if (node.kind === "diamond") {
    t = 1 / (Math.abs(dx) / hw + Math.abs(dy) / hh);
  } else if (node.kind === "ellipse") {
    t = 1 / Math.sqrt((dx / hw) ** 2 + (dy / hh) ** 2);
  } else {
    const tx = dx === 0 ? Infinity : hw / Math.abs(dx);
    const ty = dy === 0 ? Infinity : hh / Math.abs(dy);
    t = Math.min(tx, ty);
  }
  return rotatePoint({ x: c.x + dx * t, y: c.y + dy * t }, c, angle);
}

/** The outline point a fixed anchor names; the centre anchor stays the centre. */
export function anchorPoint(node: FigureNode, anchor: Vec): Vec {
  const rect = nodeRect(node);
  const c = rectCenter(rect);
  const inner = {
    x: rect.x + anchor.x * rect.width,
    y: rect.y + anchor.y * rect.height,
  };
  if (distance(inner, c) < 1e-9) return c;
  return outlinePoint(node, rotatePoint(inner, c, nodeAngle(node)));
}

/** Anchor fractions of a point relative to a shape's (unrotated) box. */
export function anchorOf(node: FigureNode, p: Vec): Vec {
  const rect = nodeRect(node);
  const local = rotatePoint(p, rectCenter(rect), -nodeAngle(node));
  const fx = rect.width > 0 ? (local.x - rect.x) / rect.width : 0.5;
  const fy = rect.height > 0 ? (local.y - rect.y) / rect.height : 0.5;
  const clamp = (v: number) => Math.min(Math.max(v, 0), 1);
  return { x: clamp(fx), y: clamp(fy) };
}

/** The outline of a freehand stroke (perfect-freehand) as SVG path data. */
export function freedrawPath(node: Pick<FreedrawNode, "points" | "size">): string {
  if (node.points.length === 0) return "";
  const outline = getStroke(
    node.points.map((s) => [s.x, s.y, s.pressure] as const),
    {
      size: node.size,
      thinning: 0.6,
      smoothing: 0.5,
      streamline: 0.4,
      simulatePressure: true,
      last: true,
    },
  );
  if (outline.length < 2) return "";
  let d = "";
  outline.forEach((p, i) => {
    d += `${i === 0 ? "M" : "L"}${formatCoord(p[0] ?? 0)} ${formatCoord(p[1] ?? 0)}`;
  });
  return `${d}Z`;
}

/** A closed polygon as SVG path data. */
export function polygonToPath(points: readonly Vec[]): string {
  if (points.length === 0) return "";
  return (
    points
      .map((p, i) => `${i === 0 ? "M" : "L"}${formatCoord(p.x)} ${formatCoord(p.y)}`)
      .join("") + "Z"
  );
}
