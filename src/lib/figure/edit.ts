/**
 * Pure figure edits, kept out of the reactive figure state so they can be unit
 * tested and reused by other tools (nudging, dragging, undo).
 */

import {
  appendNode,
  nodeById,
  removeNode,
  replaceNode,
  type FigureNode,
  type FigureScene,
  type Vec,
} from "./scene";

/**
 * A scene edit, exactly as the figure plugin's `figure.apply` takes it. Each
 * one addresses nodes by id and carries everything it needs, so applying it
 * to the same scene gives the same result on both sides of the IPC boundary:
 * the editor applies it at once and the plugin, which owns the scene and its
 * history, applies it in order behind.
 */
export type FigureEdit =
  | { readonly op: "add-node"; readonly node: FigureNode }
  | { readonly op: "update-node"; readonly node: FigureNode }
  | { readonly op: "remove-node"; readonly id: string }
  | {
      readonly op: "translate";
      readonly id: string;
      readonly dx: number;
      readonly dy: number;
    }
  | { readonly op: "clear" };

/**
 * `edit` applied to `scene`, with the plugin's rules: a duplicate id or a
 * missing node is refused (thrown), never silently ignored.
 */
export function applyEdit(scene: FigureScene, edit: FigureEdit): FigureScene {
  switch (edit.op) {
    case "add-node":
      if (nodeById(scene, edit.node.id) !== undefined) {
        throw new Error(`node with id \`${edit.node.id}\` already exists`);
      }
      return appendNode(scene, edit.node);
    case "update-node":
      requireNode(scene, edit.node.id);
      return replaceNode(scene, edit.node);
    case "remove-node":
      requireNode(scene, edit.id);
      return removeNode(scene, edit.id);
    case "translate":
      if (!Number.isFinite(edit.dx) || !Number.isFinite(edit.dy)) {
        throw new Error("invalid argument: `dx` and `dy` must be finite numbers");
      }
      requireNode(scene, edit.id);
      return translatedScene(scene, edit.id, { x: edit.dx, y: edit.dy });
    case "clear":
      return { ...scene, nodes: [] };
  }
}

function requireNode(scene: FigureScene, id: string): void {
  if (nodeById(scene, id) === undefined) {
    throw new Error(`node with id \`${id}\` not found`);
  }
}

/** Where the rendered SVG of a figure lives: next to its `.figure.json`. */
export function svgPathFor(figurePath: string): string {
  return figurePath.replace(/\.figure\.json$/, ".svg");
}

/** The PDF Manang writes next to a figure's SVG in LaTeX projects. */
export function pdfPathFor(svgPath: string): string {
  return svgPath.replace(/\.svg$/i, ".pdf");
}

/**
 * The line that embeds a figure (its project-relative SVG path) in a document
 * of the given engine. LaTeX takes the PDF written next to the SVG on save
 * (pdfTeX and XeTeX cannot read SVG, and `\includesvg` needs Inkscape).
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
