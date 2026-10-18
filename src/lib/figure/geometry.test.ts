import { describe, expect, it } from "vitest";

import {
  anchorOf,
  anchorPoint,
  formatCoord,
  frameChildren,
  freedrawPath,
  hitTest,
  hits,
  nodeBounds,
  nodesInRect,
  normalizeRect,
  outlinePoint,
  polygonToPath,
  rotatePoint,
  sceneBounds,
  unionRect,
} from "./geometry";
import {
  createScene,
  type FigureNode,
  type FigureScene,
  type ShapeNode,
} from "./scene";

function box(
  kind: ShapeNode["kind"],
  id: string,
  x: number,
  y: number,
  w = 100,
  h = 50,
): ShapeNode {
  return { kind, id, x, y, width: w, height: h };
}

function scene(...nodes: FigureNode[]): FigureScene {
  return { ...createScene(800, 600), nodes };
}

describe("coordinates", () => {
  it("rounds halves up and formats like Rust", () => {
    expect(formatCoord(10)).toBe("10");
    expect(formatCoord(10.25)).toBe("10.3");
    expect(formatCoord(-2.25)).toBe("-2.2");
    expect(formatCoord(-0.04)).toBe("0");
    expect(formatCoord(1.95)).toBe("2");
  });

  it("normalises inverted rects and unions them", () => {
    expect(normalizeRect(10, 10, -5, -5)).toEqual({ x: 5, y: 5, width: 5, height: 5 });
    expect(
      unionRect(
        { x: 0, y: 0, width: 0, height: 0 },
        { x: 1, y: 1, width: 2, height: 2 },
      ),
    ).toEqual({
      x: 1,
      y: 1,
      width: 2,
      height: 2,
    });
  });
});

describe("bounds and hits", () => {
  it("includes rotation in bounds", () => {
    const n: ShapeNode = { ...box("rect", "r", 0, 0, 100, 20), angle: 90 };
    const b = nodeBounds(n);
    expect(b.width).toBeCloseTo(20);
    expect(b.height).toBeCloseTo(100);
    expect(hits(n, { x: 50, y: -30 }, 0)).toBe(true);
    expect(hits(n, { x: 5, y: 10 }, 0)).toBe(false);
  });

  it("hits diamonds and ellipses by their outline, connectors near the line", () => {
    const d = box("diamond", "d", 0, 0, 100, 100);
    expect(hits(d, { x: 50, y: 50 }, 0)).toBe(true);
    expect(hits(d, { x: 5, y: 5 }, 0)).toBe(false);
    const e = box("ellipse", "e", 0, 0, 100, 100);
    expect(hits(e, { x: 3, y: 3 }, 0)).toBe(false);
    const a: FigureNode = {
      kind: "arrow",
      id: "a",
      points: [
        { x: 0, y: 0 },
        { x: 100, y: 0 },
      ],
    };
    expect(hits(a, { x: 50, y: 5 }, 6)).toBe(true);
    expect(hits(a, { x: 50, y: 20 }, 6)).toBe(false);
  });

  it("lets frames lose to what they contain", () => {
    const s = scene(
      box("rect", "inner", 20, 20, 20, 20),
      box("frame", "f", 0, 0, 90, 90),
    );
    expect(hitTest(s, { x: 30, y: 30 })?.id).toBe("inner");
    expect(hitTest(s, { x: 1, y: 50 })?.id).toBe("f");
    expect(hitTest(s, { x: 60, y: 60 })).toBeUndefined();
    expect(frameChildren(s, "f")).toEqual(["inner"]);
    expect(
      nodesInRect(s, { x: 10, y: 10, width: 40, height: 40 }).map((n) => n.id),
    ).toEqual(["inner"]);
    expect(sceneBounds(s)).toEqual({ x: 0, y: 0, width: 90, height: 90 });
  });
});

describe("outlines and anchors", () => {
  it("lands on each outline", () => {
    expect(outlinePoint(box("rect", "r", 0, 0), { x: 500, y: 25 })).toEqual({
      x: 100,
      y: 25,
    });
    expect(outlinePoint(box("diamond", "d", 0, 0), { x: 50, y: -100 })).toEqual({
      x: 50,
      y: 0,
    });
    const p = outlinePoint(box("ellipse", "e", 0, 0, 100, 100), { x: 200, y: 200 });
    expect(Math.hypot(p.x - 50, p.y - 50)).toBeCloseTo(50);
  });

  it("round-trips anchors", () => {
    const r = box("rect", "r", 10, 10);
    const a = anchorOf(r, { x: 110, y: 35 });
    expect(a).toEqual({ x: 1, y: 0.5 });
    expect(anchorPoint(r, a)).toEqual({ x: 110, y: 35 });
  });

  it("rotates clockwise on screen", () => {
    const p = rotatePoint({ x: 1, y: 0 }, { x: 0, y: 0 }, 90);
    expect(p.x).toBeCloseTo(0);
    expect(p.y).toBeCloseTo(1);
  });
});

describe("paths", () => {
  it("draws freehand outlines and polygons", () => {
    const d = freedrawPath({
      points: [
        { x: 0, y: 0, pressure: 0.5 },
        { x: 20, y: 10, pressure: 0.5 },
      ],
      size: 3,
    });
    expect(d.startsWith("M")).toBe(true);
    expect(d.endsWith("Z")).toBe(true);
    expect(freedrawPath({ points: [], size: 3 })).toBe("");
    expect(
      polygonToPath([
        { x: 0, y: 0 },
        { x: 1.25, y: 2 },
      ]),
    ).toBe("M0 0L1.3 2Z");
  });
});
