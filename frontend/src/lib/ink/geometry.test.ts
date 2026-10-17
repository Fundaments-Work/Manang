import { describe, expect, it } from "vitest";

import {
  allAnchors,
  anchorsOf,
  arrowHead,
  hitTest,
  nodeBounds,
  normalizeRect,
  polygonToPath,
  sceneBounds,
  SNAP_DISTANCE,
  snapToAnchors,
  strokeToPath,
  unionRect,
} from "./geometry";
import {
  appendNode,
  createScene,
  formatNodeId,
  removeNode,
  replaceNode,
  type FigureNode,
  type FigureScene,
  type RectNode,
} from "./scene";

function rectAt(x: number, y: number, width = 100, height = 50): RectNode {
  return { kind: "rect", id: "r1", x, y, width, height, stroke: "#e8e6e1" };
}

describe("nodeBounds", () => {
  it("measures a rect", () => {
    expect(nodeBounds(rectAt(10, 20, 30, 40))).toEqual({
      x: 10,
      y: 20,
      width: 30,
      height: 40,
    });
  });

  it("measures an ellipse from its radii", () => {
    const ellipse: FigureNode = {
      kind: "ellipse",
      id: "e1",
      cx: 50,
      cy: 50,
      rx: 20,
      ry: 10,
      stroke: "#e8e6e1",
    };
    expect(nodeBounds(ellipse)).toEqual({ x: 30, y: 40, width: 40, height: 20 });
  });

  it("pads a stroke by half its base size", () => {
    const stroke: FigureNode = {
      kind: "stroke",
      id: "s1",
      points: [
        { x: 10, y: 10, pressure: 0.5 },
        { x: 30, y: 10, pressure: 0.5 },
      ],
      size: 4,
      color: "#e8e6e1",
    };
    const bounds = nodeBounds(stroke);
    expect(bounds.x).toBe(8);
    expect(bounds.width).toBe(24);
  });

  it("returns an empty rect for an empty stroke", () => {
    const stroke: FigureNode = {
      kind: "stroke",
      id: "s1",
      points: [],
      size: 4,
      color: "#e8e6e1",
    };
    expect(nodeBounds(stroke)).toEqual({ x: 0, y: 0, width: 0, height: 0 });
  });
});

describe("normalizeRect", () => {
  it("pulls a negative extent back to its origin", () => {
    expect(normalizeRect(100, 100, -40, -20)).toEqual({
      x: 60,
      y: 80,
      width: 40,
      height: 20,
    });
  });
});

describe("unionRect", () => {
  it("absorbs an empty rect", () => {
    const rect = { x: 5, y: 5, width: 10, height: 10 };
    expect(unionRect({ x: 0, y: 0, width: 0, height: 0 }, rect)).toEqual(rect);
  });

  it("spans both rects", () => {
    expect(
      unionRect(
        { x: 0, y: 0, width: 10, height: 10 },
        { x: 20, y: 5, width: 10, height: 10 },
      ),
    ).toEqual({ x: 0, y: 0, width: 30, height: 15 });
  });
});

describe("sceneBounds and hitTest", () => {
  const scene: FigureScene = {
    ...createScene(400, 400),
    nodes: [rectAt(0, 0), { ...rectAt(200, 200), id: "r2" }],
  };

  it("spans every node", () => {
    expect(sceneBounds(scene)).toEqual({ x: 0, y: 0, width: 300, height: 250 });
  });

  it("returns the topmost node under a point", () => {
    // Overlapping region; the later node paints on top and must win.
    const stacked: FigureScene = {
      ...createScene(400, 400),
      nodes: [rectAt(0, 0, 200, 200), { ...rectAt(50, 50, 200, 200), id: "r2" }],
    };
    expect(hitTest(stacked, { x: 100, y: 100 })?.id).toBe("r2");
  });

  it("returns undefined outside every node", () => {
    expect(hitTest(scene, { x: 390, y: 390 })).toBeUndefined();
  });
});

describe("anchors and snapping", () => {
  const rect = rectAt(100, 100, 200, 100);

  it("places four anchors on the perimeter", () => {
    expect(anchorsOf(rect)).toEqual([
      { nodeId: "r1", side: "top", x: 200, y: 100 },
      { nodeId: "r1", side: "right", x: 300, y: 150 },
      { nodeId: "r1", side: "bottom", x: 200, y: 200 },
      { nodeId: "r1", side: "left", x: 100, y: 150 },
    ]);
  });

  it("excludes the node being dragged", () => {
    const scene: FigureScene = {
      ...createScene(400, 400),
      nodes: [rect, { ...rect, id: "r2", x: 300 }],
    };
    const anchors = allAnchors(scene, ["r1"]);
    expect(anchors.every((anchor) => anchor.nodeId === "r2")).toBe(true);
  });

  it("snaps to the nearest anchor within the threshold", () => {
    const result = snapToAnchors({ x: 202, y: 104 }, anchorsOf(rect));
    expect(result.point).toEqual({ x: 200, y: 100 });
    expect(result.anchor?.side).toBe("top");
  });

  it("leaves the point alone beyond the threshold", () => {
    const far = { x: 200 + SNAP_DISTANCE + 1, y: 100 };
    const result = snapToAnchors(far, anchorsOf(rect));
    expect(result.point).toEqual(far);
    expect(result.anchor).toBeUndefined();
  });

  it("picks the closest anchor when several are in range", () => {
    // Between the right (300,150) and bottom (200,200) anchors, but nearer right.
    const result = snapToAnchors({ x: 260, y: 175 }, anchorsOf(rect), 50);
    expect(result.anchor?.side).toBe("right");
  });

  it("snaps to an anchor exactly at the threshold", () => {
    const onThreshold = { x: 200 + SNAP_DISTANCE, y: 100 };
    expect(snapToAnchors(onThreshold, anchorsOf(rect)).anchor).toBeDefined();
  });
});

describe("strokeToPath", () => {
  it("returns an empty path for an empty stroke", () => {
    expect(
      strokeToPath({ kind: "stroke", id: "s", points: [], size: 2, color: "#fff" }),
    ).toBe("");
  });

  it("renders a single tap as a closed dot", () => {
    // A tap with the pen should leave a dot rather than nothing at all.
    const path = strokeToPath({
      kind: "stroke",
      id: "s",
      points: [{ x: 1, y: 2, pressure: 0.5 }],
      size: 2,
      color: "#fff",
    });
    expect(path.startsWith("M")).toBe(true);
    expect(path.endsWith("Z")).toBe(true);
  });

  it("produces a closed path for a real stroke", () => {
    const points = Array.from({ length: 24 }, (_, index) => ({
      x: index * 4,
      y: 40 + Math.sin(index / 2) * 12,
      pressure: 0.5,
    }));
    const path = strokeToPath({
      kind: "stroke",
      id: "s",
      points,
      size: 3,
      color: "#e8e6e1",
    });

    expect(path.startsWith("M")).toBe(true);
    expect(path.endsWith("Z")).toBe(true);
    expect(path.split("L").length).toBeGreaterThan(10);
  });

  it("is deterministic for identical input", () => {
    const node = {
      kind: "stroke" as const,
      id: "s",
      points: [
        { x: 0, y: 0, pressure: 0.4 },
        { x: 9, y: 4, pressure: 0.6 },
        { x: 18, y: 2, pressure: 0.5 },
      ],
      size: 3,
      color: "#e8e6e1",
    };
    expect(strokeToPath(node)).toBe(strokeToPath(node));
  });
});

describe("arrowHead", () => {
  it("starts at the tip and returns four points", () => {
    const head = arrowHead({ x: 0, y: 0 }, { x: 100, y: 0 });
    expect(head).toHaveLength(4);
    expect(head[0]).toEqual({ x: 100, y: 0 });
  });

  it("points along the direction of travel", () => {
    const head = arrowHead({ x: 0, y: 0 }, { x: 0, y: 100 });
    expect(head[0]).toEqual({ x: 0, y: 100 });
  });
});

describe("polygonToPath", () => {
  it("closes the polygon", () => {
    expect(
      polygonToPath([
        { x: 0, y: 0 },
        { x: 10, y: 0 },
        { x: 10, y: 10 },
      ]),
    ).toBe("M0 0L10 0L10 10Z");
  });

  it("returns an empty string for no points", () => {
    expect(polygonToPath([])).toBe("");
  });
});

describe("scene mutation", () => {
  const base = createScene(100, 100);

  it("appends without mutating the original", () => {
    const next = appendNode(base, rectAt(0, 0));
    expect(base.nodes).toHaveLength(0);
    expect(next.nodes).toHaveLength(1);
  });

  it("replaces by id", () => {
    const withNode = appendNode(base, rectAt(0, 0));
    const moved = replaceNode(withNode, { ...rectAt(0, 0), x: 50 });
    expect(moved.nodes[0]).toMatchObject({ x: 50 });
  });

  it("removes by id", () => {
    const withNode = appendNode(base, rectAt(0, 0));
    expect(removeNode(withNode, "r1").nodes).toHaveLength(0);
  });

  it("generates stable, sortable ids from a counter", () => {
    expect(formatNodeId(1)).toBe("n1");
    expect(formatNodeId(35)).toBe("nz");
    expect(formatNodeId(36)).toBe("n10");
  });
});
