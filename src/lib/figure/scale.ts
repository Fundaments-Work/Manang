/**
 * Scaling a multi-selection with its bounding-box handles: every selected
 * node goes through the same affine map (position and size), as one batch,
 * so it is one undo step. Rectangles, ellipses, text boxes and images scale
 * their box; freehand strokes and arrows scale their points (bindings stay,
 * and Rust re-routes bound ends); formulas scale their font size.
 */

import { nodeRect } from "./geometry";
import type { FigureEditOp } from "./ops";
import type { FigureNode, FigureScene, Rect } from "./scene";

/** `r` mapped from the frame `from` onto `to`. */
export function mapRect(r: Rect, from: Rect, to: Rect): Rect {
  const sx = from.width > 0 ? to.width / from.width : 1;
  const sy = from.height > 0 ? to.height / from.height : 1;
  return {
    x: to.x + (r.x - from.x) * sx,
    y: to.y + (r.y - from.y) * sy,
    width: r.width * sx,
    height: r.height * sy,
  };
}

/** A box a node can still have: at least one unit each way. */
function atLeastOne(r: Rect): Rect {
  return { ...r, width: Math.max(r.width, 1), height: Math.max(r.height, 1) };
}

/**
 * The `resize` operations that scale `nodes` from the selection box `from`
 * onto `to`. Empty when the boxes are the same.
 */
export function scaleOps(
  nodes: readonly FigureNode[],
  from: Rect,
  to: Rect,
): FigureEditOp[] {
  if (
    from.x === to.x &&
    from.y === to.y &&
    from.width === to.width &&
    from.height === to.height
  ) {
    return [];
  }
  return nodes
    .filter((n) => n.locked !== true)
    .map((n) => {
      const r = atLeastOne(mapRect(nodeRect(n), from, to));
      return {
        op: "resize" as const,
        id: n.id,
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
      };
    });
}

/** The one operation (a batch) that scales the nodes `ids`; undefined when nothing changes. */
export function scaleSelection(
  scene: FigureScene,
  ids: readonly string[],
  from: Rect,
  to: Rect,
): FigureEditOp | undefined {
  const wanted = new Set(ids);
  const ops = scaleOps(
    scene.nodes.filter((n) => wanted.has(n.id)),
    from,
    to,
  );
  return ops.length === 0 ? undefined : { op: "batch", ops };
}
