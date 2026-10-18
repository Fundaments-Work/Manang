/**
 * Snapping and smart guides: while a selection moves or a shape resizes,
 * its edges and centre lines snap to other nodes' edges and centre lines
 * within a threshold, and the lines they snapped to are drawn as guides.
 * Pure geometry in scene units; the caller converts the screen threshold.
 */

import type { Rect } from "./scene";

/** Screen pixels within which an edge snaps. */
export const SNAP_PX = 6;

/** A guide line: vertical (`x` set) or horizontal (`y` set), with its extent. */
export type Guide =
  | {
      readonly axis: "x";
      readonly x: number;
      readonly from: number;
      readonly to: number;
    }
  | {
      readonly axis: "y";
      readonly y: number;
      readonly from: number;
      readonly to: number;
    };

export interface SnapResult {
  readonly dx: number;
  readonly dy: number;
  readonly guides: readonly Guide[];
}

function xs(r: Rect): number[] {
  return [r.x, r.x + r.width / 2, r.x + r.width];
}

function ys(r: Rect): number[] {
  return [r.y, r.y + r.height / 2, r.y + r.height];
}

/** The smallest correction (within `threshold`) bringing one of `mine` onto one of `theirs`. */
export function bestOffset(
  mine: readonly number[],
  theirs: readonly number[],
  threshold: number,
): number | undefined {
  let best: number | undefined;
  for (const a of mine) {
    for (const b of theirs) {
      const d = b - a;
      if (
        Math.abs(d) <= threshold &&
        (best === undefined || Math.abs(d) < Math.abs(best))
      ) {
        best = d;
      }
    }
  }
  return best;
}

function guidesFor(moved: Rect, others: readonly Rect[]): Guide[] {
  const guides: Guide[] = [];
  const eps = 1e-6;
  for (const x of xs(moved)) {
    const hits = others.filter((o) => xs(o).some((ox) => Math.abs(ox - x) < eps));
    if (hits.length === 0) continue;
    const all = [moved, ...hits];
    guides.push({
      axis: "x",
      x,
      from: Math.min(...all.map((r) => r.y)),
      to: Math.max(...all.map((r) => r.y + r.height)),
    });
  }
  for (const y of ys(moved)) {
    const hits = others.filter((o) => ys(o).some((oy) => Math.abs(oy - y) < eps));
    if (hits.length === 0) continue;
    const all = [moved, ...hits];
    guides.push({
      axis: "y",
      y,
      from: Math.min(...all.map((r) => r.x)),
      to: Math.max(...all.map((r) => r.x + r.width)),
    });
  }
  return guides;
}

/**
 * Snaps a moving box (already moved by the raw pointer delta) to `others`.
 * Returns the extra correction and the guides of the final position.
 */
export function snapMove(
  moving: Rect,
  others: readonly Rect[],
  threshold: number,
): SnapResult {
  const theirsX = others.flatMap(xs);
  const theirsY = others.flatMap(ys);
  const dx = bestOffset(xs(moving), theirsX, threshold) ?? 0;
  const dy = bestOffset(ys(moving), theirsY, threshold) ?? 0;
  const moved = { ...moving, x: moving.x + dx, y: moving.y + dy };
  return { dx, dy, guides: guidesFor(moved, others) };
}

/**
 * Snaps the edges a resize moves. `edges` says which sides follow the
 * pointer; the others stay put. Returns the corrected box and its guides.
 */
export function snapResize(
  box: Rect,
  edges: {
    readonly left: boolean;
    readonly right: boolean;
    readonly top: boolean;
    readonly bottom: boolean;
  },
  others: readonly Rect[],
  threshold: number,
): { readonly box: Rect; readonly guides: readonly Guide[] } {
  const theirsX = others.flatMap(xs);
  const theirsY = others.flatMap(ys);
  let { x, y, width, height } = box;
  if (edges.left) {
    const d = bestOffset([x], theirsX, threshold) ?? 0;
    x += d;
    width -= d;
  } else if (edges.right) {
    width += bestOffset([x + width], theirsX, threshold) ?? 0;
  }
  if (edges.top) {
    const d = bestOffset([y], theirsY, threshold) ?? 0;
    y += d;
    height -= d;
  } else if (edges.bottom) {
    height += bestOffset([y + height], theirsY, threshold) ?? 0;
  }
  const out = { x, y, width, height };
  return { box: out, guides: guidesFor(out, others) };
}
