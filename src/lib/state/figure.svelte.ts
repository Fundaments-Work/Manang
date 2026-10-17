/**
 * Figure editor state: one `FigureState` per open figure file.
 *
 * Tools map one-to-one onto scene node kinds. The state owns the scene, the
 * active tool and the in-progress stroke; rendering is Svelte's job and the
 * headless SVG render belongs to the Rust figure crate. The scene is plain JSON
 * (`*.figure.json`), so it diffs and commits like any other project file.
 */

import { HISTORY_LIMIT, svgPathFor, translatedScene } from "$lib/figure/edit";
import {
  allAnchors,
  hitTest,
  snapToAnchors,
  type SnapResult,
} from "$lib/figure/geometry";
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
} from "$lib/figure/scene";

export type PenTool = "select" | "freehand" | "geometry" | "ellipse" | "arrow" | "math";

export const TOOLS: readonly { tool: PenTool; label: string; key: string }[] = [
  { tool: "select", label: "Select", key: "P" },
  { tool: "freehand", label: "Freehand", key: "B" },
  { tool: "geometry", label: "Rectangle", key: "G" },
  { tool: "ellipse", label: "Ellipse", key: "E" },
  { tool: "arrow", label: "Arrow", key: "A" },
  { tool: "math", label: "Formula", key: "M" },
];

/** Parse a `.figure.json` file; throws a sentence the user can act on. */
export function parseScene(text: string): FigureScene {
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    throw new Error("This file is not valid JSON, so it cannot be opened as a figure.");
  }
  const scene = raw as Partial<FigureScene> | null;
  if (
    scene === null ||
    typeof scene !== "object" ||
    scene.version !== SCENE_VERSION ||
    typeof scene.width !== "number" ||
    typeof scene.height !== "number" ||
    !Array.isArray(scene.nodes)
  ) {
    throw new Error(
      `This file is not a version ${SCENE_VERSION} Manang figure (expected version, width, height and nodes).`,
    );
  }
  return scene as FigureScene;
}

export function serializeScene(scene: FigureScene): string {
  return `${JSON.stringify(scene, null, 2)}\n`;
}

export class FigureState {
  path: string;
  scene = $state.raw<FigureScene>(createScene(960, 540));
  /** The scene as last saved; `dirty` compares by reference. */
  saved = $state.raw<FigureScene>(this.scene);
  readonly dirty: boolean = $derived(this.scene !== this.saved);

  constructor(path: string, scene?: FigureScene) {
    this.path = path;
    if (scene !== undefined) {
      this.scene = scene;
      this.saved = scene;
    }
    this.#counter = highestId(this.scene);
  }

  /** Record that the current scene is what is on disk. */
  markSaved(scene: FigureScene): void {
    this.saved = scene;
  }

  tool = $state<PenTool>("freehand");

  #counter = 0;

  /** The stroke being drawn right now; committed on pointer up. */
  draft = $state<StrokeSample[]>([]);

  /** Endpoints of the arrow or geometry being dragged. */
  dragStart = $state<Vec | null>(null);
  dragCurrent = $state<Vec | null>(null);

  /** Live snap target while dragging, so the magnet can be drawn. */
  activeSnap = $state<Anchor | null>(null);

  selectedId = $state<string | null>(null);

  /** Scenes before each committed change, newest last. Scenes are immutable, so this is cheap. */
  #undo: FigureScene[] = [];
  #redo: FigureScene[] = [];
  /** Bumped on every history change so `canUndo`/`canRedo` stay reactive. */
  historyVersion = $state(0);

  /** While dragging a selection: the last pointer position and the scene before the drag. */
  #moveLast: Vec | null = null;
  #moveBase: FigureScene | null = null;

  get canUndo(): boolean {
    void this.historyVersion;
    return this.#undo.length > 0;
  }

  get canRedo(): boolean {
    void this.historyVersion;
    return this.#redo.length > 0;
  }

  /** Replace the scene as one undoable step. */
  commit(next: FigureScene, before: FigureScene = this.scene): void {
    if (next === before) return;
    this.#undo.push(before);
    if (this.#undo.length > HISTORY_LIMIT) this.#undo.shift();
    this.#redo = [];
    this.scene = next;
    this.historyVersion += 1;
  }

  undo(): void {
    const previous = this.#undo.pop();
    if (previous === undefined) return;
    this.#redo.push(this.scene);
    this.scene = previous;
    this.#dropMissingSelection();
    this.historyVersion += 1;
  }

  redo(): void {
    const next = this.#redo.pop();
    if (next === undefined) return;
    this.#undo.push(this.scene);
    this.scene = next;
    this.#dropMissingSelection();
    this.historyVersion += 1;
  }

  #dropMissingSelection(): void {
    if (
      this.selectedId !== null &&
      nodeById(this.scene, this.selectedId) === undefined
    ) {
      this.selectedId = null;
    }
  }

  readonly selected: FigureNode | undefined = $derived(
    this.selectedId === null ? undefined : nodeById(this.scene, this.selectedId),
  );

  readonly nodeCount: number = $derived(this.scene.nodes.length);

  get isDrawing(): boolean {
    return this.draft.length > 0 || this.dragStart !== null || this.#moveLast !== null;
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
      case "ellipse":
      case "arrow":
        this.dragStart = point;
        this.dragCurrent = point;
        break;
      case "select":
        this.selectedId = hitTest(this.scene, point)?.id ?? null;
        if (this.selectedId !== null) {
          this.#moveLast = point;
          this.#moveBase = this.scene;
        }
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
    if (this.#moveLast !== null && this.selectedId !== null) {
      const delta = { x: point.x - this.#moveLast.x, y: point.y - this.#moveLast.y };
      this.#moveLast = point;
      // Live movement is not a history step; the whole drag becomes one on release.
      this.scene = translatedScene(this.scene, this.selectedId, delta);
      return;
    }

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
    if (this.#moveBase !== null) {
      this.commit(this.scene, this.#moveBase);
    }

    if (this.tool === "freehand" && this.draft.length > 1) {
      this.commit(
        appendNode(this.scene, {
          kind: "stroke",
          id: this.nextId(),
          points: this.draft,
          size: 2.5,
          color: "#e8e6e1",
        }),
      );
    }

    if (this.dragStart !== null && this.dragCurrent !== null) {
      const from = this.dragStart;
      const to = this.dragCurrent;

      if (this.tool === "arrow" && Math.hypot(to.x - from.x, to.y - from.y) > 8) {
        this.commit(
          appendNode(this.scene, {
            kind: "arrow",
            id: this.nextId(),
            from,
            to,
            color: "#e8e6e1",
          }),
        );
      }

      if (this.tool === "geometry") {
        const width = to.x - from.x;
        const height = to.y - from.y;
        if (Math.abs(width) > 8 && Math.abs(height) > 8) {
          this.commit(
            appendNode(this.scene, {
              kind: "rect",
              id: this.nextId(),
              x: from.x,
              y: from.y,
              width,
              height,
              stroke: "#e8e6e1",
              fill: "#1c1c20",
            }),
          );
        }
      }

      if (this.tool === "ellipse") {
        const rx = Math.abs(to.x - from.x) / 2;
        const ry = Math.abs(to.y - from.y) / 2;
        if (rx > 4 && ry > 4) {
          this.commit(
            appendNode(this.scene, {
              kind: "ellipse",
              id: this.nextId(),
              cx: (from.x + to.x) / 2,
              cy: (from.y + to.y) / 2,
              rx,
              ry,
              stroke: "#e8e6e1",
              fill: "#1c1c20",
            }),
          );
        }
      }
    }

    this.cancel();
  }

  cancel(): void {
    this.#moveLast = null;
    this.#moveBase = null;
    this.draft = [];
    this.dragStart = null;
    this.dragCurrent = null;
    this.activeSnap = null;
  }

  /* ---------------------------------------------------------------------- */
  /* Mutation                                                               */
  /* ---------------------------------------------------------------------- */

  /** Places a formula and selects it so its text can be typed straight away. */
  stampMath(point: Vec, tex = "x"): void {
    const id = this.nextId();
    this.commit(
      appendNode(this.scene, {
        kind: "math",
        id,
        x: point.x,
        y: point.y,
        tex,
        size: 26,
        color: "#b3d17a",
      }),
    );
    this.tool = "select";
    this.selectedId = id;
  }

  /** Change a formula's text. Empty text is refused so a formula cannot vanish invisibly. */
  setMathText(id: string, tex: string): void {
    const node = nodeById(this.scene, id);
    if (node?.kind !== "math" || tex.trim() === "" || node.tex === tex) return;
    this.commit(replaceNode(this.scene, { ...node, tex }));
  }

  deleteSelected(): void {
    if (this.selectedId === null) return;
    this.commit(removeNode(this.scene, this.selectedId));
    this.selectedId = null;
  }

  /** Move the selection by a fixed step (arrow keys); one undo step each. */
  nudge(delta: Vec): void {
    if (this.selectedId === null) return;
    this.commit(translatedScene(this.scene, this.selectedId, delta));
  }

  translate(id: string, delta: Vec): void {
    this.commit(translatedScene(this.scene, id, delta));
  }

  clear(): void {
    this.commit({
      ...createScene(this.scene.width, this.scene.height),
      version: SCENE_VERSION,
    });
    this.selectedId = null;
  }

  /** The line that embeds this figure's rendered SVG in a Typst document. */
  embedSnippet(): string {
    return `#figure(image("${svgPathFor(this.path)}", width: 80%))`;
  }
}

/** Highest numeric suffix among node ids, so new ids never collide with loaded ones. */
function highestId(scene: FigureScene): number {
  let highest = 0;
  for (const node of scene.nodes) {
    const parsed = Number.parseInt(node.id.slice(1), 36);
    if (Number.isFinite(parsed)) highest = Math.max(highest, parsed);
  }
  return highest;
}

/** A blank 960 by 540 figure, the size the starter scenes use. */
export function blankFigure(): string {
  return serializeScene(createScene(960, 540));
}
