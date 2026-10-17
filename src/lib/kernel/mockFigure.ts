/**
 * Browser stand-in for the figure plugin's `figure.render-svg`: a plain SVG
 * of the scene, so saving a figure in `pnpm dev` writes an SVG like the
 * desktop app does. The desktop app renders through the Rust figure crate.
 */

import { arrowHead, polygonToPath, strokeToPath } from "$lib/figure/geometry";
import type { FigureNode, FigureScene } from "$lib/figure/scene";

import type { MockPlugin, MockPluginFactory } from "./mock";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["figure.render-svg", "Render a scene to SVG"],
];

function escapeXml(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function nodeSvg(node: FigureNode): string {
  switch (node.kind) {
    case "stroke":
      return `<path d="${strokeToPath(node)}" fill="${node.color}"/>`;
    case "rect":
      return `<rect x="${node.x}" y="${node.y}" width="${node.width}" height="${node.height}" fill="${node.fill ?? "none"}" stroke="${node.stroke}"/>`;
    case "ellipse":
      return `<ellipse cx="${node.cx}" cy="${node.cy}" rx="${node.rx}" ry="${node.ry}" fill="${node.fill ?? "none"}" stroke="${node.stroke}"/>`;
    case "arrow":
      return (
        `<line x1="${node.from.x}" y1="${node.from.y}" x2="${node.to.x}" y2="${node.to.y}" stroke="${node.color}"/>` +
        `<path d="${polygonToPath(arrowHead(node.from, node.to))}" fill="${node.color}"/>`
      );
    case "math":
      return `<text x="${node.x}" y="${node.y}" font-size="${node.size}" fill="${node.color}">${escapeXml(node.tex)}</text>`;
  }
}

export function renderSceneSvg(scene: FigureScene): string {
  const body = scene.nodes.map(nodeSvg).join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${scene.width} ${scene.height}" width="${scene.width}" height="${scene.height}">${body}</svg>`;
}

export function figurePlugin(): MockPluginFactory {
  return (): MockPlugin => ({
    name: "figure",
    commands: COMMANDS,
    handlers: {},
    binaryHandlers: {
      "figure.render-svg": (args) => {
        const scene = args["scene"] as FigureScene | undefined;
        if (scene === undefined || !Array.isArray(scene.nodes)) {
          throw new Error("invalid `scene`");
        }
        return new TextEncoder().encode(renderSceneSvg(scene));
      },
    },
  });
}
