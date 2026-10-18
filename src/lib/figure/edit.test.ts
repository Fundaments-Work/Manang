import { describe, expect, it } from "vitest";

import {
  applyEdit,
  embedLine,
  svgPathFor,
  translatedNodes,
  translatedScene,
} from "./edit";
import { createScene, type FigureScene } from "./scene";

function sceneWith(): FigureScene {
  return {
    ...createScene(800, 400),
    nodes: [
      { kind: "rect", id: "a", x: 0, y: 0, width: 100, height: 50 },
      { kind: "rect", id: "b", x: 300, y: 0, width: 100, height: 50 },
      {
        kind: "arrow",
        id: "e",
        points: [
          { x: 104, y: 25 },
          { x: 296, y: 25 },
        ],
        start: { node: "a" },
        end: { node: "b" },
      },
      { kind: "math", id: "m", x: 5, y: 5, tex: "x", size: 20 },
      { kind: "freedraw", id: "f", points: [{ x: 1, y: 1, pressure: 0.5 }], size: 2 },
    ],
  };
}

describe("translatedNodes", () => {
  it("moves every kind and drags bound arrows along (preview only)", () => {
    const scene = sceneWith();
    const moved = translatedNodes(scene, ["b", "m", "f"], { x: 0, y: 100 });
    expect(moved.nodes[1]).toMatchObject({ x: 300, y: 100 });
    expect(moved.nodes[3]).toMatchObject({ x: 5, y: 105 });
    expect(moved.nodes[4]).toMatchObject({ points: [{ x: 1, y: 101, pressure: 0.5 }] });
    const arrow = moved.nodes[2];
    expect(arrow?.kind === "arrow" && (arrow.points[1]?.y ?? 0) > 100).toBe(true);
  });

  it("returns the same scene for a zero move or an unknown node, never mutating", () => {
    const scene = sceneWith();
    const before = JSON.stringify(scene);
    expect(translatedScene(scene, "a", { x: 0, y: 0 })).toBe(scene);
    expect(translatedScene(scene, "missing", { x: 5, y: 5 })).toBe(scene);
    translatedScene(scene, "a", { x: 9, y: 9 });
    expect(JSON.stringify(scene)).toBe(before);
  });
});

describe("applyEdit", () => {
  it("applies plugin operations without touching the input", () => {
    const scene = sceneWith();
    const moved = applyEdit(scene, { op: "translate", id: "a", dx: 3, dy: 4 });
    expect(moved.nodes[0]).toMatchObject({ x: 3, y: 4, width: 100 });
    expect(scene.nodes[0]).toMatchObject({ x: 0, y: 0 });
    expect(applyEdit(scene, { op: "clear" }).nodes).toEqual([]);
  });

  it("refuses what the plugin refuses", () => {
    const scene = sceneWith();
    expect(() =>
      applyEdit(scene, {
        op: "add-node",
        node: { kind: "rect", id: "a", x: 0, y: 0, width: 1, height: 1 },
      }),
    ).toThrow(/already exists/);
    expect(() => applyEdit(scene, { op: "remove-node", id: "zz" })).toThrow(
      /not found/,
    );
    expect(() =>
      applyEdit(scene, { op: "translate", id: "a", dx: Number.NaN, dy: 1 }),
    ).toThrow(/finite/);
  });
});

describe("paths and embeds", () => {
  it("puts the SVG next to the figure", () => {
    expect(svgPathFor("figures/lattice.figure.json")).toBe("figures/lattice.svg");
    expect(svgPathFor("notes.json")).toBe("notes.json");
  });

  it("names the SVG in each engine's syntax", () => {
    expect(embedLine("figures/a.svg", "typst")).toBe(
      '#figure(image("figures/a.svg", width: 80%))',
    );
    expect(embedLine("figures/a.svg", "latex")).toBe(
      "\\includegraphics[width=0.8\\linewidth]{figures/a.pdf}",
    );
    expect(embedLine("figures/a.svg", "markdown")).toBe("![](figures/a.svg)");
  });
});
