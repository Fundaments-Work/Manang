import { describe, expect, it } from "vitest";

import { renderSceneSvg } from "$lib/kernel/mockFigure";

import { applyEdit, embedLine, svgPathFor, translatedScene } from "./edit";
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

describe("applyEdit", () => {
  it("applies each plugin operation without touching the input", () => {
    const scene = sceneWith();
    const added = applyEdit(scene, {
      op: "add-node",
      node: {
        kind: "math",
        id: "n9",
        x: 1,
        y: 2,
        tex: "y",
        size: 10,
        color: "#b3d17a",
      },
    });
    expect(added.nodes.length).toBe(scene.nodes.length + 1);
    expect(scene.nodes.some((n) => n.id === "n9")).toBe(false);

    const moved = applyEdit(scene, { op: "translate", id: "n1", dx: 3, dy: 4 });
    expect(moved.nodes[0]).toMatchObject({ x: 13, y: 14, width: 20 });

    const renamed = applyEdit(scene, {
      op: "update-node",
      node: {
        kind: "math",
        id: "n4",
        x: 5,
        y: 5,
        tex: "E = mc^2",
        size: 20,
        color: "#b3d17a",
      },
    });
    expect(renamed.nodes.find((n) => n.id === "n4")).toMatchObject({ tex: "E = mc^2" });

    const removed = applyEdit(scene, { op: "remove-node", id: "n2" });
    expect(removed.nodes.map((n) => n.id)).not.toContain("n2");
    expect(applyEdit(scene, { op: "clear" }).nodes).toEqual([]);
  });

  it("refuses what the plugin refuses", () => {
    const scene = sceneWith();
    const first = scene.nodes[0];
    if (first === undefined) throw new Error("scene has nodes");
    expect(() => applyEdit(scene, { op: "add-node", node: first })).toThrow(
      /already exists/,
    );
    expect(() => applyEdit(scene, { op: "remove-node", id: "zz" })).toThrow(
      /not found/,
    );
    expect(() => applyEdit(scene, { op: "translate", id: "zz", dx: 1, dy: 1 })).toThrow(
      /not found/,
    );
    expect(() =>
      applyEdit(scene, { op: "translate", id: "n1", dx: Number.NaN, dy: 1 }),
    ).toThrow(/finite/);
  });
});

describe("embedLine", () => {
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
