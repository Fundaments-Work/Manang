import { describe, expect, it } from "vitest";

import { selectionBounds } from "./selection";
import { applyOp } from "./ops";
import { mapRect, scaleOps, scaleSelection } from "./scale";
import { createScene, type FigureNode, type FigureScene } from "./scene";

const nodes: FigureNode[] = [
  { kind: "rect", id: "a", x: 0, y: 0, width: 100, height: 50 },
  { kind: "ellipse", id: "b", x: 200, y: 100, width: 100, height: 100 },
  { kind: "text", id: "t", x: 0, y: 150, width: 80, height: 20, label: "Hi" },
  {
    kind: "freedraw",
    id: "f",
    points: [
      { x: 10, y: 210, pressure: 0.5 },
      { x: 90, y: 250, pressure: 0.5 },
    ],
    size: 2,
  },
  {
    kind: "math",
    id: "m",
    x: 120,
    y: 10,
    source: "x",
    size: 20,
    width: 40,
    height: 28,
  },
  {
    kind: "arrow",
    id: "e",
    points: [
      { x: 100, y: 25 },
      { x: 200, y: 150 },
    ],
    start: { node: "a" },
    end: { node: "b" },
  },
];

const scene: FigureScene = { ...createScene(800, 600), nodes };

describe("scaling a selection", () => {
  it("maps a box through the selection frame", () => {
    const from = { x: 0, y: 0, width: 100, height: 100 };
    const to = { x: 10, y: 20, width: 200, height: 50 };
    expect(mapRect({ x: 50, y: 50, width: 20, height: 20 }, from, to)).toEqual({
      x: 110,
      y: 45,
      width: 40,
      height: 10,
    });
    // A degenerate frame leaves that axis alone.
    expect(
      mapRect(
        { x: 1, y: 5, width: 2, height: 0 },
        { x: 0, y: 5, width: 10, height: 0 },
        { x: 0, y: 5, width: 20, height: 9 },
      ),
    ).toEqual({ x: 2, y: 5, width: 4, height: 0 });
  });

  it("does nothing when the frame did not change, and skips locked nodes", () => {
    const frame = { x: 0, y: 0, width: 10, height: 10 };
    expect(scaleOps(nodes, frame, frame)).toEqual([]);
    expect(scaleSelection(scene, ["a"], frame, frame)).toBeUndefined();
    const locked = nodes.map((n) => (n.id === "a" ? { ...n, locked: true } : n));
    const ops = scaleOps(locked, frame, { ...frame, width: 20 });
    expect(ops.map((o) => (o.op === "resize" ? o.id : ""))).not.toContain("a");
  });

  it("scales every kind in one batch and keeps arrow bindings", () => {
    const ids = nodes.map((n) => n.id);
    const from = selectionBounds(scene, ids);
    const to = { x: from.x, y: from.y, width: from.width * 2, height: from.height * 2 };
    const op = scaleSelection(scene, ids, from, to);
    expect(op?.op).toBe("batch");
    const next = applyOp(scene, op as NonNullable<typeof op>).scene;
    const get = (id: string) => next.nodes.find((n) => n.id === id) as FigureNode;
    expect(get("a")).toMatchObject({ x: 0, y: 0, width: 200, height: 100 });
    expect(get("b")).toMatchObject({ x: 400, y: 200, width: 200, height: 200 });
    expect(get("t")).toMatchObject({ x: 0, y: 300, width: 160, height: 40 });
    const f = get("f");
    expect(f.kind === "freedraw" && f.points[1]).toMatchObject({ x: 180, y: 500 });
    // A formula scales its size (and its measured box) with the selection.
    const m = get("m");
    expect(m.kind === "math" && m.size).toBe(40);
    expect(m).toMatchObject({ x: 240, y: 20, width: 80, height: 56 });
    // Bindings stay; the arrow was re-routed to the scaled shapes.
    const e = get("e");
    expect(e.kind === "arrow" && e.start).toEqual({ node: "a" });
    expect(e.kind === "arrow" && e.end).toEqual({ node: "b" });
    // One history step: the whole batch is a single operation.
    expect(applyOp(scene, op as NonNullable<typeof op>).applied).toBe(true);
  });

  it("scales by a handle drag: width only changes width", () => {
    const from = selectionBounds(scene, ["a", "b"]);
    const to = { ...from, width: from.width / 2 };
    const next = applyOp(
      scene,
      scaleSelection(scene, ["a", "b"], from, to) as never,
    ).scene;
    const a = next.nodes[0];
    expect(a).toMatchObject({ x: 0, width: 50, height: 50 });
  });
});
