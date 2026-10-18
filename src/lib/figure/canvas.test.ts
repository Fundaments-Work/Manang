import { describe, expect, it } from "vitest";

import {
  CLIPBOARD_TYPE,
  looksLikeMermaid,
  parseNodes,
  reidentify,
  serializeNodes,
} from "./clipboard";
import { handleAt, handlePositions, resizeBox, rotationFor } from "./handles";
import { TOOLS, actionFor, toolForKey, type KeyEventLike } from "./keymap";
import { createScene, type FigureNode, type FigureScene } from "./scene";
import {
  expandGroups,
  marqueeSelect,
  pickAt,
  planDrag,
  previewMove,
  selectAll,
  selectionBounds,
  toggleSelection,
} from "./selection";
import { bestOffset, snapMove, snapResize } from "./snapping";
import {
  cull,
  fitRect,
  pan,
  stepZoom,
  toScene,
  toScreen,
  visibleRect,
  viewBox,
  zoomAt,
} from "./viewport";

const box = (
  id: string,
  x: number,
  y: number,
  extra: Partial<FigureNode> = {},
): FigureNode =>
  ({ kind: "rect", id, x, y, width: 50, height: 50, ...extra }) as FigureNode;

function scene(...nodes: FigureNode[]): FigureScene {
  return { ...createScene(800, 600), nodes };
}

describe("viewport", () => {
  const v = { x: 100, y: 50, zoom: 2 };

  it("maps between screen and scene", () => {
    expect(toScreen(v, { x: 110, y: 60 })).toEqual({ x: 20, y: 20 });
    expect(toScene(v, { x: 20, y: 20 })).toEqual({ x: 110, y: 60 });
    expect(viewBox(v, 400, 200)).toBe("100 50 200 100");
    expect(pan(v, 20, -10)).toEqual({ x: 90, y: 55, zoom: 2 });
  });

  it("zooms around the cursor", () => {
    const z = zoomAt(v, 4, { x: 40, y: 40 });
    expect(z.zoom).toBe(4);
    expect(toScene(z, { x: 40, y: 40 })).toEqual(toScene(v, { x: 40, y: 40 }));
    expect(zoomAt(v, 100, { x: 0, y: 0 }).zoom).toBe(8);
    expect(stepZoom(1, 1)).toBe(1.1);
    expect(stepZoom(1, -1)).toBe(0.9);
    expect(stepZoom(8, 1)).toBe(8);
  });

  it("fits a rect centred, capped at 200%", () => {
    const f = fitRect({ x: 0, y: 0, width: 400, height: 200 }, 896, 496, 48);
    expect(f.zoom).toBe(2);
    expect(toScreen(f, { x: 200, y: 100 })).toEqual({ x: 448, y: 248 });
    const small = fitRect({ x: 0, y: 0, width: 4000, height: 100 }, 1000, 500, 0);
    expect(small.zoom).toBe(0.25);
  });

  it("culls to the visible rect with a margin", () => {
    const view = visibleRect({ x: 0, y: 0, zoom: 1 }, 100, 100, 10);
    expect(view).toEqual({ x: -10, y: -10, width: 120, height: 120 });
    const nodes = [box("in", 50, 50), box("edge", 105, 0), box("out", 500, 500)];
    expect(cull(nodes, view).map((n) => n.id)).toEqual(["in", "edge"]);
  });
});

describe("snapping", () => {
  it("snaps edges and centres within the threshold and draws guides", () => {
    expect(bestOffset([0, 10], [13, 50], 4)).toBe(3);
    expect(bestOffset([0], [50], 4)).toBeUndefined();
    const r = snapMove(
      { x: 103, y: 0, width: 50, height: 50 },
      [{ x: 0, y: 200, width: 100, height: 50 }],
      6,
    );
    expect(r.dx).toBe(-3);
    expect(r.dy).toBe(0);
    expect(r.guides).toEqual([{ axis: "x", x: 100, from: 0, to: 250 }]);
    const none = snapMove(
      { x: 300, y: 300, width: 10, height: 10 },
      [{ x: 0, y: 0, width: 10, height: 10 }],
      6,
    );
    expect(none).toEqual({ dx: 0, dy: 0, guides: [] });
  });

  it("snaps only the edges a resize moves", () => {
    const r = snapResize(
      { x: 0, y: 0, width: 97, height: 40 },
      { left: false, right: true, top: false, bottom: false },
      [{ x: 100, y: 100, width: 10, height: 10 }],
      6,
    );
    expect(r.box).toEqual({ x: 0, y: 0, width: 100, height: 40 });
    expect(r.guides.length).toBe(1);
  });
});

describe("handles", () => {
  const r = { x: 0, y: 0, width: 100, height: 50 };

  it("places handles and finds the one under the pointer", () => {
    const p = handlePositions(r, 0, 1);
    expect(p.se).toEqual({ x: 100, y: 50 });
    expect(p.rotate).toEqual({ x: 50, y: -22 });
    expect(handleAt(p, { x: 98, y: 52 }, 5)).toBe("se");
    expect(handleAt(p, { x: 50, y: 25 }, 5)).toBeUndefined();
    const rotated = handlePositions(r, 90, 1);
    expect(rotated.n.x).toBeCloseTo(75);
    expect(rotated.n.y).toBeCloseTo(25);
  });

  it("resizes from the opposite corner, with aspect lock", () => {
    expect(resizeBox(r, 0, "se", { x: 150, y: 80 }, false)).toEqual({
      x: 0,
      y: 0,
      width: 150,
      height: 80,
    });
    expect(resizeBox(r, 0, "nw", { x: -10, y: -10 }, false)).toEqual({
      x: -10,
      y: -10,
      width: 110,
      height: 60,
    });
    expect(resizeBox(r, 0, "e", { x: 200, y: 999 }, false)).toEqual({
      x: 0,
      y: 0,
      width: 200,
      height: 50,
    });
    expect(resizeBox(r, 0, "se", { x: 200, y: 60 }, true)).toEqual({
      x: 0,
      y: 0,
      width: 200,
      height: 100,
    });
    // Dragged past the anchor: the box flips instead of going negative.
    expect(resizeBox(r, 0, "e", { x: -20, y: 0 }, false)).toEqual({
      x: -20,
      y: 0,
      width: 20,
      height: 50,
    });
  });

  it("keeps the opposite corner fixed on a rotated box", () => {
    const angle = 30;
    const before = handlePositions(r, angle, 1).nw;
    const out = resizeBox(r, angle, "se", { x: 140, y: 120 }, false);
    const after = handlePositions(out, angle, 1).nw;
    expect(after.x).toBeCloseTo(before.x);
    expect(after.y).toBeCloseTo(before.y);
  });

  it("rotates toward the pointer, snapping to 15 degrees", () => {
    expect(rotationFor({ x: 0, y: 0 }, { x: 10, y: 0 }, false)).toBeCloseTo(90);
    expect(rotationFor({ x: 0, y: 0 }, { x: 0, y: -10 }, false)).toBeCloseTo(0);
    expect(rotationFor({ x: 0, y: 0 }, { x: 10, y: 1 }, true)).toBe(90);
  });
});

describe("selection", () => {
  const s = scene(
    box("a", 0, 0, { groups: ["g1", "g2"] }),
    box("b", 100, 0, { groups: ["g2"] }),
    box("c", 200, 0),
    box("lock", 300, 0, { locked: true }),
    { kind: "frame", id: "f", x: -10, y: -10, width: 400, height: 100 },
  );

  it("grows to outermost groups and skips locked nodes", () => {
    expect(expandGroups(s, ["a"])).toEqual(["a", "b"]);
    expect(pickAt(s, { x: 10, y: 10 })?.id).toBe("a");
    expect(pickAt(s, { x: 310, y: 10 })).toBeUndefined();
    expect(pickAt(s, { x: -9, y: 40 })?.id).toBe("f");
    expect(marqueeSelect(s, { x: -1, y: -1, width: 60, height: 60 })).toEqual([
      "a",
      "b",
    ]);
    expect(selectAll(s)).toEqual(["a", "b", "c", "f"]);
    expect(toggleSelection(["a", "b"], ["a", "b"])).toEqual([]);
    expect(toggleSelection(["c"], ["a"])).toEqual(["c", "a"]);
    expect(selectionBounds(s, ["a", "c"])).toEqual({
      x: 0,
      y: 0,
      width: 250,
      height: 50,
    });
  });

  it("previews a drag with bound arrows following, untouched nodes kept", () => {
    const t = scene(box("a", 0, 0), box("b", 300, 0), {
      kind: "arrow",
      id: "e",
      points: [
        { x: 54, y: 25 },
        { x: 296, y: 25 },
      ],
      start: { node: "a" },
      end: { node: "b" },
    });
    const plan = planDrag(t, ["b"]);
    expect(plan.bound).toEqual([2]);
    const out = previewMove(t, plan, { x: 0, y: 100 });
    expect(out.nodes[0]).toBe(t.nodes[0]);
    expect(out.nodes[1]).toMatchObject({ y: 100 });
    const e = out.nodes[2];
    expect(e?.kind === "arrow" && (e.points[1]?.y ?? 0) > 100).toBe(true);
  });
});

describe("clipboard", () => {
  it("round-trips and re-ids with remapped bindings and groups", () => {
    const nodes: FigureNode[] = [
      box("a", 0, 0, { groups: ["g1"] }),
      box("b", 100, 0, { groups: ["g1"] }),
      {
        kind: "arrow",
        id: "e",
        points: [
          { x: 0, y: 0 },
          { x: 1, y: 1 },
        ],
        start: { node: "a" },
        end: { node: "outside" },
      },
    ];
    const text = serializeNodes(nodes);
    expect(JSON.parse(text).type).toBe(CLIPBOARD_TYPE);
    expect(parseNodes(text)).toEqual(nodes);
    expect(parseNodes("hello")).toBeUndefined();
    expect(parseNodes('{"type":"other"}')).toBeUndefined();
    let n = 10;
    let g = 5;
    const copies = reidentify(
      nodes,
      () => `n${(n += 1)}`,
      () => `g${(g += 1)}`,
    );
    expect(copies.map((c) => c.id)).toEqual(["n11", "n12", "n13"]);
    expect(copies[0]).toMatchObject({ x: 16, y: 16, groups: ["g6"] });
    expect(copies[1]?.groups).toEqual(["g6"]);
    const arrow = copies[2];
    expect(arrow?.kind === "arrow" && arrow.start).toEqual({ node: "n11" });
    expect(arrow?.kind === "arrow" && arrow.end).toBeUndefined();
  });

  it("recognises Mermaid text", () => {
    expect(looksLikeMermaid("flowchart TD\n A-->B")).toBe(true);
    expect(looksLikeMermaid("\n%% c\ngraph LR\nA-->B")).toBe(true);
    expect(looksLikeMermaid("---\ntitle: x\n---\nsequenceDiagram\nA->>B: hi")).toBe(
      true,
    );
    expect(looksLikeMermaid("stateDiagram-v2\n[*] --> A")).toBe(true);
    expect(looksLikeMermaid("graphics are nice")).toBe(false);
    expect(looksLikeMermaid("hello")).toBe(false);
  });
});

describe("keymap", () => {
  const key = (k: string, extra: Partial<KeyEventLike> = {}): KeyEventLike => ({
    key: k,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    altKey: false,
    ...extra,
  });

  it("picks tools by letter, alias and number", () => {
    expect(TOOLS.map((t) => t.key).join("")).toBe("VHRDOALPTFEKM");
    expect(toolForKey("r")).toBe("rect");
    expect(toolForKey("B")).toBe("draw");
    expect(toolForKey("g")).toBeUndefined();
    expect(toolForKey("3")).toBe("rect");
    expect(actionFor(key("o"))).toEqual({ type: "tool", tool: "ellipse" });
  });

  it("maps editing and view shortcuts", () => {
    expect(actionFor(key("z", { ctrlKey: true }))).toEqual({ type: "undo" });
    expect(actionFor(key("Z", { ctrlKey: true, shiftKey: true }))).toEqual({
      type: "redo",
    });
    expect(actionFor(key("}", { ctrlKey: true, shiftKey: true }))).toEqual({
      type: "front",
    });
    expect(actionFor(key("[", { metaKey: true }))).toEqual({ type: "backward" });
    expect(actionFor(key("G", { ctrlKey: true, shiftKey: true }))).toEqual({
      type: "ungroup",
    });
    expect(actionFor(key("!", { shiftKey: true, code: "Digit1" }))).toEqual({
      type: "zoom-fit",
    });
    expect(actionFor(key("ArrowLeft", { shiftKey: true }))).toEqual({
      type: "nudge",
      dx: -10,
      dy: 0,
    });
    expect(actionFor(key("s", { ctrlKey: true }))).toBeUndefined();
    expect(actionFor(key("c", { ctrlKey: true }))).toBeUndefined();
    expect(actionFor(key("r", { altKey: true }))).toBeUndefined();
  });
});
