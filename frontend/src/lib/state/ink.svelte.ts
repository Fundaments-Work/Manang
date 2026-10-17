/**
 * Vector ink canvas state — DESIGN.md §4.
 *
 * Pen tray tools map one-to-one onto scene node kinds. The store owns the
 * scene, the active tool, and the in-progress stroke; rendering is Svelte's job
 * and persistence belongs to the Rust renderer.
 */

import { allAnchors, hitTest, snapToAnchors, type SnapResult } from "$lib/ink/geometry";
import {
  appendNode,
  createScene,
  formatNodeId,
  nodeById,
  removeNode,
  replaceNode,
  SCENE_VERSION,
  type Anchor,
  type FigureNode,
  type FigureScene,
  type StrokeSample,
  type Vec,
} from "$lib/ink/scene";

export type PenTool = "select" | "freehand" | "geometry" | "arrow" | "math";

export const TOOLS: readonly { tool: PenTool; label: string; glyph: string }[] = [
  { tool: "freehand", label: "Freehand", glyph: "✎" },
  { tool: "geometry", label: "Geometry", glyph: "□" },
  { tool: "arrow", label: "Arrow", glyph: "→" },
  { tool: "math", label: "Math", glyph: "T" },
];

export const FIGURE_PATH = "figures/state_lattice.figure.json";

/** A hand-seeded scene so the canvas is never an empty void on open. */
function seedScene(): FigureScene {
  const base = createScene(960, 540);

  const left: FigureNode = {
    kind: "rect",
    id: formatNodeId(1),
    x: 120,
    y: 200,
    width: 190,
    height: 84,
    stroke: "#e8e6e1",
    fill: "#1c1c20",
  };
  const right: FigureNode = {
    kind: "rect",
    id: formatNodeId(2),
    x: 640,
    y: 200,
    width: 190,
    height: 84,
    stroke: "#e8e6e1",
    fill: "#1c1c20",
  };
  const arrow: FigureNode = {
    kind: "arrow",
    id: formatNodeId(3),
    from: { x: 310, y: 242 },
    to: { x: 640, y: 242 },
    color: "#38bdf8",
    label: "H",
  };
  const math: FigureNode = {
    kind: "math",
    id: formatNodeId(4),
    x: 120,
    y: 120,
    tex: "|0⟩  ⟶  |1⟩",
    size: 28,
    color: "#c6f135",
  };

  return {
    ...base,
    nodes: [math, left, arrow, right],
  };
}

export class InkState {
  scene = $state<FigureScene>(seedScene());

  tool = $state<PenTool>("freehand");

  #counter = 5;

  /** The stroke being drawn right now; committed on pointer up. */
  draft = $state<StrokeSample[]>([]);

  /** Endpoints of the arrow or geometry being dragged. */
  dragStart = $state<Vec | null>(null);
  dragCurrent = $state<Vec | null>(null);

  /** Live snap target while dragging, so the magnet can be drawn. */
  activeSnap = $state<Anchor | null>(null);

  selectedId = $state<string | null>(null);

  readonly selected: FigureNode | undefined = $derived(
    this.selectedId === null ? undefined : nodeById(this.scene, this.selectedId),
  );

  readonly nodeCount: number = $derived(this.scene.nodes.length);

  get isDrawing(): boolean {
    return this.draft.length > 0 || this.dragStart !== null;
  }

  setTool(tool: PenTool): void {
    this.tool = tool;
    this.cancel();
    if (tool !== "select") this.selectedId = null;
  }

  nextId(): string {
    this.#counter += 1;
    return formatNodeId(this.#counter);
  }

  /* ---------------------------------------------------------------------- */
  /* Pointer lifecycle                                                       */
  /* ---------------------------------------------------------------------- */

  beginStroke(point: Vec, pressure: number): void {
    switch (this.tool) {
      case "freehand":
        this.draft = [{ x: point.x, y: point.y, pressure }];
        break;
      case "geometry":
      case "arrow":
        this.dragStart = point;
        this.dragCurrent = point;
        break;
      case "select":
        this.selectedId = hitTest(this.scene, point)?.id ?? null;
        break;
      case "math":
        this.stampMath(point);
        break;
    }
  }

  /**
   * Extends an in-progress interaction.
   *
   * Sampling is thinned by distance rather than by time so the stroke density
   * is identical on a 60 Hz and a 120 Hz display, which keeps the exported
   * outline deterministic across machines.
   */
  extend(point: Vec, pressure: number): void {
    if (this.tool === "freehand") {
      const last = this.draft[this.draft.length - 1];
      if (last !== undefined && Math.hypot(point.x - last.x, point.y - last.y) < 1.5) {
        return;
      }
      this.draft = [...this.draft, { x: point.x, y: point.y, pressure }];
      return;
    }

    if (this.dragStart !== null) {
      const anchors = allAnchors(this.scene);
      const snapped: SnapResult =
        this.tool === "arrow" ? snapToAnchors(point, anchors) : { point };
      this.dragCurrent = snapped.point;
      this.activeSnap = snapped.anchor ?? null;
    }
  }

  /** Commits the in-progress node, if the gesture produced one. */
  endStroke(): void {
    if (this.tool === "freehand" && this.draft.length > 1) {
      this.scene = appendNode(this.scene, {
        kind: "stroke",
        id: this.nextId(),
        points: this.draft,
        size: 2.5,
        color: "#e8e6e1",
      });
    }

    if (this.dragStart !== null && this.dragCurrent !== null) {
      const from = this.dragStart;
      const to = this.dragCurrent;

      if (this.tool === "arrow" && Math.hypot(to.x - from.x, to.y - from.y) > 8) {
        this.scene = appendNode(this.scene, {
          kind: "arrow",
          id: this.nextId(),
          from,
          to,
          color: "#e8e6e1",
        });
      }

      if (this.tool === "geometry") {
        const width = to.x - from.x;
        const height = to.y - from.y;
        if (Math.abs(width) > 8 && Math.abs(height) > 8) {
          this.scene = appendNode(this.scene, {
            kind: "rect",
            id: this.nextId(),
            x: from.x,
            y: from.y,
            width,
            height,
            stroke: "#e8e6e1",
            fill: "#1c1c20",
          });
        }
      }
    }

    this.cancel();
  }

  cancel(): void {
    this.draft = [];
    this.dragStart = null;
    this.dragCurrent = null;
    this.activeSnap = null;
  }

  /* ---------------------------------------------------------------------- */
  /* Mutation                                                               */
  /* ---------------------------------------------------------------------- */

  stampMath(point: Vec): void {
    this.scene = appendNode(this.scene, {
      kind: "math",
      id: this.nextId(),
      x: point.x,
      y: point.y,
      tex: "|ψ⟩",
      size: 26,
      color: "#c6f135",
    });
  }

  deleteSelected(): void {
    if (this.selectedId === null) return;
    this.scene = removeNode(this.scene, this.selectedId);
    this.selectedId = null;
  }

  translate(id: string, delta: Vec): void {
    const node = nodeById(this.scene, id);
    if (node === undefined) return;

    switch (node.kind) {
      case "rect":
        this.scene = replaceNode(this.scene, {
          ...node,
          x: node.x + delta.x,
          y: node.y + delta.y,
        });
        break;
      case "ellipse":
        this.scene = replaceNode(this.scene, {
          ...node,
          cx: node.cx + delta.x,
          cy: node.cy + delta.y,
        });
        break;
      case "arrow":
        this.scene = replaceNode(this.scene, {
          ...node,
          from: { x: node.from.x + delta.x, y: node.from.y + delta.y },
          to: { x: node.to.x + delta.x, y: node.to.y + delta.y },
        });
        break;
      case "math":
        this.scene = replaceNode(this.scene, {
          ...node,
          x: node.x + delta.x,
          y: node.y + delta.y,
        });
        break;
      case "stroke":
        this.scene = replaceNode(this.scene, {
          ...node,
          points: node.points.map((point) => ({
            ...point,
            x: point.x + delta.x,
            y: point.y + delta.y,
          })),
        });
        break;
    }
  }

  clear(): void {
    this.scene = {
      ...createScene(this.scene.width, this.scene.height),
      version: SCENE_VERSION,
    };
    this.selectedId = null;
  }

  /** The embed line the manuscript receives — DESIGN.md §4.1. */
  embedSnippet(): string {
    return `#figure(image("figures/state_lattice.svg", width: 80%))`;
  }
}

export const ink = new InkState();
