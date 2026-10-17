/**
 * Pure figure edits, kept out of the reactive figure state so they can be unit
 * tested and reused by other tools (nudging, dragging, undo).
 */

import { nodeById, replaceNode, type FigureScene, type Vec } from "./scene";

/** Where the rendered SVG of a figure lives: next to its `.figure.json`. */
export function svgPathFor(figurePath: string): string {
  return figurePath.replace(/\.figure\.json$/, ".svg");
}

/** Most undo steps kept per figure. */
export const HISTORY_LIMIT = 200;

/** The scene with one node moved; the same scene if the node does not exist. */
export function translatedScene(
  scene: FigureScene,
  id: string,
  delta: Vec,
): FigureScene {
  const node = nodeById(scene, id);
  if (node === undefined || (delta.x === 0 && delta.y === 0)) return scene;

  switch (node.kind) {
    case "rect":
      return replaceNode(scene, {
        ...node,
        x: node.x + delta.x,
        y: node.y + delta.y,
      });
    case "ellipse":
      return replaceNode(scene, {
        ...node,
        cx: node.cx + delta.x,
        cy: node.cy + delta.y,
      });
    case "arrow":
      return replaceNode(scene, {
        ...node,
        from: { x: node.from.x + delta.x, y: node.from.y + delta.y },
        to: { x: node.to.x + delta.x, y: node.to.y + delta.y },
      });
    case "math":
      return replaceNode(scene, {
        ...node,
        x: node.x + delta.x,
        y: node.y + delta.y,
      });
    case "stroke":
      return replaceNode(scene, {
        ...node,
        points: node.points.map((point) => ({
          ...point,
          x: point.x + delta.x,
          y: point.y + delta.y,
        })),
      });
  }
}
