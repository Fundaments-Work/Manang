import { describe, expect, it } from "vitest";

import { renderSceneSvg } from "$lib/kernel/mockFigure";

import { svgPathFor, translatedScene } from "./edit";
import { createScene, type FigureScene } from "./scene";

function sceneWith(): FigureScene {
  return {
    ...createScene(200, 100),
    nodes: [
      {
        kind: "rect",
        id: "n1",
        x: 10,
        y: 10,
        width: 20,
        height: 10,
        stroke: "#e8e6e1",
      },
      { kind: "ellipse", id: "n2", cx: 50, cy: 50, rx: 5, ry: 4, stroke: "#e8e6e1" },
      {
        kind: "arrow",
        id: "n3",
        from: { x: 0, y: 0 },
        to: { x: 10, y: 0 },
        color: "#e8e6e1",
      },
      { kind: "math", id: "n4", x: 5, y: 5, tex: "x", size: 20, color: "#b3d17a" },
      {
        kind: "stroke",
        id: "n5",
        points: [{ x: 1, y: 1, pressure: 0.5 }],
        size: 2,
        color: "#e8e6e1",
      },
    ],
  };
}

describe("translatedScene", () => {
  it("moves every node kind by the delta", () => {
    const scene = sceneWith();
    const d = { x: 3, y: -2 };
    const moved = ["n1", "n2", "n3", "n4", "n5"].reduce(
      (s, id) => translatedScene(s, id, d),
      scene,
    );
    const [rect, ellipse, arrow, math, stroke] = moved.nodes;
    expect(rect).toMatchObject({ x: 13, y: 8 });
    expect(ellipse).toMatchObject({ cx: 53, cy: 48 });
    expect(arrow).toMatchObject({ from: { x: 3, y: -2 }, to: { x: 13, y: -2 } });
    expect(math).toMatchObject({ x: 8, y: 3 });
    expect(stroke).toMatchObject({ points: [{ x: 4, y: -1, pressure: 0.5 }] });
  });

  it("returns the same scene for a zero move or an unknown node", () => {
    const scene = sceneWith();
    expect(translatedScene(scene, "n1", { x: 0, y: 0 })).toBe(scene);
    expect(translatedScene(scene, "missing", { x: 5, y: 5 })).toBe(scene);
  });

  it("never mutates the input scene (undo relies on it)", () => {
    const scene = sceneWith();
    const before = JSON.stringify(scene);
    translatedScene(scene, "n1", { x: 9, y: 9 });
    expect(JSON.stringify(scene)).toBe(before);
  });
});

describe("svgPathFor", () => {
  it("puts the SVG next to the figure", () => {
    expect(svgPathFor("figures/lattice.figure.json")).toBe("figures/lattice.svg");
  });

  it("leaves other paths alone", () => {
    expect(svgPathFor("notes.json")).toBe("notes.json");
  });
});

describe("browser SVG stand-in", () => {
  it("renders every node and escapes formula text", () => {
    const scene = sceneWith();
    const svg = renderSceneSvg({
      ...scene,
      nodes: [
        ...scene.nodes,
        {
          kind: "math",
          id: "n6",
          x: 0,
          y: 0,
          tex: "a<b&c",
          size: 10,
          color: "#b3d17a",
        },
      ],
    });
    expect(svg.startsWith("<svg")).toBe(true);
    for (const tag of ["<rect", "<ellipse", "<line", "<text", "<path"]) {
      expect(svg).toContain(tag);
    }
    expect(svg).toContain("a&lt;b&amp;c");
  });
});
