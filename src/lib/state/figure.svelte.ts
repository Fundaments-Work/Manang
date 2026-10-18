/**
 * Figure editor state: one `FigureState` per open figure file.
 *
 * The figure plugin (`figure.*`) owns the scene, its revision and its undo
 * history. This state keeps a mirror of the scene for drawing, the active
 * tool and the gesture in progress.
 *
 * Edits are optimistic: every `figure.apply` operation addresses nodes by id
 * and carries all its data, so the editor applies it to the mirror at once
 * (`applyEdit`, the same rules the plugin uses) and sends it to the plugin
 * behind, in order. A refused operation, an undo or a redo replaces the mirror
 * with the plugin's scene and replays whatever is still queued on top. A drag
 * moves the node on the mirror only and becomes one `translate` operation on
 * release, so the history gets one step per gesture and the IPC sees one call.
 */

import { applyEdit, translatedScene, type FigureEdit } from "$lib/figure/edit";
import { allAnchors, snapToAnchors, type SnapResult } from "$lib/figure/geometry";
import {
  createScene,
  formatNodeId,
  nodeById,
  type Anchor,
  type FigureNode,
  type FigureScene,
  type StrokeSample,
  type Vec,
} from "$lib/figure/scene";
import { errorMessage, reportError } from "$lib/kernel";
import type { FigureApplyResult, FigureOp, FigureSaved } from "$lib/kernel/api";

import { notice } from "./notice.svelte";

export type PenTool = "select" | "freehand" | "geometry" | "ellipse" | "arrow" | "math";

export const TOOLS: readonly { tool: PenTool; label: string; key: string }[] = [
  { tool: "select", label: "Select", key: "P" },
  { tool: "freehand", label: "Freehand", key: "B" },
  { tool: "geometry", label: "Rectangle", key: "G" },
  { tool: "ellipse", label: "Ellipse", key: "E" },
  { tool: "arrow", label: "Arrow", key: "A" },
  { tool: "math", label: "Formula", key: "M" },
];

/** The plugin side of one open figure (`figure.*` bound to its id). */
export interface FigureBackend {
  apply(op: FigureOp): Promise<FigureApplyResult>;
  get(): Promise<FigureScene>;
  /** The id of the topmost node at `point`, or null. */
  hitTest(point: Vec): Promise<string | null>;
  /** Write `<name>.figure.json` and `<name>.svg` for the project path `path`. */
  save(path: string): Promise<FigureSaved>;
  close(): Promise<void>;
}

export function serializeScene(scene: FigureScene): string {
  return `${JSON.stringify(scene, null, 2)}\n`;
}

/** A select-tool press: the hit test is asynchronous, so the drag waits for it. */
interface Press {
  readonly start: Vec;
  current: Vec;
  released: boolean;
  /** The node under the press, once the plugin answered. */
  id: string | null;
  /** The mirror before the drag started. */
  base: FigureScene | null;
}

export class FigureState {
  path: string;
  readonly #backend: FigureBackend;

  /** The mirror of the plugin's scene, ahead of it by the queued edits. */
  scene = $state.raw<FigureScene>(createScene(960, 540));
  /** The plugin's revision and the revision on disk. */
  revision = $state(0);
  savedRevision = $state(0);
  /** Edits sent to the plugin and not yet acknowledged. */
  pending = $state(0);
  canUndo = $state(false);
  canRedo = $state(false);

  readonly dirty: boolean = $derived(
    this.pending > 0 || this.revision !== this.savedRevision,
  );

  constructor(path: string, scene: FigureScene, backend: FigureBackend) {
    this.path = path;
    this.scene = scene;
    this.#backend = backend;
    this.#counter = highestId(scene);
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

  #press: Press | null = null;
  #pressToken = 0;

  /** Plugin calls run one at a time, in the order they were made. */
  #chain: Promise<unknown> = Promise.resolve();
  /** Edits applied to the mirror whose plugin call has not finished. */
  #inflight: FigureEdit[] = [];

  readonly selected: FigureNode | undefined = $derived(
    this.selectedId === null ? undefined : nodeById(this.scene, this.selectedId),
  );

  readonly nodeCount: number = $derived(this.scene.nodes.length);

  get isDrawing(): boolean {
    return this.draft.length > 0 || this.dragStart !== null || this.#press !== null;
  }

  /* ---------------------------------------------------------------------- */
  /* Plugin round trips                                                      */
  /* ---------------------------------------------------------------------- */

  #run<T>(task: () => Promise<T>): Promise<T> {
    const next = this.#chain.then(task);
    this.#chain = next.catch(() => undefined);
    return next;
  }

  /** Settles once every call made so far has finished. */
  idle(): Promise<void> {
    return this.#run(() => Promise.resolve());
  }

  #ack(result: FigureApplyResult): void {
    this.revision = result.revision;
    this.canUndo = result.can_undo;
    this.canRedo = result.can_redo;
  }

  /** Take the plugin's scene, with the still-queued edits replayed on top. */
  async #resync(): Promise<void> {
    let scene = await this.#backend.get();
    for (const edit of this.#inflight) {
      try {
        scene = applyEdit(scene, edit);
      } catch {
        // That edit will be refused by the plugin too, and resync again.
      }
    }
    this.scene = scene;
    this.#dropMissingSelection();
  }

  /** Apply `edit` now and send it to the plugin. False if it was refused locally. */
  edit(edit: FigureEdit): boolean {
    let next: FigureScene;
    try {
      next = applyEdit(this.scene, edit);
    } catch (error) {
      reportError("figure.edit", error);
      return false;
    }
    this.scene = next;
    this.canUndo = true;
    this.canRedo = false;
    this.#inflight.push(edit);
    this.pending += 1;
    void this.#run(async () => {
      try {
        this.#ack(await this.#backend.apply(edit));
        this.#settle(edit);
      } catch (error) {
        this.#settle(edit);
        reportError("figure.apply", error);
        notice.error(`The figure refused that change: ${errorMessage(error)}`);
        await this.#resync().catch((e: unknown) => reportError("figure.get", e));
      }
    });
    return true;
  }

  #settle(edit: FigureEdit): void {
    const at = this.#inflight.indexOf(edit);
    if (at !== -1) this.#inflight.splice(at, 1);
    this.pending -= 1;
  }

  #history(op: "undo" | "redo"): Promise<void> {
    this.cancel();
    return this.#run(async () => {
      try {
        const result = await this.#backend.apply({ op });
        this.#ack(result);
        if (result.applied) await this.#resync();
      } catch (error) {
        reportError(`figure.${op}`, error);
        notice.error(`Could not ${op}: ${errorMessage(error)}`);
      }
    });
  }

  undo(): Promise<void> {
    return this.#history("undo");
  }

  redo(): Promise<void> {
    return this.#history("redo");
  }

  /** Save after every queued edit; resolves with what was written. */
  save(): Promise<FigureSaved> {
    const path = this.path;
    return this.#run(async () => {
      const saved = await this.#backend.save(path);
      this.savedRevision = saved.revision;
      return saved;
    });
  }

  /** Release the plugin's copy (the tab closed). */
  close(): Promise<void> {
    return this.#run(() => this.#backend.close());
  }

  #dropMissingSelection(): void {
    if (
      this.selectedId !== null &&
      nodeById(this.scene, this.selectedId) === undefined
    ) {
      this.selectedId = null;
    }
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
        this.#press = {
          start: point,
          current: point,
          released: false,
          id: null,
          base: null,
        };
        void this.#pick(point, ++this.#pressToken);
        break;
      case "math":
        this.stampMath(point);
        break;
    }
  }

  /** Ask the plugin what is under the press, after every queued edit. */
  async #pick(point: Vec, token: number): Promise<void> {
    let id: string | null = null;
    try {
      id = await this.#run(() => this.#backend.hitTest(point));
    } catch (error) {
      reportError("figure.hit-test", error);
    }
    const press = this.#press;
    if (token !== this.#pressToken || press === null) return;
    this.selectedId = id;
    if (id === null) {
      if (press.released) this.#press = null;
      return;
    }
    press.id = id;
    press.base = this.scene;
    if (press.released) {
      this.#finishMove();
      return;
    }
    this.#dragTo(press);
  }

  #dragTo(press: Press): void {
    if (press.id === null || press.base === null) return;
    const delta = {
      x: press.current.x - press.start.x,
      y: press.current.y - press.start.y,
    };
    // Live movement is drawn on the mirror only; the release sends one step.
    this.scene = translatedScene(press.base, press.id, delta);
  }

  #finishMove(): void {
    const press = this.#press;
    this.#press = null;
    if (press === null || press.id === null || press.base === null) return;
    const dx = press.current.x - press.start.x;
    const dy = press.current.y - press.start.y;
    this.scene = press.base;
    if (dx !== 0 || dy !== 0) this.edit({ op: "translate", id: press.id, dx, dy });
  }

  /**
   * Extends an in-progress interaction.
   *
   * Sampling is thinned by distance rather than by time so the stroke density
   * is identical on a 60 Hz and a 120 Hz display, which keeps the exported
   * outline deterministic across machines.
   */
  extend(point: Vec, pressure: number): void {
    const press = this.#press;
    if (press !== null) {
      press.current = point;
      this.#dragTo(press);
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
    const press = this.#press;
    if (press !== null) {
      press.released = true;
      // Still waiting for the hit test: it finishes the move when it lands.
      if (press.id !== null) this.#finishMove();
      return;
    }

    if (this.tool === "freehand" && this.draft.length > 1) {
      this.add({
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
        this.add({ kind: "arrow", id: this.nextId(), from, to, color: "#e8e6e1" });
      }

      if (this.tool === "geometry") {
        const width = to.x - from.x;
        const height = to.y - from.y;
        if (Math.abs(width) > 8 && Math.abs(height) > 8) {
          this.add({
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

      if (this.tool === "ellipse") {
        const rx = Math.abs(to.x - from.x) / 2;
        const ry = Math.abs(to.y - from.y) / 2;
        if (rx > 4 && ry > 4) {
          this.add({
            kind: "ellipse",
            id: this.nextId(),
            cx: (from.x + to.x) / 2,
            cy: (from.y + to.y) / 2,
            rx,
            ry,
            stroke: "#e8e6e1",
            fill: "#1c1c20",
          });
        }
      }
    }

    this.cancel();
  }

  cancel(): void {
    if (this.#press !== null) {
      // An abandoned drag snaps back; nothing was sent.
      if (this.#press.base !== null) this.scene = this.#press.base;
      this.#press = null;
      this.#pressToken += 1;
    }
    this.draft = [];
    this.dragStart = null;
    this.dragCurrent = null;
    this.activeSnap = null;
  }

  /* ---------------------------------------------------------------------- */
  /* Mutation                                                               */
  /* ---------------------------------------------------------------------- */

  add(node: FigureNode): boolean {
    return this.edit({ op: "add-node", node });
  }

  /** Places a formula and selects it so its text can be typed straight away. */
  stampMath(point: Vec, tex = "x"): void {
    const id = this.nextId();
    const added = this.add({
      kind: "math",
      id,
      x: point.x,
      y: point.y,
      tex,
      size: 26,
      color: "#b3d17a",
    });
    if (!added) return;
    this.tool = "select";
    this.selectedId = id;
  }

  /** Change a formula's text. Empty text is refused so a formula cannot vanish invisibly. */
  setMathText(id: string, tex: string): void {
    const node = nodeById(this.scene, id);
    if (node?.kind !== "math" || tex.trim() === "" || node.tex === tex) return;
    this.edit({ op: "update-node", node: { ...node, tex } });
  }

  deleteSelected(): void {
    if (this.selectedId === null) return;
    this.edit({ op: "remove-node", id: this.selectedId });
    this.selectedId = null;
  }

  /** Move the selection by a fixed step (arrow keys); one undo step each. */
  nudge(delta: Vec): void {
    if (this.selectedId === null) return;
    this.edit({ op: "translate", id: this.selectedId, dx: delta.x, dy: delta.y });
  }

  clear(): void {
    this.edit({ op: "clear" });
    this.selectedId = null;
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
