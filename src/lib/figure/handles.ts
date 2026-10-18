/**
 * Selection handles: where the eight resize handles and the rotation handle
 * of a (possibly rotated) box sit, which one the pointer is on, and the box
 * a resize or rotation drag produces.
 */

import { rotatePoint } from "./geometry";
import type { Rect, Vec } from "./scene";

export type ResizeHandle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";
export type Handle = ResizeHandle | "rotate";

export const RESIZE_HANDLES: readonly ResizeHandle[] = [
  "nw",
  "n",
  "ne",
  "e",
  "se",
  "s",
  "sw",
  "w",
];

/** Screen distance of the rotation handle above the box. */
export const ROTATE_OFFSET_PX = 22;

/** Fractions of the box for each resize handle. */
const FRACTIONS: Record<ResizeHandle, Vec> = {
  nw: { x: 0, y: 0 },
  n: { x: 0.5, y: 0 },
  ne: { x: 1, y: 0 },
  e: { x: 1, y: 0.5 },
  se: { x: 1, y: 1 },
  s: { x: 0.5, y: 1 },
  sw: { x: 0, y: 1 },
  w: { x: 0, y: 0.5 },
};

const OPPOSITE: Record<ResizeHandle, ResizeHandle> = {
  nw: "se",
  n: "s",
  ne: "sw",
  e: "w",
  se: "nw",
  s: "n",
  sw: "ne",
  w: "e",
};

function center(r: Rect): Vec {
  return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
}

function local(r: Rect, f: Vec): Vec {
  return { x: r.x + f.x * r.width, y: r.y + f.y * r.height };
}

/** Scene positions of every handle of `r` rotated by `angle` degrees. */
export function handlePositions(
  r: Rect,
  angle: number,
  zoom: number,
): Record<Handle, Vec> {
  const c = center(r);
  const out = {} as Record<Handle, Vec>;
  for (const h of RESIZE_HANDLES)
    out[h] = rotatePoint(local(r, FRACTIONS[h]), c, angle);
  out.rotate = rotatePoint({ x: c.x, y: r.y - ROTATE_OFFSET_PX / zoom }, c, angle);
  return out;
}

/** The handle within `tolerance` scene units of `p`, if any (rotation first). */
export function handleAt(
  positions: Record<Handle, Vec>,
  p: Vec,
  tolerance: number,
): Handle | undefined {
  for (const h of ["rotate", ...RESIZE_HANDLES] as Handle[]) {
    const q = positions[h];
    if (Math.abs(q.x - p.x) <= tolerance && Math.abs(q.y - p.y) <= tolerance) return h;
  }
  return undefined;
}

/** Which sides a handle moves. */
export function handleEdges(h: ResizeHandle): {
  left: boolean;
  right: boolean;
  top: boolean;
  bottom: boolean;
} {
  return {
    left: h.includes("w"),
    right: h.includes("e"),
    top: h.startsWith("n"),
    bottom: h.startsWith("s"),
  };
}

/** The CSS cursor for a handle on an unrotated box. */
export function handleCursor(h: Handle): string {
  switch (h) {
    case "rotate":
      return "grab";
    case "n":
    case "s":
      return "ns-resize";
    case "e":
    case "w":
      return "ew-resize";
    case "nw":
    case "se":
      return "nwse-resize";
    case "ne":
    case "sw":
      return "nesw-resize";
  }
}

/**
 * The box after dragging handle `h` of `r` (rotated by `angle`) to the scene
 * point `pointer`. The opposite handle stays fixed on screen; with
 * `keepAspect` a corner keeps the width/height ratio. Never smaller than 1.
 */
export function resizeBox(
  r: Rect,
  angle: number,
  h: ResizeHandle,
  pointer: Vec,
  keepAspect: boolean,
): Rect {
  const c = center(r);
  const p = rotatePoint(pointer, c, -angle);
  const anchor = local(r, FRACTIONS[OPPOSITE[h]]);
  const edges = handleEdges(h);
  let x0 = r.x;
  let x1 = r.x + r.width;
  let y0 = r.y;
  let y1 = r.y + r.height;
  if (edges.left) x0 = p.x;
  if (edges.right) x1 = p.x;
  if (edges.top) y0 = p.y;
  if (edges.bottom) y1 = p.y;
  let w = Math.max(Math.abs(x1 - x0), 1);
  let hgt = Math.max(Math.abs(y1 - y0), 1);
  const corner = (edges.left || edges.right) && (edges.top || edges.bottom);
  if (keepAspect && corner && r.width > 0 && r.height > 0) {
    const scale = Math.max(w / r.width, hgt / r.height);
    w = r.width * scale;
    hgt = r.height * scale;
  }
  // The new box in the old box's unrotated frame, pinned at the anchor and
  // growing toward the pointer.
  const towardX = edges.left
    ? p.x <= anchor.x
      ? -1
      : 1
    : edges.right
      ? p.x >= anchor.x
        ? 1
        : -1
      : 0;
  const towardY = edges.top
    ? p.y <= anchor.y
      ? -1
      : 1
    : edges.bottom
      ? p.y >= anchor.y
        ? 1
        : -1
      : 0;
  let lx: number;
  let lw: number;
  if (towardX === 0) {
    lx = r.x;
    lw = r.width;
  } else {
    lw = w;
    lx = towardX > 0 ? anchor.x : anchor.x - w;
  }
  let ly: number;
  let lh: number;
  if (towardY === 0) {
    ly = r.y;
    lh = r.height;
  } else {
    lh = hgt;
    ly = towardY > 0 ? anchor.y : anchor.y - hgt;
  }
  // Keep the anchor where it is on screen: place the new centre by rotating
  // it about the old centre, then rebuild the box around it.
  const nc = rotatePoint({ x: lx + lw / 2, y: ly + lh / 2 }, c, angle);
  return { x: nc.x - lw / 2, y: nc.y - lh / 2, width: lw, height: lh };
}

/**
 * The angle (degrees, 0..360) that points the rotation handle at `pointer`;
 * with `snap`, in 15 degree steps.
 */
export function rotationFor(c: Vec, pointer: Vec, snap: boolean): number {
  let deg = (Math.atan2(pointer.y - c.y, pointer.x - c.x) * 180) / Math.PI + 90;
  if (snap) deg = Math.round(deg / 15) * 15;
  return ((deg % 360) + 360) % 360;
}
