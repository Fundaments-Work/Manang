import { describe, expect, it } from "vitest";

import { applyOp, deepEqual, nextGroupId, normalizeNode, type FigureOp } from "./ops";
import { createScene, migrateScene, type FigureNode, type FigureScene } from "./scene";

const rect = (id: string, x: number, y = 0): FigureNode => ({
  kind: "rect",
  id,
  x,
  y,
  width: 50,
  height: 50,
});

function run(scene: FigureScene, ...ops: FigureOp[]): FigureScene {
  return ops.reduce((s, op) => applyOp(s, op).scene, scene);
}

describe("applyOp", () => {
  it("reports what changed and whether anything did", () => {
    const s = createScene(100, 100);
    const added = applyOp(s, { op: "add", nodes: [rect("a", 0), rect("b", 100)] });
    expect(added.changed).toEqual(["a", "b"]);
    expect(added.applied).toBe(true);
    const still = applyOp(added.scene, { op: "move", ids: ["a"], dx: 0, dy: 0 });
    expect(still.applied).toBe(false);
    expect(still.scene).toBe(added.scene);
  });

  it("aligns, distributes, reorders and groups", () => {
    let s = run(createScene(800, 600), {
      op: "add",
      nodes: [rect("a", 0, 10), rect("b", 100, 40), rect("c", 400, 0)],
    });
    s = run(s, { op: "align", ids: ["a", "b", "c"], edge: "top" });
    expect(s.nodes.map((n) => (n.kind === "rect" ? n.y : -1))).toEqual([0, 0, 0]);
    s = run(s, { op: "distribute", ids: ["a", "b", "c"], axis: "horizontal" });
    expect(s.nodes.map((n) => (n.kind === "rect" ? n.x : -1))).toEqual([0, 200, 400]);
    s = run(s, { op: "reorder", ids: ["a"], to: "front" });
    expect(s.nodes.map((n) => n.id)).toEqual(["b", "c", "a"]);
    s = run(s, { op: "group", ids: ["a", "b"] });
    expect(nextGroupId(s)).toBe("g2");
    s = run(s, { op: "ungroup", ids: ["a", "b"] });
    expect(s.nodes.every((n) => n.groups === undefined)).toBe(true);
  });

  it("refuses unknown colours with the palette in the message", () => {
    expect(() =>
      applyOp(createScene(10, 10), {
        op: "add",
        nodes: [{ ...rect("a", 0), style: { stroke: "red" } } as FigureNode],
      }),
    ).toThrow(/vermillion/);
  });

  it("refuses unknown fields and bad kinds in updates", () => {
    const s = run(createScene(10, 10), { op: "add", nodes: [rect("a", 0)] });
    expect(() => applyOp(s, { op: "update", id: "a", set: { width: "wide" } })).toThrow(
      /cannot update/,
    );
    expect(() => applyOp(s, { op: "update", id: "a", set: { kind: "arrow" } })).toThrow(
      /cannot update/,
    );
    expect(() => applyOp(s, { op: "undo" })).toThrow(/batch/);
  });
});

describe("normal form", () => {
  it("omits defaults like serde", () => {
    expect(
      normalizeNode({
        kind: "arrow",
        id: "a",
        points: [],
        routing: "straight",
        heads: {},
        groups: [],
        locked: false,
        style: {},
      }),
    ).toEqual({ kind: "arrow", id: "a", points: [] });
    expect(deepEqual({ a: 1, b: undefined }, { a: 1 })).toBe(true);
  });
});

describe("migrateScene", () => {
  it("upgrades v1 and refuses newer versions", () => {
    const s = migrateScene({
      version: 1,
      width: 10,
      height: 10,
      nodes: [
        {
          kind: "rect",
          id: "n1",
          x: 10,
          y: 10,
          width: -4,
          height: 3,
          stroke: "#e8e6e1",
        },
        { kind: "ellipse", id: "n2", cx: 5, cy: 5, rx: 2, ry: 1 },
        {
          kind: "arrow",
          id: "n3",
          from: { x: 0, y: 0 },
          to: { x: 1, y: 1 },
          color: "#e8e6e1",
        },
        { kind: "stroke", id: "n4", points: [], size: 2, color: "#e8e6e1" },
        { kind: "math", id: "n5", x: 1, y: 1, tex: "x", size: 9, color: "#b3d17a" },
      ],
    });
    expect(s.version).toBe(2);
    expect(s.nodes.map((n) => n.kind)).toEqual([
      "rect",
      "ellipse",
      "arrow",
      "freedraw",
      "math",
    ]);
    expect(s.nodes[0]).toMatchObject({ x: 6, width: 4, style: { stroke: "#e8e6e1" } });
    expect(s.nodes[1]).toMatchObject({ x: 3, y: 4, width: 4, height: 2 });
    expect(() => migrateScene({ version: 9, width: 1, height: 1, nodes: [] })).toThrow(
      /version 9/,
    );
    expect(() =>
      migrateScene({ version: 1, width: 1, height: 1, nodes: [{ kind: "x" }] }),
    ).toThrow(/unknown node kind/);
  });
});
