/**
 * Selection rules: what a click or a marquee selects (whole outermost
 * groups, never locked nodes), the selection's bounds, and the cheap live
 * preview of a drag (moved nodes plus the arrows bound to them, re-routed).
 */

import { route } from "./binding";
import {
  HIT_TOLERANCE,
  hits,
  nodeBounds,
  rectContainsRect,
  unionRect,
  EMPTY_RECT,
} from "./geometry";
import {
  isConnector,
  type FigureNode,
  type FigureScene,
  type Rect,
  type Vec,
} from "./scene";

/** The outermost group of a node, if it has one. */
export function outermostGroup(node: FigureNode): string | undefined {
  const groups = node.groups ?? [];
  return groups[groups.length - 1];
}

/** `ids` grown to whole outermost groups, in paint order, without duplicates. */
export function expandGroups(scene: FigureScene, ids: readonly string[]): string[] {
  const wanted = new Set(ids);
  const groups = new Set<string>();
  for (const node of scene.nodes) {
    if (!wanted.has(node.id)) continue;
    const g = outermostGroup(node);
    if (g !== undefined) groups.add(g);
  }
  return scene.nodes
    .filter(
      (n) =>
        wanted.has(n.id) ||
        (outermostGroup(n) !== undefined && groups.has(outermostGroup(n) as string)),
    )
    .map((n) => n.id);
}

/**
 * The topmost selectable node at `p` (locked nodes are skipped; frames lose
 * to what they contain). `tolerance` is in scene units.
 */
export function pickAt(
  scene: FigureScene,
  p: Vec,
  tolerance: number = HIT_TOLERANCE,
): FigureNode | undefined {
  let frame: FigureNode | undefined;
  for (let i = scene.nodes.length - 1; i >= 0; i -= 1) {
    const node = scene.nodes[i];
    if (
      node === undefined ||
      node.locked === true ||
      node.hidden === true ||
      !hits(node, p, tolerance)
    )
      continue;
    if (node.kind === "frame") {
      frame ??= node;
      continue;
    }
    return node;
  }
  return frame;
}

/** Ids a marquee selects: unlocked nodes wholly inside it, grown to groups. */
export function marqueeSelect(scene: FigureScene, area: Rect): string[] {
  const inside = scene.nodes
    .filter(
      (n) =>
        n.locked !== true && n.hidden !== true && rectContainsRect(area, nodeBounds(n)),
    )
    .map((n) => n.id);
  return expandGroups(scene, inside);
}

/** Shift-click: add the clicked ids, or take them out when all are selected. */
export function toggleSelection(
  current: readonly string[],
  ids: readonly string[],
): string[] {
  const set = new Set(current);
  if (ids.every((id) => set.has(id))) return current.filter((id) => !ids.includes(id));
  return [...current, ...ids.filter((id) => !set.has(id))];
}

/** Union of the selected nodes' bounds. */
export function selectionBounds(scene: FigureScene, ids: readonly string[]): Rect {
  const set = new Set(ids);
  return scene.nodes
    .filter((n) => set.has(n.id))
    .reduce<Rect>((acc, n) => unionRect(acc, nodeBounds(n)), EMPTY_RECT);
}

/** Every unlocked, visible node id (Ctrl+A). */
export function selectAll(scene: FigureScene): string[] {
  return scene.nodes
    .filter((n) => n.locked !== true && n.hidden !== true)
    .map((n) => n.id);
}

function translated(node: FigureNode, d: Vec): FigureNode {
  switch (node.kind) {
    case "arrow":
    case "line":
      return {
        ...node,
        points: node.points.map((p) => ({ x: p.x + d.x, y: p.y + d.y })),
      };
    case "freedraw":
      return {
        ...node,
        points: node.points.map((p) => ({ ...p, x: p.x + d.x, y: p.y + d.y })),
      };
    default:
      return { ...node, x: node.x + d.x, y: node.y + d.y };
  }
}

/** What a drag of `ids` touches: their indices and the connectors bound to them. */
export interface DragPlan {
  readonly ids: ReadonlySet<string>;
  /** Indices (in `scene.nodes`) of the moving nodes. */
  readonly moving: readonly number[];
  /** Indices of connectors bound to a moving node that do not move themselves. */
  readonly bound: readonly number[];
  /** Shapes those connectors are bound to that stay put, by id. */
  readonly anchors: ReadonlyMap<string, FigureNode>;
}

/** Plans a drag once at pointer-down: the per-move work is then O(plan). */
export function planDrag(scene: FigureScene, ids: readonly string[]): DragPlan {
  const set = new Set(ids);
  const moving: number[] = [];
  const bound: number[] = [];
  const wanted = new Set<string>();
  scene.nodes.forEach((n, i) => {
    if (set.has(n.id)) moving.push(i);
    else if (
      isConnector(n) &&
      [n.start, n.end].some((b) => b !== undefined && set.has(b.node))
    ) {
      bound.push(i);
      for (const b of [n.start, n.end])
        if (b !== undefined && !set.has(b.node)) wanted.add(b.node);
    }
  });
  const anchors = new Map<string, FigureNode>();
  if (wanted.size > 0) {
    for (const n of scene.nodes) if (wanted.has(n.id)) anchors.set(n.id, n);
  }
  return { ids: set, moving, bound, anchors };
}

/**
 * The live preview of a drag: moving nodes shifted by `delta`, bound arrows
 * re-routed against their (moved) targets. Untouched nodes keep their
 * identity so the canvas does not redraw them.
 */
export function previewMove(
  scene: FigureScene,
  plan: DragPlan,
  delta: Vec,
): FigureScene {
  if (delta.x === 0 && delta.y === 0) return scene;
  return previewReplace(
    scene,
    plan,
    plan.moving.map((i) => translated(scene.nodes[i] as FigureNode, delta)),
  );
}

/**
 * The scene with the planned nodes replaced by `next` (same order as
 * `plan.moving`) and the arrows bound to them re-routed: the preview of a
 * resize, a rotation or any other change to a few nodes.
 */
export function previewReplace(
  scene: FigureScene,
  plan: DragPlan,
  next: readonly FigureNode[],
): FigureScene {
  const nodes = [...scene.nodes];
  const byId = new Map<string, FigureNode>();
  plan.moving.forEach((i, k) => {
    const n = next[k];
    if (n === undefined) return;
    nodes[i] = n;
    byId.set(n.id, n);
  });
  for (const i of plan.bound) {
    const c = nodes[i] as FigureNode;
    if (!isConnector(c)) continue;
    const targets: FigureNode[] = [];
    for (const b of [c.start, c.end]) {
      if (b === undefined) continue;
      const t = byId.get(b.node) ?? plan.anchors.get(b.node);
      if (t !== undefined) targets.push(t);
    }
    nodes[i] = { ...c, points: route({ ...scene, nodes: targets }, c) };
  }
  return { ...scene, nodes };
}

/** A plain (non-reactive) id set, for lookups inside a gesture. */
export function idSet(ids: Iterable<string>): ReadonlySet<string> {
  return new Set(ids);
}
