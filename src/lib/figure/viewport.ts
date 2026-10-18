/**
 * The infinite canvas's view: which scene point sits at the canvas's top
 * left corner and how far it is zoomed. Lives in the editor only, never in
 * the scene file. Screen coordinates here are relative to the canvas element.
 */

import { nodeBounds, rectsIntersect } from "./geometry";
import type { FigureNode, Rect, Vec } from "./scene";

export interface Viewport {
  /** Scene x at the canvas's left edge. */
  readonly x: number;
  /** Scene y at the canvas's top edge. */
  readonly y: number;
  /** Screen pixels per scene unit. */
  readonly zoom: number;
}

export const MIN_ZOOM = 0.1;
export const MAX_ZOOM = 8;

/** Steps Ctrl+= and Ctrl+- walk through. */
export const ZOOM_STEPS: readonly number[] = [
  0.1, 0.25, 0.33, 0.5, 0.67, 0.75, 0.9, 1, 1.1, 1.25, 1.5, 2, 3, 4, 6, 8,
];

export function clampZoom(zoom: number): number {
  return Math.min(Math.max(zoom, MIN_ZOOM), MAX_ZOOM);
}

export function toScreen(v: Viewport, p: Vec): Vec {
  return { x: (p.x - v.x) * v.zoom, y: (p.y - v.y) * v.zoom };
}

export function toScene(v: Viewport, s: Vec): Vec {
  return { x: s.x / v.zoom + v.x, y: s.y / v.zoom + v.y };
}

/** The SVG `viewBox` for a canvas of `width` by `height` screen pixels. */
export function viewBox(v: Viewport, width: number, height: number): string {
  return `${v.x} ${v.y} ${Math.max(width, 1) / v.zoom} ${Math.max(height, 1) / v.zoom}`;
}

/** Moves the view by a screen-pixel delta (content follows the pointer). */
export function pan(v: Viewport, dx: number, dy: number): Viewport {
  return { ...v, x: v.x - dx / v.zoom, y: v.y - dy / v.zoom };
}

/** Zooms to `zoom`, keeping the scene point under `screen` where it is. */
export function zoomAt(v: Viewport, zoom: number, screen: Vec): Viewport {
  const next = clampZoom(zoom);
  const anchor = toScene(v, screen);
  return { x: anchor.x - screen.x / next, y: anchor.y - screen.y / next, zoom: next };
}

/** The next zoom step in or out from `zoom`. */
export function stepZoom(zoom: number, direction: 1 | -1): number {
  if (direction > 0) return ZOOM_STEPS.find((z) => z > zoom + 1e-9) ?? MAX_ZOOM;
  return [...ZOOM_STEPS].reverse().find((z) => z < zoom - 1e-9) ?? MIN_ZOOM;
}

/** A wheel delta (pixels) as a zoom factor; trackpad pinches send small deltas. */
export function wheelZoomFactor(deltaY: number): number {
  return Math.exp(-Math.max(Math.min(deltaY, 100), -100) * 0.0025 * 4);
}

/**
 * The view that shows `rect` whole and centred in a canvas of the given
 * size, with `padding` screen pixels around it, never zoomed in past `max`.
 */
export function fitRect(
  rect: Rect,
  width: number,
  height: number,
  padding = 48,
  max = 2,
): Viewport {
  const w = Math.max(rect.width, 1);
  const h = Math.max(rect.height, 1);
  const zoom = clampZoom(
    Math.min(
      Math.max(width - 2 * padding, 1) / w,
      Math.max(height - 2 * padding, 1) / h,
      max,
    ),
  );
  const cx = rect.x + rect.width / 2;
  const cy = rect.y + rect.height / 2;
  return { x: cx - width / 2 / zoom, y: cy - height / 2 / zoom, zoom };
}

/** The scene rectangle the canvas shows, grown by `margin` screen pixels. */
export function visibleRect(
  v: Viewport,
  width: number,
  height: number,
  margin = 0,
): Rect {
  const m = margin / v.zoom;
  return {
    x: v.x - m,
    y: v.y - m,
    width: width / v.zoom + 2 * m,
    height: height / v.zoom + 2 * m,
  };
}

/**
 * Bounds memoised per node object: nodes are immutable, so a node that did
 * not change keeps its bounds, and culling a moving scene only measures the
 * nodes that moved.
 */
const BOUNDS = new WeakMap<FigureNode, Rect>();

export function cachedBounds(node: FigureNode): Rect {
  let b = BOUNDS.get(node);
  if (b === undefined) {
    b = nodeBounds(node);
    BOUNDS.set(node, b);
  }
  return b;
}

/** The nodes whose bounds meet `area`, in paint order. */
export function cull(nodes: readonly FigureNode[], area: Rect): FigureNode[] {
  return nodes.filter((n) => rectsIntersect(cachedBounds(n), area));
}
