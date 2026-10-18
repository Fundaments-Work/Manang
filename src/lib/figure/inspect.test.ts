import { describe, expect, it } from "vitest";

import { applyOp } from "./ops";
import {
  canRotate,
  filterLayers,
  layerName,
  layerOrder,
  layoutBox,
  layoutEdit,
  moveLayerOp,
  moveSteps,
} from "./inspect";
import { createScene, type FigureNode, type FigureScene } from "./scene";

const nodes: FigureNode[] = [
  { kind: "rect", id: "a", x: 0, y: 0, width: 100, height: 50 },
  { kind: "ellipse", id: "b", x: 200, y: 100, width: 100, height: 100, label: "Core" },
  {
    kind: "arrow",
    id: "c",
    points: [
      { x: 0, y: 0 },
      { x: 40, y: 0 },
    ],
  },
  { kind: "rect", id: "d", x: 10, y: 10, width: 20, height: 20, locked: true },
];
const scene: FigureScene = { ...createScene(960, 540), nodes };

describe("layoutBox", () => {
  it("is the node's own box for one node and the union for several", () => {
    expect(layoutBox(scene, ["a"])).toEqual({ x: 0, y: 0, width: 100, height: 50 });
    const both = layoutBox(scene, ["a", "b"]);
    expect([both.x, both.y, both.width, both.height]).toEqual([0, 0, 300, 200]);
  });
});

describe("layoutEdit", () => {
  it("moves by the difference for X and Y", () => {
    const op = layoutEdit(scene, ["a"], "x", 30, false);
    expect(op).toEqual({ op: "move", ids: ["a"], dx: 30, dy: 0 });
    expect(layoutEdit(scene, ["a"], "y", 0, false)).toBeUndefined();
  });

  it("sets the size of one shape", () => {
    const op = layoutEdit(scene, ["a"], "width", 200, false);
    if (op === undefined) throw new Error("no op");
    const after = applyOp(scene, op).scene.nodes[0];
    expect(after).toMatchObject({ x: 0, y: 0, width: 200, height: 50 });
  });

  it("keeps the proportions when locked", () => {
    const op = layoutEdit(scene, ["a"], "height", 100, true);
    if (op === undefined) throw new Error("no op");
    expect(applyOp(scene, op).scene.nodes[0]).toMatchObject({
      width: 200,
      height: 100,
    });
  });

  it("scales a multi-selection from its top left", () => {
    const op = layoutEdit(scene, ["a", "b"], "width", 600, false);
    if (op === undefined) throw new Error("no op");
    const next = applyOp(scene, op).scene;
    expect(next.nodes[0]).toMatchObject({ x: 0, width: 200 });
    expect(next.nodes[1]).toMatchObject({ x: 400, width: 200 });
  });

  it("scales arrows by their points", () => {
    const op = layoutEdit(scene, ["c"], "width", 80, false);
    if (op === undefined) throw new Error("no op");
    const arrow = applyOp(scene, op).scene.nodes[2];
    expect(arrow?.kind === "arrow" && arrow.points[1]?.x).toBe(80);
  });

  it("never makes a side smaller than one unit", () => {
    const op = layoutEdit(scene, ["a"], "width", -5, false);
    if (op === undefined) throw new Error("no op");
    expect(applyOp(scene, op).scene.nodes[0]).toMatchObject({ width: 1 });
  });

  it("leaves locked nodes alone", () => {
    expect(layoutEdit(scene, ["d"], "x", 50, false)).toBeUndefined();
    const op = layoutEdit(scene, ["a", "d"], "x", 5, false);
    expect(op).toMatchObject({ op: "move", ids: ["a"] });
  });
});

describe("layer names and order", () => {
  it("names a layer by its label, else by type and id", () => {
    expect(layerName(nodes[1] as FigureNode)).toBe("Core");
    expect(layerName(nodes[0] as FigureNode)).toBe("Rectangle a");
    expect(
      layerName({ kind: "math", id: "m", x: 0, y: 0, source: "x^2", size: 20 }),
    ).toBe("x^2");
    const long = { ...(nodes[0] as FigureNode), label: "x".repeat(80) };
    expect(layerName(long).length).toBe(40);
  });

  it("lists the topmost first and filters by name or type", () => {
    expect(layerOrder(scene).map((n) => n.id)).toEqual(["d", "c", "b", "a"]);
    expect(filterLayers(nodes, "core").map((n) => n.id)).toEqual(["b"]);
    expect(filterLayers(nodes, "RECT").map((n) => n.id)).toEqual(["a", "d"]);
    expect(filterLayers(nodes, "  ")).toHaveLength(4);
  });

  it("rotation is for shapes", () => {
    expect(canRotate([nodes[0] as FigureNode])).toBe(true);
    expect(canRotate([nodes[0] as FigureNode, nodes[2] as FigureNode])).toBe(false);
    expect(canRotate([])).toBe(false);
  });
});

describe("moveSteps", () => {
  const paint = ["a", "b", "c", "d"];

  it("steps forward to be drawn above a layer in front", () => {
    expect(moveSteps(paint, "a", "c", "above")).toEqual(["forward", "forward"]);
    expect(moveSteps(paint, "a", "c", "below")).toEqual(["forward"]);
  });

  it("steps backward to go behind", () => {
    expect(moveSteps(paint, "d", "a", "below")).toEqual([
      "backward",
      "backward",
      "backward",
    ]);
    expect(moveSteps(paint, "d", "b", "above")).toEqual(["backward"]);
  });

  it("does nothing when it is already there", () => {
    expect(moveSteps(paint, "b", "a", "above")).toEqual([]);
    expect(moveSteps(paint, "b", "b", "above")).toEqual([]);
    expect(moveSteps(paint, "x", "a", "above")).toEqual([]);
  });

  it("the batch reorders the scene as asked", () => {
    const op = moveLayerOp(scene, "a", "c", "above");
    if (op === undefined) throw new Error("no op");
    expect(applyOp(scene, op).scene.nodes.map((n) => n.id)).toEqual([
      "b",
      "c",
      "a",
      "d",
    ]);
    expect(moveLayerOp(scene, "d", "c", "above")).toBeUndefined();
  });
});
