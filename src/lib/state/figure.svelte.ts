/**
 * Figure editor state: one `FigureState` per open figure file (ADR 0012).
 *
 * The figure plugin (`figure.*`) owns the scene, its revision and its undo
 * history. This state keeps a mirror of the scene for drawing, the view
 * (pan and zoom, never saved), the selection and the gesture in progress.
 *
 * Edits are optimistic: every `figure.apply` operation addresses nodes by id
 * and carries all its data, so the editor applies it to the mirror at once
 * (`applyOp`, the mirror of the plugin's rules) and sends it to the plugin
 * behind, in order. A refused operation, an undo or a redo replaces the mirror
 * with the plugin's scene and replays whatever is still queued on top.
 *
 * Gestures (move, resize, rotate, arrow ends) change a local preview only;
 * pointer-up sends one operation, so the history gets one step per gesture
 * and pointer moves never cross the IPC boundary.
 */

import { nodeRect, normalizeRect } from "$lib/figure/geometry";
import { strokeColour } from "$lib/figure/palette";
import { scaleSelection } from "$lib/figure/scale";
import { applyEdit, type FigureEdit } from "$lib/figure/edit";
import { route } from "$lib/figure/binding";
import {
  MAX_IMAGE_BYTES,
  PASTE_OFFSET,
  looksLikeMermaid,
  parseNodes,
  reidentify,
  serializeNodes,
} from "$lib/figure/clipboard";
import {
  handleAt,
  handleEdges,
  handlePositions,
  resizeBox,
  rotationFor,
  type Handle,
  type ResizeHandle,
} from "$lib/figure/handles";
import { actionFor, type KeyEventLike, type Tool } from "$lib/figure/keymap";
import {
  applyOp,
  nextGroupId,
  type AlignEdge,
  type Axis,
  type ZOrder,
} from "$lib/figure/ops";
import { naturalSize } from "$lib/figure/text";
import {
  FORMULA_SIZE,
  createScene,
  formatNodeId,
  highestCounter,
  isBindable,
  isConnector,
  isShape,
  languageOf,
  nodeById,
  styleDefaults,
  type ConnectorNode,
  type FigureNode,
  type FigureScene,
  type MathLanguage,
  type Rect,
  type ShapeNode,
  type StrokeSample,
  type Style,
  type Vec,
} from "$lib/figure/scene";
import {
  expandGroups,
  idSet,
  marqueeSelect,
  pickAt,
  planDrag,
  previewMove,
  previewReplace,
  selectAll,
  selectionBounds,
  toggleSelection,
  type DragPlan,
} from "$lib/figure/selection";
import { SNAP_PX, snapMove, snapResize, type Guide } from "$lib/figure/snapping";
import {
  cachedBounds,
  fitRect,
  pan,
  revealRect,
  stepZoom,
  toScene,
  wheelZoomFactor,
  zoomAt,
  type Viewport,
} from "$lib/figure/viewport";
import { errorMessage, reportError } from "$lib/kernel";
import type { FigureApplyResult, FigureOp, FigureSaved } from "$lib/kernel/api";

import { layout } from "./layout.svelte";
import { mathRenders } from "./mathRenders.svelte";
import { notice } from "./notice.svelte";
import { workspace } from "./workspace.svelte";

export type { Tool };

/** What `figure.from-mermaid` answers. */
export interface MermaidResult {
  readonly ids: readonly string[];
  readonly mode?: "shapes" | "block";
  readonly figure?: { readonly revision?: number };
  readonly fallback?: string | null;
}

/** The plugin side of one open figure (`figure.*` bound to its id). */
export interface FigureBackend {
  apply(op: FigureOp): Promise<FigureApplyResult>;
  get(): Promise<FigureScene>;
  /** Write `<name>.figure.json` and `<name>.svg` for the project path `path`. */
  save(path: string): Promise<FigureSaved>;
  close(): Promise<void>;
  /** The scene as SVG markup (`figure.export-svg`). */
  exportSvg(): Promise<string>;
  /** Mermaid text to native shapes or a block (`figure.from-mermaid`). */
  fromMermaid(args: {
    source: string;
    x: number;
    y: number;
    mode: "shapes" | "block";
  }): Promise<MermaidResult>;
}

export function serializeScene(scene: FigureScene): string {
  return `${JSON.stringify(scene, null, 2)}\n`;
}

/** Pointer details the state reads (the component passes them in). */
export interface PointerInfo {
  /** Scene coordinates. */
  readonly point: Vec;
  /** Canvas-relative screen coordinates. */
  readonly screen: Vec;
  readonly button: number;
  readonly shift: boolean;
  readonly alt: boolean;
  readonly mod: boolean;
  readonly pressure: number;
}

type CreateTool = "rect" | "diamond" | "ellipse" | "frame" | "arrow" | "line";

type Gesture =
  | { readonly kind: "pan"; readonly start: Vec; readonly view: Viewport }
  | {
      readonly kind: "create";
      readonly tool: CreateTool;
      readonly start: Vec;
      current: Vec;
    }
  | { readonly kind: "draw"; points: StrokeSample[] }
  | {
      readonly kind: "move";
      readonly start: Vec;
      readonly ids: readonly string[];
      readonly plan: DragPlan;
      readonly startBounds: Rect;
      readonly others: readonly Rect[];
      delta: Vec;
    }
  | {
      readonly kind: "resize";
      readonly id: string;
      readonly handle: ResizeHandle;
      readonly node: ShapeNode;
      readonly plan: DragPlan;
      readonly others: readonly Rect[];
      box: Rect;
    }
  | {
      /** Scaling a multi-selection (or a formula) by its bounding-box handles. */
      readonly kind: "scale";
      readonly handle: ResizeHandle;
      readonly ids: readonly string[];
      readonly from: Rect;
      /** Formulas always scale proportionally. */
      readonly proportional: boolean;
      box: Rect;
    }
  | {
      readonly kind: "rotate";
      readonly id: string;
      readonly node: ShapeNode;
      readonly plan: DragPlan;
      angle: number;
    }
  | {
      readonly kind: "marquee";
      readonly start: Vec;
      current: Vec;
      readonly additive: boolean;
    }
  | {
      readonly kind: "endpoint";
      readonly id: string;
      readonly end: "start" | "end";
      readonly node: ConnectorNode;
      point: Vec;
      target: string | null;
    }
  | { readonly kind: "erase"; readonly ids: ReadonlySet<string> };

/** An inline label editor over a shape, or over the point a text will go. */
export interface LabelEdit {
  /** The node edited, or null for a new text node. */
  readonly id: string | null;
  readonly kind: string;
  /** Scene box the editor covers. */
  readonly box: Rect;
  readonly angle: number;
  readonly text: string;
}

/** The formula editor: an inline panel over a formula node. */
export interface FormulaEdit {
  readonly id: string;
  readonly source: string;
  readonly language: MathLanguage;
  /** Just stamped: an empty commit removes it instead of keeping it. */
  readonly fresh: boolean;
}

/** What the canvas draws for a formula while it is typed into. */
export interface FormulaShown {
  readonly id: string;
  readonly source: string;
  readonly language: MathLanguage;
}

const PANEL_KEY = "manang.figure.properties";

function readPanelOpen(): boolean {
  try {
    return localStorage.getItem(PANEL_KEY) !== "closed";
  } catch {
    return true;
  }
}

/** Milliseconds typing pauses before the canvas re-renders the formula. */
export const FORMULA_DEBOUNCE_MS = 150;

/** Pixels (screen) a press must travel before it counts as a drag. */
const DRAG_SLOP = 3;
/** Screen size of a handle's hit area. */
export const HANDLE_PX = 8;

function shapeOf(node: FigureNode | undefined): ShapeNode | undefined {
  return node !== undefined && isShape(node) ? node : undefined;
}

export class FigureState {
  path: string;
  /** The plugin's id of this figure (for `figure.changed` events). */
  readonly id: number;
  readonly #backend: FigureBackend;

  /** The mirror of the plugin's scene, ahead of it by the queued edits. */
  scene = $state.raw<FigureScene>(createScene(960, 540));
  /** The scene with the gesture in progress applied, or null. */
  preview = $state.raw<FigureScene | null>(null);
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

  tool = $state<Tool>("select");
  /** Selected node ids, in selection order. */
  selection = $state.raw<readonly string[]>([]);
  view = $state.raw<Viewport>({ x: -40, y: -40, zoom: 1 });
  /** Canvas size in screen pixels. */
  width = $state(0);
  height = $state(0);
  /** Space held: drags pan. */
  spaceHeld = $state(false);

  gesture = $state.raw<Gesture | null>(null);
  guides = $state.raw<readonly Guide[]>([]);
  /** Freehand stroke being drawn. */
  draft = $state.raw<readonly StrokeSample[]>([]);
  /** Laser trail (scene points with a timestamp), drawn and faded locally. */
  laser = $state.raw<readonly { x: number; y: number; t: number }[]>([]);
  /** The inline label editor, when open. */
  editing = $state.raw<LabelEdit | null>(null);
  /** The formula editor, when open. */
  formula = $state.raw<FormulaEdit | null>(null);
  /** The formula as the canvas shows it while the editor is open (debounced). */
  formulaShown = $state.raw<FormulaShown | null>(null);
  /** The properties column: open, or collapsed to its toggle (remembered per user). */
  panelOpen = $state(readPanelOpen());
  /** Pasted text that looks like Mermaid, waiting for a choice. */
  mermaidPrompt = $state<string | null>(null);
  /** The Insert Mermaid dialog. */
  mermaidDialog = $state(false);

  #counter = 0;
  #formulaTimer: ReturnType<typeof setTimeout> | undefined;
  #press: { readonly screen: Vec; readonly point: Vec; moved: boolean } | null = null;
  #fitted = false;

  constructor(path: string, scene: FigureScene, backend: FigureBackend, id = 0) {
    this.path = path;
    this.id = id;
    this.scene = scene;
    this.#backend = backend;
    this.#counter = highestCounter(scene);
  }

  /** What the canvas draws: the preview during a gesture, else the mirror. */
  readonly display: FigureScene = $derived(this.preview ?? this.scene);
  readonly nodeCount: number = $derived(this.scene.nodes.length);
  readonly selectedNodes: readonly FigureNode[] = $derived.by(() => {
    const set = idSet(this.selection);
    return this.display.nodes.filter((n) => set.has(n.id));
  });
  /** The one selected node, if exactly one is selected. */
  readonly selected: FigureNode | undefined = $derived(
    this.selectedNodes.length === 1 ? this.selectedNodes[0] : undefined,
  );

  get isDrawing(): boolean {
    return this.gesture !== null || this.draft.length > 0;
  }

  /* ---------------------------------------------------------------------- */
  /* Plugin round trips                                                      */
  /* ---------------------------------------------------------------------- */

  #chain: Promise<unknown> = Promise.resolve();
  #inflight: FigureEdit[] = [];

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
    this.#counter = Math.max(this.#counter, highestCounter(scene));
    this.#dropMissingSelection();
  }

  /** Apply `edit` now and send it to the plugin. False if it was refused locally. */
  edit(edit: FigureEdit): boolean {
    let next: FigureScene;
    try {
      next = applyEdit(this.scene, edit);
    } catch (error) {
      reportError("figure.edit", error);
      notice.error(`That change is not possible: ${errorMessage(error)}`);
      return false;
    }
    if (next === this.scene) return true;
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

  /** The current scene as SVG, after every queued edit. */
  exportSvg(): Promise<string> {
    return this.#run(() => this.#backend.exportSvg());
  }

  /** Release the plugin's copy (the tab closed). */
  close(): Promise<void> {
    return this.#run(() => this.#backend.close());
  }

  /** Another plugin or an agent changed this figure: follow when it is newer. */
  externalChange(revision: number): Promise<void> {
    return this.#run(async () => {
      if (revision <= this.revision) return;
      this.revision = revision;
      this.canUndo = true;
      this.canRedo = false;
      await this.#resync().catch((e: unknown) => reportError("figure.get", e));
    });
  }

  #dropMissingSelection(): void {
    const ids = this.selection.filter((id) => nodeById(this.scene, id) !== undefined);
    if (ids.length !== this.selection.length) this.selection = ids;
  }

  nextId(): string {
    this.#counter += 1;
    return formatNodeId(this.#counter);
  }

  /* ---------------------------------------------------------------------- */
  /* Tools, view and selection                                              */
  /* ---------------------------------------------------------------------- */

  setTool(tool: Tool): void {
    this.cancel();
    this.tool = tool;
    if (tool !== "select") this.selection = [];
  }

  select(ids: readonly string[]): void {
    this.selection = [...ids];
  }

  setSize(width: number, height: number): void {
    this.width = width;
    this.height = height;
    if (!this.#fitted && width > 0 && height > 0) {
      this.#fitted = true;
      this.zoomToFit();
    }
  }

  get zoomPercent(): number {
    return Math.round(this.view.zoom * 100);
  }

  /** The scene point at the centre of the canvas. */
  get centre(): Vec {
    return toScene(this.view, { x: this.width / 2, y: this.height / 2 });
  }

  #contentRect(): Rect {
    const page = { x: 0, y: 0, width: this.scene.width, height: this.scene.height };
    const all = selectionBounds(
      this.scene,
      this.scene.nodes.map((n) => n.id),
    );
    if (all.width === 0 && all.height === 0) return page;
    const x = Math.min(page.x, all.x);
    const y = Math.min(page.y, all.y);
    return {
      x,
      y,
      width: Math.max(page.x + page.width, all.x + all.width) - x,
      height: Math.max(page.y + page.height, all.y + all.height) - y,
    };
  }

  zoomToFit(): void {
    if (this.width <= 0) return;
    this.view = fitRect(this.#contentRect(), this.width, this.height, 32, 1);
  }

  zoomToSelection(): void {
    if (this.selection.length === 0 || this.width <= 0) return;
    this.view = fitRect(
      selectionBounds(this.scene, this.selection),
      this.width,
      this.height,
      64,
      4,
    );
  }

  zoomBy(direction: 1 | -1): void {
    this.view = zoomAt(this.view, stepZoom(this.view.zoom, direction), {
      x: this.width / 2,
      y: this.height / 2,
    });
  }

  zoomReset(): void {
    this.view = zoomAt(this.view, 1, { x: this.width / 2, y: this.height / 2 });
  }

  /** Wheel: Ctrl/Cmd (and trackpad pinch) zooms around the cursor, else pans. */
  wheel(screen: Vec, dx: number, dy: number, zoom: boolean): void {
    if (zoom) {
      this.view = zoomAt(this.view, this.view.zoom * wheelZoomFactor(dy), screen);
    } else {
      this.view = pan(this.view, -dx, -dy);
    }
  }

  /* ---------------------------------------------------------------------- */
  /* Pointer lifecycle                                                       */
  /* ---------------------------------------------------------------------- */

  /** The handle of the single selection under `screen`, if any. */
  handleUnder(point: Vec): Handle | "start" | "end" | undefined {
    const tol = HANDLE_PX / this.view.zoom;
    if (this.selection.length > 1) {
      const nodes = this.selectedNodes;
      if (nodes.every((n) => n.locked === true)) return undefined;
      const h = handleAt(
        handlePositions(selectionBounds(this.scene, this.selection), 0, this.view.zoom),
        point,
        tol,
      );
      return h === "rotate" ? undefined : h;
    }
    const node = this.selected;
    if (node === undefined || node.locked === true) return undefined;
    if (node.kind === "math") {
      return handleAt(handlePositions(nodeRect(node), 0, this.view.zoom), point, tol);
    }
    if (isConnector(node)) {
      const first = node.points[0];
      const last = node.points[node.points.length - 1];
      if (
        first !== undefined &&
        Math.hypot(first.x - point.x, first.y - point.y) <= tol
      )
        return "start";
      if (last !== undefined && Math.hypot(last.x - point.x, last.y - point.y) <= tol)
        return "end";
      return undefined;
    }
    if (!isShape(node)) return undefined;
    const r = normalizeRect(node.x, node.y, node.width, node.height);
    return handleAt(handlePositions(r, node.angle ?? 0, this.view.zoom), point, tol);
  }

  #others(exclude: ReadonlySet<string>): Rect[] {
    return this.scene.nodes
      .filter((n) => !exclude.has(n.id) && !isConnector(n))
      .map((n) => cachedBounds(n));
  }

  /** The topmost bindable shape under `point`, other than `except`. */
  bindTarget(point: Vec, except?: string): FigureNode | undefined {
    for (let i = this.scene.nodes.length - 1; i >= 0; i -= 1) {
      const n = this.scene.nodes[i];
      if (n === undefined || n.id === except || !isBindable(n)) continue;
      if (
        pickAt({ ...this.scene, nodes: [n] }, point, 4 / this.view.zoom) !== undefined
      )
        return n;
    }
    return undefined;
  }

  pointerDown(p: PointerInfo): void {
    this.mermaidPrompt = null;
    if (this.editing !== null) this.commitLabel();
    if (this.formula !== null) this.commitFormula();
    this.#press = { screen: p.screen, point: p.point, moved: false };
    if (p.button === 1 || this.tool === "hand" || (this.spaceHeld && p.button === 0)) {
      this.gesture = { kind: "pan", start: p.screen, view: this.view };
      return;
    }
    if (p.button !== 0) return;
    switch (this.tool) {
      case "select":
        this.#pressSelect(p);
        return;
      case "rect":
      case "diamond":
      case "ellipse":
      case "frame":
      case "arrow":
      case "line":
        this.gesture = {
          kind: "create",
          tool: this.tool,
          start: p.point,
          current: p.point,
        };
        return;
      case "draw":
        this.draft = [{ x: p.point.x, y: p.point.y, pressure: p.pressure }];
        this.gesture = { kind: "draw", points: [...this.draft] };
        return;
      case "eraser":
        this.gesture = { kind: "erase", ids: idSet([]) };
        this.#erase(p.point);
        return;
      case "laser":
        this.laser = [
          ...this.laser,
          { x: p.point.x, y: p.point.y, t: performance.now() },
        ];
        return;
      case "text":
        this.startText(p.point);
        return;
      case "math":
        this.stampMath(p.point);
        return;
    }
  }

  #pressSelect(p: PointerInfo): void {
    const handle = this.handleUnder(p.point);
    const single = this.selected;
    if (handle !== undefined && handle !== "start" && handle !== "end") {
      if (this.selection.length > 1 || single?.kind === "math") {
        const ids = this.selection.filter(
          (id) => nodeById(this.scene, id)?.locked !== true,
        );
        const from =
          single?.kind === "math" ? nodeRect(single) : selectionBounds(this.scene, ids);
        if (handle !== "rotate") {
          this.gesture = {
            kind: "scale",
            handle,
            ids,
            from,
            proportional: single?.kind === "math",
            box: from,
          };
        }
        return;
      }
    }
    if (handle !== undefined && single !== undefined) {
      if ((handle === "start" || handle === "end") && isConnector(single)) {
        this.gesture = {
          kind: "endpoint",
          id: single.id,
          end: handle,
          node: single,
          point: p.point,
          target: null,
        };
        return;
      }
      const shape = shapeOf(single);
      if (shape !== undefined) {
        const plan = planDrag(this.scene, [shape.id]);
        if (handle === "rotate") {
          this.gesture = {
            kind: "rotate",
            id: shape.id,
            node: shape,
            plan,
            angle: shape.angle ?? 0,
          };
        } else if (handle !== "start" && handle !== "end") {
          this.gesture = {
            kind: "resize",
            id: shape.id,
            handle,
            node: shape,
            plan,
            others: this.#others(idSet([shape.id])),
            box: normalizeRect(shape.x, shape.y, shape.width, shape.height),
          };
        }
        return;
      }
    }
    const hit = pickAt(this.scene, p.point, 6 / this.view.zoom);
    if (hit === undefined) {
      this.gesture = {
        kind: "marquee",
        start: p.point,
        current: p.point,
        additive: p.shift,
      };
      if (!p.shift) this.selection = [];
      return;
    }
    const ids = expandGroups(this.scene, [hit.id]);
    if (p.shift) {
      this.selection = toggleSelection(this.selection, ids);
      return;
    }
    if (!ids.every((id) => this.selection.includes(id))) this.selection = ids;
    const moving = this.selection.filter(
      (id) => nodeById(this.scene, id)?.locked !== true,
    );
    const set = idSet(moving);
    this.gesture = {
      kind: "move",
      start: p.point,
      ids: moving,
      plan: planDrag(this.scene, moving),
      startBounds: selectionBounds(this.scene, moving),
      others: this.#others(set),
      delta: { x: 0, y: 0 },
    };
  }

  pointerMove(p: PointerInfo): void {
    const press = this.#press;
    if (press !== null && !press.moved) {
      press.moved =
        Math.hypot(p.screen.x - press.screen.x, p.screen.y - press.screen.y) >
        DRAG_SLOP;
    }
    if (
      this.tool === "laser" &&
      (press !== null || this.laser.length > 0) &&
      this.gesture === null
    ) {
      if (press !== null) {
        const now = performance.now();
        this.laser = [
          ...this.laser.filter((q) => now - q.t < 700),
          { x: p.point.x, y: p.point.y, t: now },
        ];
      }
      return;
    }
    const g = this.gesture;
    if (g === null) return;
    switch (g.kind) {
      case "pan":
        this.view = pan(g.view, p.screen.x - g.start.x, p.screen.y - g.start.y);
        return;
      case "create":
        this.gesture = { ...g, current: p.point };
        return;
      case "draw": {
        const last = g.points[g.points.length - 1];
        if (
          last !== undefined &&
          Math.hypot(p.point.x - last.x, p.point.y - last.y) < 1.5 / this.view.zoom
        ) {
          return;
        }
        g.points.push({ x: p.point.x, y: p.point.y, pressure: p.pressure });
        this.draft = [...g.points];
        return;
      }
      case "erase":
        this.#erase(p.point);
        return;
      case "marquee":
        this.gesture = { ...g, current: p.point };
        return;
      case "move": {
        if (press !== null && !press.moved) return;
        let delta = { x: p.point.x - g.start.x, y: p.point.y - g.start.y };
        this.guides = [];
        if (!p.alt && g.ids.length > 0) {
          const moved = {
            ...g.startBounds,
            x: g.startBounds.x + delta.x,
            y: g.startBounds.y + delta.y,
          };
          const snap = snapMove(moved, g.others, SNAP_PX / this.view.zoom);
          delta = { x: delta.x + snap.dx, y: delta.y + snap.dy };
          this.guides = snap.guides;
        }
        this.gesture = { ...g, delta };
        this.preview = previewMove(this.scene, g.plan, delta);
        return;
      }
      case "resize": {
        let box = resizeBox(
          normalizeRect(g.node.x, g.node.y, g.node.width, g.node.height),
          g.node.angle ?? 0,
          g.handle,
          p.point,
          p.shift,
        );
        this.guides = [];
        if (!p.alt && (g.node.angle ?? 0) === 0 && !p.shift) {
          const snapped = snapResize(
            box,
            handleEdges(g.handle),
            g.others,
            SNAP_PX / this.view.zoom,
          );
          box = snapped.box;
          this.guides = snapped.guides;
        }
        this.gesture = { ...g, box };
        this.preview = previewReplace(this.scene, g.plan, [{ ...g.node, ...box }]);
        return;
      }
      case "scale": {
        const box = resizeBox(g.from, 0, g.handle, p.point, p.shift || g.proportional);
        this.gesture = { ...g, box };
        const op = scaleSelection(this.scene, g.ids, g.from, box);
        this.preview = op === undefined ? null : applyOp(this.scene, op).scene;
        return;
      }
      case "rotate": {
        const c = { x: g.node.x + g.node.width / 2, y: g.node.y + g.node.height / 2 };
        const angle = rotationFor(c, p.point, p.shift);
        this.gesture = { ...g, angle };
        this.preview = previewReplace(this.scene, g.plan, [{ ...g.node, angle }]);
        return;
      }
      case "endpoint": {
        const target = this.bindTarget(p.point, g.id) ?? null;
        this.gesture = { ...g, point: p.point, target: target?.id ?? null };
        this.preview = this.#endpointPreview(g.node, g.end, p.point, target);
        return;
      }
    }
  }

  #endpointPreview(
    c: ConnectorNode,
    end: "start" | "end",
    point: Vec,
    target: FigureNode | null | undefined,
  ): FigureScene {
    const points = [...c.points];
    points[end === "start" ? 0 : points.length - 1] = point;
    let next: ConnectorNode = { ...c, points };
    const binding =
      target === null || target === undefined ? undefined : { node: target.id };
    if (end === "start") {
      const { start: _drop, ...rest } = next;
      next = binding === undefined ? rest : { ...rest, start: binding };
    } else {
      const { end: _drop, ...rest } = next;
      next = binding === undefined ? rest : { ...rest, end: binding };
    }
    next = { ...next, points: route(this.scene, next) };
    return {
      ...this.scene,
      nodes: this.scene.nodes.map((n) => (n.id === c.id ? next : n)),
    };
  }

  #erase(point: Vec): void {
    const g = this.gesture;
    if (g?.kind !== "erase") return;
    const hit = pickAt(this.scene, point, 6 / this.view.zoom);
    if (hit !== undefined && !g.ids.has(hit.id)) {
      this.gesture = { ...g, ids: idSet([...g.ids, hit.id]) };
    }
  }

  pointerUp(p: PointerInfo): void {
    const press = this.#press;
    this.#press = null;
    const g = this.gesture;
    this.gesture = null;
    this.guides = [];
    const preview = this.preview;
    this.preview = null;
    if (g === null) return;
    const clicked = press !== null && !press.moved;
    switch (g.kind) {
      case "pan":
        return;
      case "create":
        this.#finishCreate(g.tool, g.start, p.point, clicked);
        return;
      case "draw":
        this.draft = [];
        if (g.points.length > 1) {
          this.add(
            [{ kind: "freedraw", id: this.nextId(), points: g.points, size: 2.5 }],
            false,
          );
        }
        return;
      case "erase":
        if (g.ids.size > 0) this.edit({ op: "remove", ids: [...g.ids] });
        this.selection = this.selection.filter((id) => !g.ids.has(id));
        return;
      case "marquee": {
        const area = normalizeRect(
          g.start.x,
          g.start.y,
          g.current.x - g.start.x,
          g.current.y - g.start.y,
        );
        if (area.width < 1 && area.height < 1) return;
        const ids = marqueeSelect(this.scene, area);
        this.selection = g.additive ? toggleSelection(this.selection, ids) : ids;
        return;
      }
      case "move":
        if (preview !== null && (g.delta.x !== 0 || g.delta.y !== 0)) {
          this.edit({ op: "move", ids: [...g.ids], dx: g.delta.x, dy: g.delta.y });
        }
        return;
      case "resize": {
        const b = g.box;
        const same =
          b.x === g.node.x &&
          b.y === g.node.y &&
          b.width === g.node.width &&
          b.height === g.node.height;
        if (!same)
          this.edit({
            op: "resize",
            id: g.id,
            x: b.x,
            y: b.y,
            width: b.width,
            height: b.height,
          });
        return;
      }
      case "scale": {
        const op = scaleSelection(this.scene, g.ids, g.from, g.box);
        if (op !== undefined) this.edit(op);
        return;
      }
      case "rotate":
        if (g.angle !== (g.node.angle ?? 0))
          this.edit({ op: "rotate", ids: [g.id], angle: g.angle });
        return;
      case "endpoint": {
        if (clicked) return;
        const points = [...g.node.points];
        points[g.end === "start" ? 0 : points.length - 1] = g.point;
        this.edit({
          op: "batch",
          ops: [
            { op: "update", id: g.id, set: { points } },
            { op: "bind", id: g.id, end: g.end, node: g.target },
          ],
        });
        return;
      }
    }
  }

  #finishCreate(tool: CreateTool, from: Vec, to: Vec, clicked: boolean): void {
    if (tool === "arrow" || tool === "line") {
      if (Math.hypot(to.x - from.x, to.y - from.y) * this.view.zoom < 8) return;
      const start = this.bindTarget(from);
      const end = this.bindTarget(to);
      this.add([
        {
          kind: tool,
          id: this.nextId(),
          points: [from, to],
          ...(start === undefined ? {} : { start: { node: start.id } }),
          ...(end === undefined || end.id === start?.id
            ? {}
            : { end: { node: end.id } }),
        },
      ]);
      return;
    }
    let box = normalizeRect(from.x, from.y, to.x - from.x, to.y - from.y);
    if (clicked || box.width < 4 || box.height < 4) {
      // A click places a default-sized shape centred on it.
      const [w, h] =
        tool === "frame" ? [320, 200] : tool === "diamond" ? [140, 100] : [140, 70];
      box = { x: from.x - w / 2, y: from.y - h / 2, width: w, height: h };
    }
    if (tool === "frame") {
      const n = this.scene.nodes.filter((x) => x.kind === "frame").length + 1;
      this.add([{ kind: "frame", id: this.nextId(), ...box, label: `Frame ${n}` }]);
      return;
    }
    this.add([{ kind: tool, id: this.nextId(), ...box, style: { fill: "surface" } }]);
  }

  /** The laser trail without points older than `maxAge` ms. */
  fadeLaser(now: number, maxAge = 700): void {
    if (this.laser.length === 0) return;
    const kept = this.laser.filter((q) => now - q.t < maxAge);
    if (kept.length !== this.laser.length) this.laser = kept;
  }

  doubleClick(point: Vec): void {
    const hit = pickAt(this.scene, point, 6 / this.view.zoom);
    if (hit === undefined) {
      if (this.tool === "select" || this.tool === "text") this.startText(point);
      return;
    }
    this.selection = [hit.id];
    this.startLabelEdit(hit.id);
  }

  /** Cancel the gesture in progress; nothing is sent. */
  cancel(): void {
    this.gesture = null;
    this.preview = null;
    this.guides = [];
    this.draft = [];
    this.#press = null;
  }

  /* ---------------------------------------------------------------------- */
  /* Labels and text                                                        */
  /* ---------------------------------------------------------------------- */

  startLabelEdit(id: string): void {
    const node = nodeById(this.scene, id);
    if (node === undefined || node.locked === true) return;
    if (node.kind === "math") {
      this.openFormula(id, false);
      return;
    }
    if (isConnector(node)) {
      const mid = node.points[Math.floor(node.points.length / 2)] ?? { x: 0, y: 0 };
      this.editing = {
        id,
        kind: node.kind,
        box: { x: mid.x - 80, y: mid.y - 14, width: 160, height: 28 },
        angle: 0,
        text: node.label ?? "",
      };
      return;
    }
    if (!isShape(node) || node.kind === "image") return;
    if (node.kind === "mermaid") return;
    this.editing = {
      id,
      kind: node.kind,
      box: normalizeRect(node.x, node.y, node.width, node.height),
      angle: node.angle ?? 0,
      text: node.label ?? "",
    };
  }

  /** Opens the editor for a new text node at `point`. */
  startText(point: Vec): void {
    this.editing = {
      id: null,
      kind: "text",
      box: { x: point.x, y: point.y - 10, width: 200, height: 28 },
      angle: 0,
      text: "",
    };
  }

  setEditingText(text: string): void {
    if (this.editing !== null) this.editing = { ...this.editing, text };
  }

  /** Commit the label editor: a label update, a new text node, or nothing. */
  commitLabel(): void {
    const e = this.editing;
    this.editing = null;
    if (e === null) return;
    const text = e.text;
    if (e.id === null) {
      if (text.trim() === "") return;
      const size = naturalSize(text, 16, "sans");
      this.add([
        {
          kind: "text",
          id: this.nextId(),
          x: e.box.x,
          y: e.box.y,
          width: Math.ceil(size.width) + 4,
          height: Math.ceil(size.height),
          label: text,
        },
      ]);
      return;
    }
    const node = nodeById(this.scene, e.id);
    if (node === undefined) return;
    if (node.kind === "math") return;
    if (node.kind === "text") {
      if (text.trim() === "") {
        this.edit({ op: "remove", ids: [e.id] });
        return;
      }
      if (text === node.label) return;
      const size = naturalSize(
        text,
        node.style?.fontSize ?? 16,
        node.style?.fontFamily ?? "sans",
      );
      this.edit({
        op: "update",
        id: e.id,
        set: {
          label: text,
          width: Math.max(node.width, Math.ceil(size.width) + 4),
          height: Math.max(node.height, Math.ceil(size.height)),
        },
      });
      return;
    }
    const current = "label" in node ? (node.label ?? "") : "";
    if (text === current) return;
    this.edit({ op: "update", id: e.id, set: { label: text === "" ? null : text } });
  }

  cancelLabel(): void {
    this.editing = null;
  }

  /* ---------------------------------------------------------------------- */
  /* Mutation                                                               */
  /* ---------------------------------------------------------------------- */

  /** Add nodes (one undo step) and select them; back to Select unless `keepTool`. */
  add(nodes: readonly FigureNode[], selectThem = true): boolean {
    const ok = this.edit({ op: "add", nodes });
    if (ok && selectThem) {
      this.selection = nodes.map((n) => n.id);
      if (this.tool !== "draw" && this.tool !== "eraser" && this.tool !== "laser")
        this.tool = "select";
    }
    return ok;
  }

  /** The language a new formula gets: the project's engine decides. */
  get defaultLanguage(): MathLanguage {
    return workspace.engine === "latex" ? "latex" : "typst";
  }

  /** Places a formula and opens the formula editor on it. */
  stampMath(point: Vec, source = "x"): void {
    const id = this.nextId();
    if (
      !this.add([
        {
          kind: "math",
          id,
          x: point.x,
          y: point.y,
          source,
          ...(this.defaultLanguage === "typst"
            ? {}
            : { language: this.defaultLanguage }),
          size: FORMULA_SIZE,
        },
      ])
    ) {
      return;
    }
    this.openFormula(id, true);
  }

  /** The colour a formula draws in, as the canvas resolves it. */
  #formulaColour(node: FigureNode): string {
    return strokeColour(styleDefaults.stroke(node.style ?? {}), layout.theme);
  }

  /** Opens the formula editor over `id`. */
  openFormula(id: string, fresh: boolean): void {
    const node = nodeById(this.scene, id);
    if (node?.kind !== "math" || node.locked === true) return;
    this.cancelLabel();
    clearTimeout(this.#formulaTimer);
    const language = languageOf(node);
    this.formula = { id, source: node.source, language, fresh };
    this.formulaShown = { id, source: node.source, language };
  }

  /** Typing in the formula editor: the draft is immediate, the canvas follows after a pause. */
  setFormulaSource(source: string): void {
    const f = this.formula;
    if (f === null) return;
    this.formula = { ...f, source };
    clearTimeout(this.#formulaTimer);
    this.#formulaTimer = setTimeout(() => {
      const now = this.formula;
      if (now !== null) {
        this.formulaShown = { id: now.id, source: now.source, language: now.language };
      }
    }, FORMULA_DEBOUNCE_MS);
  }

  /** The language switch re-renders at once. */
  setFormulaLanguage(language: MathLanguage): void {
    const f = this.formula;
    if (f === null || f.language === language) return;
    clearTimeout(this.#formulaTimer);
    this.formula = { ...f, language };
    this.formulaShown = { id: f.id, source: f.source, language };
  }

  /** Closes the formula editor, keeping what was typed. */
  commitFormula(): void {
    const f = this.formula;
    clearTimeout(this.#formulaTimer);
    this.formula = null;
    this.formulaShown = null;
    if (f === null) return;
    const node = nodeById(this.scene, f.id);
    if (node?.kind !== "math") return;
    if (f.source.trim() === "") {
      if (f.fresh) this.edit({ op: "remove", ids: [f.id] });
      return;
    }
    // A stamped formula is measured even when its text did not change.
    if (f.source === node.source && f.language === languageOf(node) && !f.fresh) return;
    void this.setFormula(f.id, { source: f.source, language: f.language });
  }

  /** Closes the formula editor and drops the draft (a fresh formula is removed). */
  cancelFormula(): void {
    const f = this.formula;
    clearTimeout(this.#formulaTimer);
    this.formula = null;
    this.formulaShown = null;
    if (f?.fresh === true) this.edit({ op: "remove", ids: [f.id] });
  }

  /**
   * Changes a formula's source, language or size as one update that also
   * records its measured box, so selection, alignment and arrow binding see
   * the size of the rendering. A formula that does not compile is stored
   * as typed and keeps its old box; the properties panel shows the error.
   */
  async setFormula(
    id: string,
    patch: {
      readonly source?: string;
      readonly language?: MathLanguage;
      readonly size?: number;
    },
  ): Promise<void> {
    const node = nodeById(this.scene, id);
    if (node?.kind !== "math") return;
    const next = {
      source: patch.source ?? node.source,
      language: patch.language ?? languageOf(node),
      size: patch.size ?? node.size,
    };
    const render = await mathRenders.load(
      next.language,
      next.source,
      next.size,
      this.#formulaColour(node),
    );
    const set: Record<string, unknown> = { ...next };
    if (render.width !== undefined && render.height !== undefined) {
      set["width"] = render.width;
      set["height"] = render.height;
    }
    this.edit({ op: "update", id, set });
  }

  setLabel(id: string, label: string): void {
    const node = nodeById(this.scene, id);
    if (node === undefined || node.kind === "freedraw" || node.kind === "math") return;
    if ((node.label ?? "") === label) return;
    this.edit({ op: "update", id, set: { label: label === "" ? null : label } });
  }

  /** Update arbitrary fields of one node (properties panel). */
  update(id: string, set: Record<string, unknown>): void {
    this.edit({ op: "update", id, set });
  }

  setStyle(patch: Style): void {
    if (this.selection.length === 0) return;
    this.edit({ op: "set-style", ids: [...this.selection], style: patch });
  }

  /** Text and label colour follows the stroke again. */
  clearLabelColour(): void {
    if (this.selection.length === 0) return;
    this.edit({
      op: "batch",
      ops: this.selection.map((id) => ({
        op: "update" as const,
        id,
        set: { style: { labelColor: null } },
      })),
    });
  }

  setPanelOpen(open: boolean): void {
    this.panelOpen = open;
    try {
      localStorage.setItem(PANEL_KEY, open ? "open" : "closed");
    } catch {
      // Blocked storage: the choice just is not remembered.
    }
  }

  /** The page background: a token or `#rrggbb`, or undefined for transparent. */
  setBackground(background: string | undefined): void {
    this.edit({
      op: "set-canvas",
      width: this.scene.width,
      height: this.scene.height,
      background: background ?? "none",
    });
  }

  /** Brings `rect` into view when it is not. */
  reveal(rect: Rect): void {
    this.view = revealRect(this.view, rect, this.width, this.height);
  }

  deleteSelected(): void {
    if (this.selection.length === 0) return;
    this.edit({ op: "remove", ids: [...this.selection] });
    this.selection = [];
  }

  /** Move the selection by a fixed step (arrow keys); one undo step each. */
  nudge(delta: Vec): void {
    if (this.selection.length === 0) return;
    this.edit({ op: "move", ids: [...this.selection], dx: delta.x, dy: delta.y });
  }

  align(edge: AlignEdge): void {
    if (this.selection.length >= 2)
      this.edit({ op: "align", ids: [...this.selection], edge });
  }

  distribute(axis: Axis): void {
    if (this.selection.length >= 3)
      this.edit({ op: "distribute", ids: [...this.selection], axis });
  }

  reorder(to: ZOrder): void {
    if (this.selection.length > 0)
      this.edit({ op: "reorder", ids: [...this.selection], to });
  }

  group(): void {
    if (this.selection.length >= 2)
      this.edit({ op: "group", ids: [...this.selection] });
  }

  ungroup(): void {
    if (this.selection.length > 0)
      this.edit({ op: "ungroup", ids: [...this.selection] });
  }

  /** Lock the selection, or unlock it when everything selected is locked. */
  toggleLock(): void {
    const nodes = this.selectedNodes;
    if (nodes.length === 0) return;
    const lock = !nodes.every((n) => n.locked === true);
    this.edit({
      op: "batch",
      ops: nodes.map((n) => ({
        op: "update" as const,
        id: n.id,
        set: { locked: lock ? true : null },
      })),
    });
  }

  clear(): void {
    this.edit({ op: "clear" });
    this.selection = [];
  }

  /* ---------------------------------------------------------------------- */
  /* Clipboard                                                              */
  /* ---------------------------------------------------------------------- */

  /** The selection as clipboard text, or null when nothing is selected. */
  copyText(): string | null {
    const nodes = this.selectedNodes;
    return nodes.length === 0 ? null : serializeNodes(nodes);
  }

  cut(): string | null {
    const text = this.copyText();
    if (text !== null) this.deleteSelected();
    return text;
  }

  #groupCounter(): () => string {
    let scene = this.scene;
    return () => {
      const g = nextGroupId(scene);
      // Reserve it so the next call moves on.
      scene = {
        ...scene,
        nodes: [
          ...scene.nodes,
          { kind: "rect", id: "", x: 0, y: 0, width: 0, height: 0, groups: [g] },
        ],
      };
      return g;
    };
  }

  #insertCopies(nodes: readonly FigureNode[], offset: number): void {
    if (nodes.length === 0) return;
    const copies = reidentify(nodes, () => this.nextId(), this.#groupCounter(), offset);
    this.add(copies);
  }

  duplicate(): void {
    this.#insertCopies(this.selectedNodes, PASTE_OFFSET);
  }

  /**
   * Paste text: figure nodes go in as copies, Mermaid text asks how to
   * insert it, anything else becomes a text node at the centre.
   */
  pasteText(text: string): void {
    const nodes = parseNodes(text);
    if (nodes !== undefined) {
      this.#insertCopies(nodes, PASTE_OFFSET);
      return;
    }
    if (looksLikeMermaid(text)) {
      this.mermaidPrompt = text;
      return;
    }
    if (text.trim() === "") return;
    this.pasteAsText(text);
  }

  /** `text` as a plain text node at the centre of the view. */
  pasteAsText(text: string): void {
    this.mermaidPrompt = null;
    const size = naturalSize(text, 16, "sans");
    const c = this.centre;
    this.add([
      {
        kind: "text",
        id: this.nextId(),
        x: c.x - size.width / 2,
        y: c.y - size.height / 2,
        width: Math.ceil(size.width) + 4,
        height: Math.ceil(size.height),
        label: text,
      },
    ]);
  }

  /** An image (a `data:` URI with its natural size) at the centre of the view. */
  pasteImage(dataUri: string, bytes: number, width: number, height: number): void {
    if (bytes > MAX_IMAGE_BYTES) {
      notice.error(
        `That image is ${Math.round(bytes / 1024)} KB; figures embed images up to ${MAX_IMAGE_BYTES / 1024} KB. Save it into the project and reference it instead.`,
      );
      return;
    }
    const scale = Math.min(1, 480 / Math.max(width, 1), 360 / Math.max(height, 1));
    const w = Math.max(width * scale, 8);
    const h = Math.max(height * scale, 8);
    const c = this.centre;
    this.add([
      {
        kind: "image",
        id: this.nextId(),
        x: c.x - w / 2,
        y: c.y - h / 2,
        width: w,
        height: h,
        src: dataUri,
      },
    ]);
  }

  /* ---------------------------------------------------------------------- */
  /* Mermaid                                                                */
  /* ---------------------------------------------------------------------- */

  /**
   * Where inserted content goes so it overlaps nothing: the middle of an
   * empty view, else just right of what is on the page, top aligned.
   */
  freeSpot(): Vec {
    if (this.scene.nodes.length === 0) {
      const c = this.centre;
      return { x: Math.max(c.x - 160, 40), y: Math.max(c.y - 100, 40) };
    }
    const b = selectionBounds(
      this.scene,
      this.scene.nodes.map((n) => n.id),
    );
    return { x: b.x + b.width + 60, y: b.y };
  }

  /** Insert Mermaid through the plugin, then follow its scene and select the result. */
  insertMermaid(source: string, mode: "shapes" | "block"): Promise<boolean> {
    this.mermaidPrompt = null;
    const at = this.freeSpot();
    return this.#run(async () => {
      try {
        const result = await this.#backend.fromMermaid({
          source,
          x: at.x,
          y: at.y,
          mode,
        });
        const revision = result.figure?.revision;
        if (typeof revision === "number")
          this.revision = Math.max(this.revision, revision);
        this.canUndo = true;
        this.canRedo = false;
        await this.#resync();
        this.selection = [...result.ids];
        this.tool = "select";
        if (result.ids.length > 0) {
          this.reveal(selectionBounds(this.scene, result.ids));
        }
        if (result.fallback)
          notice.show(`Inserted as a Mermaid block: ${result.fallback}`);
        return true;
      } catch (error) {
        reportError("figure.from-mermaid", error);
        notice.error(`Could not insert the Mermaid diagram: ${errorMessage(error)}`);
        return false;
      }
    });
  }

  /* ---------------------------------------------------------------------- */
  /* Keyboard                                                                */
  /* ---------------------------------------------------------------------- */

  /** Handle a key on the canvas; true when the editor used it. */
  key(e: KeyEventLike): boolean {
    const action = actionFor(e);
    if (action === undefined) return false;
    switch (action.type) {
      case "tool":
        this.setTool(action.tool);
        return true;
      case "nudge":
        if (this.selection.length === 0) return false;
        this.nudge({ x: action.dx, y: action.dy });
        return true;
      case "undo":
        void this.undo();
        return true;
      case "redo":
        void this.redo();
        return true;
      case "select-all":
        this.tool = "select";
        this.selection = selectAll(this.scene);
        return true;
      case "duplicate":
        this.duplicate();
        return true;
      case "group":
        this.group();
        return true;
      case "ungroup":
        this.ungroup();
        return true;
      case "lock":
        this.toggleLock();
        return true;
      case "front":
      case "back":
      case "forward":
      case "backward":
        this.reorder(action.type);
        return true;
      case "zoom-in":
        this.zoomBy(1);
        return true;
      case "zoom-out":
        this.zoomBy(-1);
        return true;
      case "zoom-reset":
        this.zoomReset();
        return true;
      case "zoom-fit":
        this.zoomToFit();
        return true;
      case "zoom-selection":
        this.zoomToSelection();
        return true;
      case "delete":
        this.deleteSelected();
        return true;
      case "escape":
        if (this.gesture !== null) this.cancel();
        else if (this.mermaidPrompt !== null) this.mermaidPrompt = null;
        else if (this.selection.length > 0) this.selection = [];
        else if (this.tool !== "select") this.tool = "select";
        return true;
      case "edit": {
        const one = this.selected;
        if (one === undefined) return false;
        this.startLabelEdit(one.id);
        return true;
      }
    }
  }
}

/** A blank 960 by 540 figure, the size the starter scenes use. */
export function blankFigure(): string {
  return serializeScene(createScene(960, 540));
}
