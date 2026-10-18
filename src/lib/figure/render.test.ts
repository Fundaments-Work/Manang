import { describe, expect, it } from "vitest";

import { fillPaint, isColour, strokeColour, tokenValue, TOKEN_NAMES } from "./palette";
import {
  exportRegion,
  labelWidth,
  nodeVisual,
  pathMidpoint,
  renderSceneSvg,
} from "./render";
import { createScene, type FigureNode, type FigureScene } from "./scene";
import { measure, naturalSize, wrap } from "./text";

const SCENE: FigureScene = {
  ...createScene(400, 300),
  nodes: [
    {
      kind: "rect",
      id: "r",
      x: 10,
      y: 10,
      width: 120,
      height: 60,
      label: "Hello <world>",
      style: { fill: "blue" },
    },
    {
      kind: "diamond",
      id: "d",
      x: 200,
      y: 10,
      width: 100,
      height: 80,
      style: { roughness: 1 },
    },
    {
      kind: "ellipse",
      id: "e",
      x: 10,
      y: 120,
      width: 80,
      height: 40,
      angle: 30,
      style: { opacity: 50 },
    },
    { kind: "text", id: "t", x: 200, y: 120, width: 100, height: 40, label: "Note" },
    { kind: "frame", id: "f", x: 0, y: 0, width: 390, height: 290, label: "Fig" },
    { kind: "image", id: "i", x: 300, y: 200, width: 40, height: 40, src: "a.png" },
    {
      kind: "mermaid",
      id: "m",
      x: 200,
      y: 200,
      width: 60,
      height: 40,
      source: "graph TD",
    },
    {
      kind: "arrow",
      id: "a",
      points: [
        { x: 134, y: 40 },
        { x: 196, y: 50 },
      ],
      start: { node: "r" },
      end: { node: "d" },
      label: "go",
      heads: { start: "dot", end: "triangle" },
    },
    {
      kind: "line",
      id: "l",
      points: [
        { x: 0, y: 280 },
        { x: 100, y: 280 },
      ],
      heads: { end: "bar" },
      style: { strokeStyle: "dashed" },
    },
    {
      kind: "freedraw",
      id: "p",
      points: [
        { x: 1, y: 1, pressure: 0.5 },
        { x: 9, y: 9, pressure: 0.5 },
      ],
      size: 2,
    },
    {
      kind: "math",
      id: "x",
      x: 20,
      y: 250,
      tex: "a<b",
      size: 16,
      style: { stroke: "sage" },
    },
  ] as FigureNode[],
};

describe("render", () => {
  it("draws every kind with resolved colours", () => {
    const svg = renderSceneSvg(SCENE);
    for (const id of SCENE.nodes.map((n) => n.id)) expect(svg).toContain(`id="${id}"`);
    expect(svg).toContain("Hello &lt;world&gt;");
    expect(svg).toContain(`fill="${tokenValue("blue", "print")}" fill-opacity="0.2"`);
    expect(svg).toContain('transform="rotate(30');
    expect(svg).toContain('opacity="0.5"');
    expect(svg).toContain("<circle");
    expect(svg).toContain("a&lt;b");
    expect(svg).not.toMatch(/var\(--/);
  });

  it("hides the exported frame's border and grows the page for outside content", () => {
    expect(
      nodeVisual(SCENE, SCENE.nodes[4] as FigureNode, "dark", "f"),
    ).toBeUndefined();
    const far: FigureScene = {
      ...createScene(100, 100),
      nodes: [{ kind: "rect", id: "z", x: 300, y: 300, width: 140, height: 70 }],
    };
    expect(exportRegion(far)).toEqual({ x: 0, y: 0, width: 456, height: 386 });
    expect(exportRegion(SCENE, "f")).toEqual({ x: 0, y: 0, width: 390, height: 290 });
  });

  it("measures label room and path midpoints", () => {
    expect(labelWidth("rect", 120)).toBe(104);
    expect(labelWidth("diamond", 100)).toBeCloseTo(39);
    expect(labelWidth("text", -50)).toBe(50);
    expect(
      pathMidpoint([
        { x: 0, y: 0 },
        { x: 10, y: 0 },
        { x: 10, y: 10 },
      ]),
    ).toEqual({ x: 10, y: 0 });
  });
});

describe("palette", () => {
  it("resolves tokens per theme and tints hues", () => {
    expect(TOKEN_NAMES).toContain("ink");
    expect(strokeColour("ink", "print")).toBe(tokenValue("ink", "print"));
    expect(strokeColour("nonsense", "dark")).toBe(tokenValue("ink", "dark"));
    expect(fillPaint("surface", "dark").alpha).toBe(1);
    expect(fillPaint("blue", "dark").alpha).toBeLessThan(1);
    expect(fillPaint("none", "light").colour).toBe("none");
    expect(isColour("vermillion") && !isColour("red")).toBe(true);
  });
});

describe("text", () => {
  it("wraps greedily and measures mono at 0.6 em", () => {
    expect(wrap("hello wide world", 60, 16, "sans")).toEqual([
      "hello",
      "wide",
      "world",
    ]);
    expect(measure("ab", 10, "mono")).toBeCloseTo(12);
    const size = naturalSize("ab\nabcd", 10, "mono");
    expect(size.width).toBeCloseTo(24);
    expect(size.height).toBeCloseTo(25);
  });
});
