/**
 * Figure geometry: bounds, anchors, snapping, and stroke outline generation.
 *
 * Everything here is pure and side-effect free so it can be unit tested and,
 * later, mirrored by the Rust renderer without behavioural drift.
 */

import { getStroke } from "perfect-freehand";

import type {
  Anchor,
  AnchorSide,
  FigureNode,
  FigureScene,
  Rect,
  StrokeNode,
  Vec,
} from "./scene";

export const EMPTY_RECT: Rect = { x: 0, y: 0, width: 0, height: 0 };

export function rectCenter(rect: Rect): Vec {
  return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
}

export function distance(a: Vec, b: Vec): number {
  return Math.hypot(a.x - b.x, a.y - b.y);
}

/* -------------------------------------------------------------------------- */
/* Bounds                                                                      */
/* -------------------------------------------------------------------------- */

/**
 * Bounds of a single node. Stroke bounds are approximated from the base size
 * rather than the generated outline: the outline is a rendering detail, and the
 * Rust renderer must agree with this value or hit testing will drift.
 */
export function nodeBounds(node: FigureNode): Rect {
  switch (node.kind) {
    case "rect":
      return normalizeRect(node.x, node.y, node.width, node.height);
    case "ellipse":
      return normalizeRect(
        node.cx - node.rx,
        node.cy - node.ry,
        node.rx * 2,
        node.ry * 2,
      );
    case "stroke": {
      if (node.points.length === 0) return EMPTY_RECT;
      const pad = node.size / 2;
      const xs = node.points.map((point) => point.x);
      const ys = node.points.map((point) => point.y);
      const minX = Math.min(...xs) - pad;
      const minY = Math.min(...ys) - pad;
      return {
        x: minX,
        y: minY,
        width: Math.max(...xs) + pad - minX,
        height: Math.max(...ys) + pad - minY,
      };
    }
    case "arrow": {
      const pad = node.label ? 8 : 0;
      return normalizeRect(
        Math.min(node.from.x, node.to.x) - pad,
        Math.min(node.from.y, node.to.y) - pad,
        Math.abs(node.to.x - node.from.x) + pad * 2,
        Math.abs(node.to.y - node.from.y) + pad * 2,
      );
    }
    case "math":
      // Math is measured from its font size until the renderer reports real
      // metrics; an over-wide box only makes hit testing forgiving.
      return normalizeRect(
        node.x,
        node.y,
        node.size * node.tex.length * 0.62,
        node.size * 1.4,
      );
  }
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

export function sceneBounds(scene: FigureScene): Rect {
  return scene.nodes.reduce<Rect>(
    (acc, node) => unionRect(acc, nodeBounds(node)),
    EMPTY_RECT,
  );
}

export function hitTest(scene: FigureScene, point: Vec): FigureNode | undefined {
  // Topmost first: later nodes paint over earlier ones, so they win the hit.
  for (let index = scene.nodes.length - 1; index >= 0; index -= 1) {
    const node = scene.nodes[index];
    if (node === undefined) continue;
    const bounds = nodeBounds(node);
    if (
      point.x >= bounds.x &&
      point.x <= bounds.x + bounds.width &&
      point.y >= bounds.y &&
      point.y <= bounds.y + bounds.height
    ) {
      return node;
    }
  }
  return undefined;
}

/* -------------------------------------------------------------------------- */
/* Anchors & snapping — DESIGN.md §4.1 "magnetically snap to node boundaries"    */
/* -------------------------------------------------------------------------- */

/** Perimeter attachment points, in a fixed order so output is deterministic. */
const SIDES: readonly AnchorSide[] = ["top", "right", "bottom", "left"];

export function anchorsOf(node: FigureNode): Anchor[] {
  const bounds = nodeBounds(node);
  const { x, y, width: w, height: h } = bounds;
  const cx = x + w / 2;
  const cy = y + h / 2;

  return SIDES.map((side) => {
    switch (side) {
      case "top":
        return { nodeId: node.id, side, x: cx, y };
      case "right":
        return { nodeId: node.id, side, x: x + w, y: cy };
      case "bottom":
        return { nodeId: node.id, side, x: cx, y: y + h };
      case "left":
        return { nodeId: node.id, side, x, y: cy };
    }
  });
}

export function allAnchors(scene: FigureScene, exclude?: readonly string[]): Anchor[] {
  const excluded = new Set(exclude ?? []);
  return scene.nodes.filter((node) => !excluded.has(node.id)).flatMap(anchorsOf);
}

export interface SnapResult {
  readonly point: Vec;
  readonly anchor?: Anchor;
}

export const SNAP_DISTANCE = 12;

/**
 * Snaps a dragged endpoint to the nearest anchor within `threshold`.
 *
 * Returns the original point when nothing is close enough, so the magnet is a
 * convenience rather than a constraint that fights the user.
 */
export function snapToAnchors(
  point: Vec,
  anchors: readonly Anchor[],
  threshold: number = SNAP_DISTANCE,
): SnapResult {
  let best: Anchor | undefined;
  let bestDistance = threshold;

  for (const anchor of anchors) {
    const candidate = distance(point, anchor);
    if (candidate <= bestDistance) {
      bestDistance = candidate;
      best = anchor;
    }
  }

  return best === undefined
    ? { point }
    : { point: { x: best.x, y: best.y }, anchor: best };
}

/* -------------------------------------------------------------------------- */
/* Stroke outlines                                                             */
/* -------------------------------------------------------------------------- */

/**
 * Converts a freehand stroke to an SVG path.
 *
 * `perfect-freehand` returns the outline polygon as a flat list of points; a
 * closed cubic path through them is what gives the stroke its pressure-varying
 * edge. One decimal place is plenty at 100% zoom and keeps the exported SVG
 * small, which matters because the file is committed alongside the manuscript.
 */
export function strokeToPath(node: StrokeNode): string {
  if (node.points.length === 0) return "";

  const outline = getStroke(
    node.points.map((sample) => [sample.x, sample.y, sample.pressure] as const),
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

  const first = outline[0];
  const last = outline[outline.length - 1];
  if (first === undefined || last === undefined) return "";

  let d = `M${round(first[0] ?? 0)} ${round(first[1] ?? 0)}`;
  for (let index = 1; index < outline.length; index += 1) {
    const point = outline[index];
    if (point === undefined) continue;
    d += `L${round(point[0] ?? 0)} ${round(point[1] ?? 0)}`;
  }
  return `${d}Z`;
}

function round(value: number): number {
  return Math.round(value * 10) / 10;
}

/* -------------------------------------------------------------------------- */
/* Arrow head                                                                  */
/* -------------------------------------------------------------------------- */

/**
 * Arrow head as an explicit polygon.
 *
 * A marker would be simpler, but markers inherit stroke width and do not scale
 * predictably when the figure is embedded at 80% width in a Typst document, so
 * the head is geometry the Rust renderer can reproduce exactly.
 */
export function arrowHead(from: Vec, to: Vec, size = 7): readonly Vec[] {
  const angle = Math.atan2(to.y - from.y, to.x - from.x);
  const spread = Math.PI / 7;

  return [
    to,
    {
      x: to.x - size * Math.cos(angle - spread),
      y: to.y - size * Math.sin(angle - spread),
    },
    {
      x: to.x - size * 0.72 * Math.cos(angle),
      y: to.y - size * 0.72 * Math.sin(angle),
    },
    {
      x: to.x - size * Math.cos(angle + spread),
      y: to.y - size * Math.sin(angle + spread),
    },
  ];
}

export function polygonToPath(points: readonly Vec[]): string {
  if (points.length === 0) return "";
  return (
    points
      .map(
        (point, index) =>
          `${index === 0 ? "M" : "L"}${round(point.x)} ${round(point.y)}`,
      )
      .join("") + "Z"
  );
}
