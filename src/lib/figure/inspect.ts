/**
 * What the inspector and the layers panel need to know about the scene,
 * as pure functions: the Layout box of a selection and the operation that
 * edits one of its fields, the names layers show, and the reorder steps
 * for a layer dragged to a new place.
 */

import { nodeRect } from "./geometry";
import { keepAspect } from "./fields";
import type { FigureEditOp } from "./ops";
import { scaleSelection } from "./scale";
import { selectionBounds } from "./selection";
import { isShape, type FigureNode, type FigureScene, type Rect } from "./scene";

export type LayoutField = "x" | "y" | "width" | "height";

/**
 * The box the X, Y, W and H fields describe: the node's own box for one
 * node (a rotated shape keeps its unrotated box), the union for several.
 */
export function layoutBox(scene: FigureScene, ids: readonly string[]): Rect {
  const [only] = ids;
  if (ids.length === 1 && only !== undefined) {
    const node = scene.nodes.find((n) => n.id === only);
    if (node !== undefined) return nodeRect(node);
  }
  return selectionBounds(scene, ids);
}

/** A layout box side is never smaller than this. */
export const MIN_SIDE = 1;

/**
 * The operation that sets one Layout field of the selection to `value`, or
 * undefined when nothing would change. Position moves the selection; size
 * scales it from its top left (every node by the same factor), keeping the
 * proportions when `lockAspect` is on. Locked nodes are left where they are.
 */
export function layoutEdit(
  scene: FigureScene,
  ids: readonly string[],
  field: LayoutField,
  value: number,
  lockAspect: boolean,
): FigureEditOp | undefined {
  const free = ids.filter(
    (id) => scene.nodes.find((n) => n.id === id)?.locked !== true,
  );
  if (free.length === 0 || !Number.isFinite(value)) return undefined;
  const box = layoutBox(scene, free);
  if (field === "x" || field === "y") {
    const delta = field === "x" ? value - box.x : value - box.y;
    if (delta === 0) return undefined;
    return {
      op: "move",
      ids: free,
      dx: field === "x" ? delta : 0,
      dy: field === "y" ? delta : 0,
    };
  }
  const side = Math.max(value, MIN_SIDE);
  let { width, height } = box;
  if (lockAspect) {
    ({ width, height } = keepAspect(field, side, box.width, box.height));
  } else if (field === "width") {
    width = side;
  } else {
    height = side;
  }
  const next = {
    x: box.x,
    y: box.y,
    width: Math.max(width, MIN_SIDE),
    height: Math.max(height, MIN_SIDE),
  };
  return scaleSelection(scene, free, box, next);
}

/* -------------------------------------------------------------------------- */
/* Layers                                                                      */
/* -------------------------------------------------------------------------- */

const KIND_NAMES: Readonly<Record<string, string>> = {
  rect: "Rectangle",
  diamond: "Diamond",
  ellipse: "Ellipse",
  text: "Text",
  frame: "Frame",
  image: "Image",
  mermaid: "Mermaid",
  arrow: "Arrow",
  line: "Line",
  freedraw: "Drawing",
  math: "Formula",
};

/** The type of a node in words ("Rectangle"). */
export function kindName(node: FigureNode): string {
  return KIND_NAMES[node.kind] ?? node.kind;
}

/** Longest name a layer row shows before it is cut with an ellipsis. */
export const NAME_LIMIT = 40;

/**
 * What a layer is called: its label (the first line), a formula's source,
 * or its type with its id ("Rectangle n3"). A node has no separate name in
 * the file; a label is the name, and renaming a layer edits it.
 */
export function layerName(node: FigureNode): string {
  const text =
    node.kind === "math"
      ? node.source
      : node.kind === "freedraw"
        ? undefined
        : node.label;
  const line = text
    ?.split("\n")
    .find((l) => l.trim() !== "")
    ?.trim();
  if (line !== undefined && line !== "") {
    return line.length > NAME_LIMIT ? `${line.slice(0, NAME_LIMIT - 1)}…` : line;
  }
  return `${kindName(node)} ${node.id}`;
}

/** Whether renaming the layer is possible (it edits the label). */
export function isRenamable(node: FigureNode): boolean {
  return node.kind !== "math" && node.kind !== "freedraw";
}

/** Layers in the order a panel lists them: topmost first. */
export function layerOrder(scene: FigureScene): readonly FigureNode[] {
  return [...scene.nodes].reverse();
}

/** Rows whose name or type contains `query` (case-insensitive); all when empty. */
export function filterLayers(
  nodes: readonly FigureNode[],
  query: string,
): readonly FigureNode[] {
  const q = query.trim().toLowerCase();
  if (q === "") return nodes;
  return nodes.filter(
    (n) =>
      layerName(n).toLowerCase().includes(q) || kindName(n).toLowerCase().includes(q),
  );
}

export type DropSide = "above" | "below";

/**
 * The `reorder` steps (one `forward` or `backward` each) that bring layer
 * `id` next to `target`: "above" means drawn in front of it. Empty when it
 * is already there. `paint` is the scene's node ids, bottom first.
 */
export function moveSteps(
  paint: readonly string[],
  id: string,
  target: string,
  side: DropSide,
): readonly ("forward" | "backward")[] {
  const from = paint.indexOf(id);
  const rest = paint.filter((p) => p !== id);
  const at = rest.indexOf(target);
  if (from === -1 || at === -1 || id === target) return [];
  const to = side === "above" ? at + 1 : at;
  const delta = to - from;
  return Array.from({ length: Math.abs(delta) }, () =>
    delta > 0 ? ("forward" as const) : ("backward" as const),
  );
}

/** The batch that performs `moveSteps`, or undefined when there is nothing to do. */
export function moveLayerOp(
  scene: FigureScene,
  id: string,
  target: string,
  side: DropSide,
): FigureEditOp | undefined {
  const steps = moveSteps(
    scene.nodes.map((n) => n.id),
    id,
    target,
    side,
  );
  if (steps.length === 0) return undefined;
  return {
    op: "batch",
    ops: steps.map((to) => ({ op: "reorder" as const, ids: [id], to })),
  };
}

/** Shapes carry an angle; the Rotation field is for them. */
export function canRotate(nodes: readonly FigureNode[]): boolean {
  return nodes.length > 0 && nodes.every(isShape);
}

/**
 * The ids of the layers between `from` and `to` as a panel lists them
 * (topmost first), both included: a Shift-click range. Empty when either is
 * not in the scene.
 */
export function displayRange(scene: FigureScene, from: string, to: string): string[] {
  const order = layerOrder(scene).map((n) => n.id);
  const a = order.indexOf(from);
  const b = order.indexOf(to);
  if (a === -1 || b === -1) return [];
  return order.slice(Math.min(a, b), Math.max(a, b) + 1);
}
