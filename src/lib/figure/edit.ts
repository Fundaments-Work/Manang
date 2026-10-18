/**
 * Editor-side helpers around the operation model: live (preview-only)
 * moves with bound arrows following, and the paths and embed lines of a
 * figure's files. Operations themselves live in `ops.ts`.
 */

import { route } from "./binding";
import { applyOp, type FigureEditOp } from "./ops";
import { isConnector, type FigureNode, type FigureScene, type Vec } from "./scene";

export type { FigureOp, FigureEditOp } from "./ops";

/** An edit as the editor sends it (everything but undo and redo). */
export type FigureEdit = FigureEditOp;

/** `edit` applied to `scene` with the plugin's rules; throws when refused. */
export function applyEdit(scene: FigureScene, edit: FigureEdit): FigureScene {
  return applyOp(scene, edit).scene;
}

function moved(node: FigureNode, d: Vec): FigureNode {
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

/**
 * The scene with nodes `ids` moved by `delta` and every arrow bound to them
 * re-routed: the live preview of a drag. The same scene when nothing moves.
 */
export function translatedNodes(
  scene: FigureScene,
  ids: readonly string[],
  delta: Vec,
): FigureScene {
  if ((delta.x === 0 && delta.y === 0) || ids.length === 0) return scene;
  const moving = new Set(ids);
  if (!scene.nodes.some((n) => moving.has(n.id))) return scene;
  let nodes = scene.nodes.map((n) => (moving.has(n.id) ? moved(n, delta) : n));
  const next = { ...scene, nodes };
  nodes = nodes.map((n) => {
    if (!isConnector(n) || moving.has(n.id)) return n;
    const bound = [n.start, n.end].some((b) => b !== undefined && moving.has(b.node));
    return bound ? { ...n, points: route(next, n) } : n;
  });
  return { ...scene, nodes };
}

/** One node moved (see `translatedNodes`). */
export function translatedScene(
  scene: FigureScene,
  id: string,
  delta: Vec,
): FigureScene {
  return translatedNodes(scene, [id], delta);
}

/** Where the rendered SVG of a figure lives: next to its `.figure` (or legacy `.figure.json`). */
export function svgPathFor(figurePath: string): string {
  return figurePath.replace(/\.figure(\.json)?$/i, ".svg");
}

/** The PDF Manang writes next to a figure's SVG in LaTeX projects. */
export function pdfPathFor(svgPath: string): string {
  return svgPath.replace(/\.svg$/i, ".pdf");
}

/**
 * The line that embeds a figure (its project-relative SVG path) in a document
 * of the given engine. LaTeX takes the PDF written next to the SVG on save.
 */
export function embedLine(
  svgPath: string,
  engine: "typst" | "latex" | "markdown",
): string {
  switch (engine) {
    case "typst":
      return `#figure(image("${svgPath}", width: 80%))`;
    case "latex":
      return `\\includegraphics[width=0.8\\linewidth]{${pdfPathFor(svgPath)}}`;
    case "markdown":
      return `![](${svgPath})`;
  }
}
