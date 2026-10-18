/**
 * Scene operations; mirrors `crates/manang-figure/src/ops.rs`.
 *
 * Each operation addresses nodes by id. `applyOp` returns a new scene (the
 * input is never mutated) and the ids that changed, or throws with the Rust
 * plugin's wording, so a refused operation leaves the caller's scene as it
 * was. Bound arrows re-route inside the same operation. The editor applies
 * operations optimistically with this module; the plugin applies them
 * authoritatively in Rust; `parity.test.ts` keeps the two in step.
 */

import { route } from "./binding";
import {
  nodeBounds,
  nodeRect,
  normalizeRect,
  rectCenter,
  unionRect,
  EMPTY_RECT,
} from "./geometry";
import { TOKEN_NAMES, isColour, isMalformedHex, normalizeColour } from "./palette";
import {
  STYLE_KEYS,
  isBindable,
  isConnector,
  isShape,
  mergeStyle,
  type Binding,
  type FigureNode,
  type FigureScene,
  type Rect,
  type Style,
  type Vec,
} from "./scene";

export type ZOrder = "front" | "back" | "forward" | "backward";
export type End = "start" | "end";
export type AlignEdge = "left" | "center" | "right" | "top" | "middle" | "bottom";
export type Axis = "horizontal" | "vertical";

/** One operation, exactly as `figure.apply` takes it (`op` tag, kebab-case). */
export type FigureOp =
  | {
      readonly op: "add";
      readonly nodes: readonly FigureNode[];
      readonly index?: number;
    }
  | {
      readonly op: "update";
      readonly id: string;
      readonly set: Readonly<Record<string, unknown>>;
    }
  | { readonly op: "remove"; readonly ids: readonly string[] }
  | {
      readonly op: "move";
      readonly ids: readonly string[];
      readonly dx: number;
      readonly dy: number;
    }
  | {
      readonly op: "resize";
      readonly id: string;
      readonly x: number;
      readonly y: number;
      readonly width: number;
      readonly height: number;
    }
  | { readonly op: "rotate"; readonly ids: readonly string[]; readonly angle: number }
  | { readonly op: "reorder"; readonly ids: readonly string[]; readonly to: ZOrder }
  | { readonly op: "group"; readonly ids: readonly string[]; readonly group?: string }
  | { readonly op: "ungroup"; readonly ids: readonly string[] }
  | {
      readonly op: "bind";
      readonly id: string;
      readonly end: End;
      readonly node: string | null;
      readonly anchor?: Vec;
    }
  | { readonly op: "align"; readonly ids: readonly string[]; readonly edge: AlignEdge }
  | { readonly op: "distribute"; readonly ids: readonly string[]; readonly axis: Axis }
  | {
      readonly op: "set-style";
      readonly ids?: readonly string[];
      readonly style?: Style;
      /** v1 form. */
      readonly id?: string;
      readonly stroke?: string;
      readonly fill?: string;
      readonly size?: number;
      readonly color?: string;
    }
  | {
      readonly op: "set-canvas";
      readonly width: number;
      readonly height: number;
      /** A token or `#rrggbb`; `none` clears it; absent leaves it. */
      readonly background?: string;
    }
  | { readonly op: "batch"; readonly ops: readonly FigureOp[] }
  | { readonly op: "clear" }
  | { readonly op: "undo" }
  | { readonly op: "redo" }
  | { readonly op: "add-node"; readonly node: FigureNode }
  | { readonly op: "update-node"; readonly node: FigureNode }
  | { readonly op: "remove-node"; readonly id: string }
  | {
      readonly op: "translate";
      readonly id: string;
      readonly dx: number;
      readonly dy: number;
    };

/** Operations that change the scene (everything but undo and redo). */
export type FigureEditOp = Exclude<FigureOp, { op: "undo" } | { op: "redo" }>;

export interface OpResult {
  readonly scene: FigureScene;
  /** Ids of nodes whose value changed, sorted. */
  readonly changed: string[];
  /** Whether anything (nodes, order or page) changed. */
  readonly applied: boolean;
}

const invalid = (message: string) => new Error(`invalid argument: ${message}`);
const notFound = (id: string) => new Error(`node with id \`${id}\` not found`);

/* -------------------------------------------------------------------------- */
/* Normal form and validation (serde's view of a node)                         */
/* -------------------------------------------------------------------------- */

type Json = Record<string, unknown>;

function isObject(value: unknown): value is Json {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Structural equality ignoring keys whose value is `undefined`. */
export function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b) || a.length !== b.length) return false;
    return a.every((v, i) => deepEqual(v, b[i]));
  }
  if (!isObject(a) || !isObject(b)) return false;
  const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
  for (const key of keys) {
    if (!deepEqual(a[key], b[key])) return false;
  }
  return true;
}

/** The node with every default omitted, as Rust serialises it. */
export function normalizeNode(node: FigureNode): FigureNode {
  const out: Json = {};
  for (const [key, value] of Object.entries(node)) {
    if (value === undefined) continue;
    switch (key) {
      case "angle":
        if (value === 0) continue;
        break;
      case "groups":
        if (Array.isArray(value) && value.length === 0) continue;
        break;
      case "locked":
      case "hidden":
        if (value === false) continue;
        break;
      case "language":
        // Typst is the default and is omitted, as in Rust.
        if (value === "typst") continue;
        break;
      case "width":
      case "height":
        // A formula not measured yet has no box of its own.
        if (node.kind === "math" && value === 0) continue;
        break;
      case "routing":
        if (value === "straight") continue;
        break;
      case "style": {
        const style = cleanStyle(value as Style);
        if (style === undefined) continue;
        out[key] = style;
        continue;
      }
      case "heads": {
        const heads = value as Json;
        const clean: Json = {};
        if (heads["start"] !== undefined) clean["start"] = heads["start"];
        if (heads["end"] !== undefined) clean["end"] = heads["end"];
        if (Object.keys(clean).length === 0) continue;
        out[key] = clean;
        continue;
      }
      case "start":
      case "end": {
        const b = value as Binding;
        out[key] =
          b.anchor === undefined
            ? { node: b.node }
            : { node: b.node, anchor: b.anchor };
        continue;
      }
      default:
        break;
    }
    out[key] = value;
  }
  return out as unknown as FigureNode;
}

function cleanStyle(style: Style): Style | undefined {
  const out: Json = {};
  for (const key of STYLE_KEYS) {
    const value = style[key];
    if (value === undefined) continue;
    out[key] =
      (key === "stroke" || key === "fill" || key === "labelColor") &&
      typeof value === "string"
        ? normalizeColour(value)
        : value;
  }
  return Object.keys(out).length === 0 ? undefined : (out as Style);
}

const SHAPE_KEYS = new Set([
  "kind",
  "id",
  "x",
  "y",
  "width",
  "height",
  "angle",
  "label",
  "src",
  "source",
  "style",
  "groups",
  "locked",
  "hidden",
]);
const CONNECTOR_KEYS = new Set([
  "kind",
  "id",
  "points",
  "start",
  "end",
  "routing",
  "heads",
  "label",
  "style",
  "groups",
  "locked",
  "hidden",
]);
const FREEDRAW_KEYS = new Set([
  "kind",
  "id",
  "points",
  "size",
  "style",
  "groups",
  "locked",
  "hidden",
]);
const MATH_KEYS = new Set([
  "kind",
  "id",
  "x",
  "y",
  "source",
  "language",
  "size",
  "width",
  "height",
  "style",
  "groups",
  "locked",
  "hidden",
]);
const SHAPE_KINDS = new Set([
  "rect",
  "diamond",
  "ellipse",
  "text",
  "frame",
  "image",
  "mermaid",
]);

function expectNumber(v: unknown, what: string): void {
  if (typeof v !== "number") throw new Error(`${what} must be a number`);
}

function expectString(v: unknown, what: string): void {
  if (typeof v !== "string") throw new Error(`${what} must be a string`);
}

function checkFields(
  n: Json,
  allowed: ReadonlySet<string>,
  required: readonly string[],
): void {
  for (const key of Object.keys(n)) {
    if (n[key] !== undefined && !allowed.has(key))
      throw new Error(`unknown field \`${key}\``);
  }
  for (const key of required) {
    if (n[key] === undefined) throw new Error(`missing field \`${key}\``);
  }
}

function checkVec(v: unknown, what: string, pressure = false): void {
  if (!isObject(v)) throw new Error(`${what} must be a point`);
  expectNumber(v["x"], `${what}.x`);
  expectNumber(v["y"], `${what}.y`);
  if (pressure) expectNumber(v["pressure"], `${what}.pressure`);
}

function checkStyle(v: unknown): void {
  if (v === undefined) return;
  if (!isObject(v)) throw new Error("`style` must be an object");
  for (const [key, value] of Object.entries(v)) {
    if (value === undefined) continue;
    switch (key) {
      case "stroke":
      case "fill":
      case "labelColor":
        expectString(value, key);
        break;
      case "strokeWidth":
      case "opacity":
      case "fontSize":
      case "fillOpacity":
        expectNumber(value, key);
        break;
      case "roughness":
        if (
          typeof value !== "number" ||
          !Number.isInteger(value) ||
          value < 0 ||
          value > 255
        ) {
          throw new Error("roughness must be a small whole number");
        }
        break;
      case "strokeStyle":
        if (!["solid", "dashed", "dotted"].includes(value as string)) {
          throw new Error(`unknown variant \`${String(value)}\``);
        }
        break;
      case "fontFamily":
        if (!["sans", "mono"].includes(value as string)) {
          throw new Error(`unknown variant \`${String(value)}\``);
        }
        break;
      case "textAlign":
        if (!["left", "center", "right"].includes(value as string)) {
          throw new Error(`unknown variant \`${String(value)}\``);
        }
        break;
      default:
        throw new Error(`unknown field \`${key}\``);
    }
  }
}

function checkCommon(n: Json): void {
  expectString(n["id"], "id");
  checkStyle(n["style"]);
  if (n["groups"] !== undefined) {
    if (!Array.isArray(n["groups"]) || n["groups"].some((g) => typeof g !== "string")) {
      throw new Error("`groups` must be a list of strings");
    }
  }
  if (n["locked"] !== undefined && typeof n["locked"] !== "boolean") {
    throw new Error("`locked` must be true or false");
  }
  if (n["hidden"] !== undefined && typeof n["hidden"] !== "boolean") {
    throw new Error("`hidden` must be true or false");
  }
}

function checkBinding(v: unknown, what: string): void {
  if (v === undefined || v === null) return;
  if (!isObject(v)) throw new Error(`\`${what}\` must be a binding`);
  for (const key of Object.keys(v)) {
    if (v[key] !== undefined && key !== "node" && key !== "anchor") {
      throw new Error(`unknown field \`${key}\``);
    }
  }
  expectString(v["node"], `${what}.node`);
  if (v["anchor"] !== undefined) checkVec(v["anchor"], `${what}.anchor`);
}

/** serde's typed view: throws a plain message when `value` is not a node. */
function checkShape(value: unknown): asserts value is FigureNode {
  if (!isObject(value)) throw new Error("a node must be an object");
  const kind = value["kind"];
  if (typeof kind !== "string") throw new Error("missing field `kind`");
  if (SHAPE_KINDS.has(kind)) {
    checkFields(value, SHAPE_KEYS, ["id", "x", "y", "width", "height"]);
    for (const k of ["x", "y", "width", "height"]) expectNumber(value[k], k);
    if (value["angle"] !== undefined) expectNumber(value["angle"], "angle");
    for (const k of ["label", "src", "source"]) {
      if (value[k] !== undefined) expectString(value[k], k);
    }
  } else if (kind === "arrow" || kind === "line") {
    checkFields(value, CONNECTOR_KEYS, ["id", "points"]);
    if (!Array.isArray(value["points"])) throw new Error("`points` must be a list");
    value["points"].forEach((p, i) => checkVec(p, `points[${i}]`));
    checkBinding(value["start"], "start");
    checkBinding(value["end"], "end");
    if (
      value["routing"] !== undefined &&
      !["straight", "elbow", "curved"].includes(value["routing"] as string)
    ) {
      throw new Error(`unknown variant \`${String(value["routing"])}\``);
    }
    if (value["heads"] !== undefined) {
      const heads = value["heads"];
      if (!isObject(heads)) throw new Error("`heads` must be an object");
      for (const [k, h] of Object.entries(heads)) {
        if (h === undefined) continue;
        if (k !== "start" && k !== "end") throw new Error(`unknown field \`${k}\``);
        if (!["none", "arrow", "triangle", "dot", "bar"].includes(h as string)) {
          throw new Error(`unknown variant \`${String(h)}\``);
        }
      }
    }
    if (value["label"] !== undefined) expectString(value["label"], "label");
  } else if (kind === "freedraw") {
    checkFields(value, FREEDRAW_KEYS, ["id", "points", "size"]);
    if (!Array.isArray(value["points"])) throw new Error("`points` must be a list");
    value["points"].forEach((p, i) => checkVec(p, `points[${i}]`, true));
    expectNumber(value["size"], "size");
  } else if (kind === "math") {
    checkFields(value, MATH_KEYS, ["id", "x", "y", "source", "size"]);
    for (const k of ["x", "y", "size"]) expectNumber(value[k], k);
    for (const k of ["width", "height"]) {
      if (value[k] !== undefined) expectNumber(value[k], k);
    }
    expectString(value["source"], "source");
    if (
      value["language"] !== undefined &&
      !["typst", "latex"].includes(value["language"] as string)
    ) {
      throw new Error(`unknown variant \`${String(value["language"])}\``);
    }
  } else {
    throw new Error(`unknown variant \`${kind}\``);
  }
  checkCommon(value);
}

function nodeNumbers(node: FigureNode): number[] {
  const s = node.style ?? {};
  const out = [s.strokeWidth, s.opacity, s.fontSize, s.fillOpacity].filter(
    (n): n is number => n !== undefined,
  );
  switch (node.kind) {
    case "arrow":
    case "line":
      for (const p of node.points) out.push(p.x, p.y);
      for (const b of [node.start, node.end]) {
        if (b?.anchor !== undefined) out.push(b.anchor.x, b.anchor.y);
      }
      break;
    case "freedraw":
      out.push(node.size);
      for (const p of node.points) out.push(p.x, p.y, p.pressure);
      break;
    case "math":
      out.push(node.x, node.y, node.size, node.width ?? 0, node.height ?? 0);
      break;
    default:
      out.push(node.x, node.y, node.width, node.height, node.angle ?? 0);
  }
  return out;
}

/** `FigureNode::validate`. */
export function validateNode(node: FigureNode): void {
  const id = node.id;
  if (id === "") throw invalid("a node id must not be empty");
  if (nodeNumbers(node).some((n) => !Number.isFinite(n))) {
    throw invalid(`node \`${id}\`: every number must be finite`);
  }
  if (isConnector(node) && node.points.length < 2) {
    throw invalid(`node \`${id}\`: a connector needs at least two points`);
  }
  if (node.kind === "image" && (node.src === undefined || node.src === "")) {
    throw invalid(`node \`${id}\`: an image needs \`src\``);
  }
  if (node.kind === "mermaid" && node.source === undefined) {
    throw invalid(`node \`${id}\`: a Mermaid block needs \`source\``);
  }
  for (const c of [node.style?.stroke, node.style?.fill, node.style?.labelColor]) {
    if (c !== undefined && isMalformedHex(c)) {
      throw invalid(`node \`${id}\`: \`${c}\` is not a valid #rrggbb colour`);
    }
  }
  if (node.kind === "math" && !(node.size > 0 && node.size <= 1024)) {
    throw invalid(`node \`${id}\`: a formula's size must be between 0 and 1024`);
  }
}

/** `validate_colours`. */
export function validateColours(style: Style | undefined): void {
  if (style === undefined) return;
  for (const value of [style.stroke, style.fill, style.labelColor]) {
    if (value !== undefined && !isColour(value)) {
      throw invalid(
        `unknown colour \`${value}\`; use none, a palette token (${TOKEN_NAMES.join(", ")}) or #rrggbb`,
      );
    }
  }
  if (
    style.fillOpacity !== undefined &&
    !(style.fillOpacity >= 0 && style.fillOpacity <= 100)
  ) {
    throw invalid("fillOpacity must be between 0 and 100");
  }
  if (style.opacity !== undefined && !(style.opacity >= 0 && style.opacity <= 100)) {
    throw invalid("opacity must be between 0 and 100");
  }
  const w = style.strokeWidth;
  if (w !== undefined && !(Number.isFinite(w) && w > 0 && w <= 64)) {
    throw invalid("strokeWidth must be between 0 and 64");
  }
  const f = style.fontSize;
  if (f !== undefined && !(Number.isFinite(f) && f > 0 && f <= 512)) {
    throw invalid("fontSize must be between 0 and 512");
  }
}

function finite(values: readonly number[]): void {
  if (!values.every((v) => Number.isFinite(v)))
    throw invalid("every number must be finite");
}

/* -------------------------------------------------------------------------- */
/* Transaction                                                                 */
/* -------------------------------------------------------------------------- */

class Tx {
  nodes: FigureNode[];
  width: number;
  height: number;
  background: string | undefined;
  readonly before = new Map<string, FigureNode | undefined>();
  orderBefore: string[] | undefined;
  readonly #base: FigureScene;

  constructor(scene: FigureScene) {
    this.#base = scene;
    this.nodes = [...scene.nodes];
    this.width = scene.width;
    this.height = scene.height;
    this.background = scene.background;
  }

  get scene(): FigureScene {
    const { background: _dropped, ...rest } = this.#base;
    return {
      ...rest,
      width: this.width,
      height: this.height,
      ...(this.background === undefined ? {} : { background: this.background }),
      nodes: this.nodes,
    };
  }

  find(id: string): FigureNode | undefined {
    return this.nodes.find((n) => n.id === id);
  }

  touch(id: string): void {
    if (!this.before.has(id)) this.before.set(id, this.find(id));
  }

  snapshotOrder(): void {
    this.orderBefore ??= this.nodes.map((n) => n.id);
  }

  require(id: string): FigureNode {
    const node = this.find(id);
    if (node === undefined) throw notFound(id);
    return node;
  }

  /** Replace node `id` with `f(current)`. */
  update(id: string, f: (node: FigureNode) => FigureNode): void {
    const current = this.require(id);
    this.touch(id);
    const next = normalizeNode(f(current));
    this.nodes = this.nodes.map((n) => (n.id === id ? next : n));
  }

  insert(index: number | undefined, node: FigureNode): void {
    validateNode(node);
    validateColours(node.style);
    if (this.find(node.id) !== undefined) {
      throw new Error(`node with id \`${node.id}\` already exists`);
    }
    this.snapshotOrder();
    this.touch(node.id);
    const clean = normalizeNode(node);
    if (index === undefined) {
      this.nodes = [...this.nodes, clean];
    } else {
      const i = Math.min(index, this.nodes.length);
      this.nodes = [...this.nodes.slice(0, i), clean, ...this.nodes.slice(i)];
    }
  }

  remove(id: string): void {
    this.require(id);
    this.snapshotOrder();
    this.touch(id);
    this.nodes = this.nodes.filter((n) => n.id !== id);
  }

  setOrder(order: readonly string[]): void {
    this.snapshotOrder();
    this.nodes = reorderTo(this.nodes, order);
  }
}

function reorderTo(
  nodes: readonly FigureNode[],
  order: readonly string[],
): FigureNode[] {
  const rest = [...nodes];
  const out: FigureNode[] = [];
  for (const id of order) {
    const i = rest.findIndex((n) => n.id === id);
    if (i !== -1) out.push(...rest.splice(i, 1));
  }
  return [...out, ...rest];
}

/**
 * `op` applied to `scene`. Throws (with the plugin's message) when refused;
 * `undo` and `redo` are history operations and are refused here.
 */
export function applyOp(scene: FigureScene, op: FigureOp): OpResult {
  const tx = new Tx(scene);
  run(tx, op);
  reroute(tx);
  const changed: string[] = [];
  for (const [id, old] of tx.before) {
    if (!deepEqual(old, tx.find(id))) changed.push(id);
  }
  changed.sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  const order = tx.nodes.map((n) => n.id);
  const orderChanged =
    tx.orderBefore !== undefined && !deepEqual(tx.orderBefore, order);
  const canvasChanged =
    tx.width !== scene.width ||
    tx.height !== scene.height ||
    tx.background !== scene.background;
  const applied = changed.length > 0 || orderChanged || canvasChanged;
  return { scene: applied ? tx.scene : scene, changed, applied };
}

function translateNode(node: FigureNode, dx: number, dy: number): FigureNode {
  switch (node.kind) {
    case "arrow":
    case "line":
      return {
        ...node,
        points: node.points.map((p) => ({ x: p.x + dx, y: p.y + dy })),
      };
    case "freedraw":
      return {
        ...node,
        points: node.points.map((p) => ({
          x: p.x + dx,
          y: p.y + dy,
          pressure: p.pressure,
        })),
      };
    default:
      return { ...node, x: node.x + dx, y: node.y + dy };
  }
}

function run(tx: Tx, op: FigureOp): void {
  switch (op.op) {
    case "undo":
    case "redo":
      throw invalid("undo and redo cannot be part of a batch");
    case "batch":
      for (const inner of op.ops) run(tx, inner);
      return;
    case "add":
      op.nodes.forEach((node, k) => {
        checkParsed(node);
        tx.insert(op.index === undefined ? undefined : op.index + k, node);
      });
      return;
    case "add-node":
      checkParsed(op.node);
      tx.insert(undefined, op.node);
      return;
    case "update-node": {
      checkParsed(op.node);
      validateNode(op.node);
      validateColours(op.node.style);
      tx.update(op.node.id, () => op.node);
      return;
    }
    case "update":
      update(tx, op.id, op.set);
      return;
    case "remove":
      remove(tx, op.ids);
      return;
    case "remove-node":
      remove(tx, [op.id]);
      return;
    case "move":
      moveNodes(tx, op.ids, op.dx, op.dy);
      return;
    case "translate":
      moveNodes(tx, [op.id], op.dx, op.dy);
      return;
    case "resize":
      finite([op.x, op.y, op.width, op.height]);
      resize(tx, op.id, normalizeRect(op.x, op.y, op.width, op.height));
      return;
    case "rotate": {
      finite([op.angle]);
      const a = ((op.angle % 360) + 360) % 360;
      for (const id of op.ids) {
        tx.update(id, (n) => (isShape(n) ? { ...n, angle: a } : n));
      }
      return;
    }
    case "reorder":
      reorder(tx, op.ids, op.to);
      return;
    case "group": {
      if (op.ids.length < 2) throw invalid("a group needs at least two nodes");
      const gid =
        op.group !== undefined && op.group !== "" ? op.group : nextGroupId(tx.scene);
      for (const id of op.ids) {
        tx.update(id, (n) => {
          const groups = n.groups ?? [];
          return groups.includes(gid) ? n : { ...n, groups: [...groups, gid] };
        });
      }
      return;
    }
    case "ungroup":
      for (const id of op.ids) {
        tx.update(id, (n) => ({ ...n, groups: (n.groups ?? []).slice(0, -1) }));
      }
      return;
    case "bind":
      bind(tx, op.id, op.end, op.node, op.anchor);
      return;
    case "align":
      align(tx, op.ids, op.edge);
      return;
    case "distribute":
      distribute(tx, op.ids, op.axis);
      return;
    case "set-style": {
      const targets = [...(op.ids ?? []), ...(op.id === undefined ? [] : [op.id])];
      const legacy: Style = {
        ...(op.stroke !== undefined || op.color !== undefined
          ? { stroke: (op.stroke ?? op.color) as string }
          : {}),
        ...(op.fill !== undefined ? { fill: op.fill } : {}),
      };
      const patch = mergeStyle(op.style ?? {}, legacy);
      validateColours(patch);
      const size = op.size;
      if (size !== undefined) finite([size]);
      for (const id of targets) {
        tx.update(id, (n) => {
          let next: FigureNode = { ...n, style: mergeStyle(n.style ?? {}, patch) };
          if (
            size !== undefined &&
            (next.kind === "freedraw" || next.kind === "math")
          ) {
            next = { ...next, size };
          }
          return next;
        });
      }
      return;
    }
    case "set-canvas":
      if (!(
        Number.isFinite(op.width) &&
        Number.isFinite(op.height) &&
        op.width > 0 &&
        op.height > 0
      )) {
        throw invalid("the page size must be positive and finite");
      }
      if (op.background !== undefined && !isColour(op.background)) {
        throw invalid(
          `the background \`${op.background}\` is not none, a palette token or #rrggbb`,
        );
      }
      tx.width = op.width;
      tx.height = op.height;
      if (op.background === "none") tx.background = undefined;
      else if (op.background !== undefined)
        tx.background = normalizeColour(op.background);
      return;
    case "clear":
      for (const id of tx.nodes.map((n) => n.id)) tx.remove(id);
      return;
  }
}

function checkParsed(node: unknown): void {
  try {
    checkShape(node);
  } catch (error) {
    throw invalid(
      `invalid \`op\`: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

/** `g<base36>` one past the highest group id in use. */
export function nextGroupId(scene: FigureScene): string {
  let highest = 0;
  for (const node of scene.nodes) {
    for (const g of node.groups ?? []) {
      const digits = g.startsWith("g") ? g.slice(1) : "";
      if (digits !== "" && /^[0-9a-zA-Z]+$/.test(digits)) {
        const n = Number.parseInt(digits, 36);
        if (Number.isFinite(n)) highest = Math.max(highest, n);
      }
    }
  }
  return `g${(highest + 1).toString(36)}`;
}

function update(tx: Tx, id: string, set: Readonly<Record<string, unknown>>): void {
  const current = tx.require(id);
  const fields: Json = { ...(current as unknown as Json) };
  for (const [key, v] of Object.entries(set)) {
    if (key === "id") {
      if (v === id) continue;
      throw invalid("a node's id cannot change");
    }
    if (key === "style" && isObject(v)) {
      const style: Json = { ...((fields["style"] as Json | undefined) ?? {}) };
      for (const [k, sv] of Object.entries(v)) {
        if (sv === null) delete style[k];
        else style[k] = sv;
      }
      fields["style"] = style;
      continue;
    }
    if (v === null) delete fields[key];
    else fields[key] = v;
  }
  try {
    checkShape(fields);
  } catch (error) {
    throw invalid(
      `cannot update \`${id}\`: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  const node = normalizeNode(fields);
  validateNode(node);
  validateColours(node.style);
  tx.update(id, () => node);
}

function remove(tx: Tx, ids: readonly string[]): void {
  for (const id of ids) tx.remove(id);
  const removed = new Set(ids);
  const bound = tx.nodes
    .filter(isConnector)
    .filter((c) => [c.start, c.end].some((b) => b !== undefined && removed.has(b.node)))
    .map((c) => c.id);
  for (const cid of bound) {
    tx.update(cid, (n) => {
      if (!isConnector(n)) return n;
      const next: Json = { ...n };
      if (n.start !== undefined && removed.has(n.start.node)) delete next["start"];
      if (n.end !== undefined && removed.has(n.end.node)) delete next["end"];
      return next as unknown as FigureNode;
    });
  }
}

function moveNodes(tx: Tx, ids: readonly string[], dx: number, dy: number): void {
  finite([dx, dy]);
  for (const id of ids) tx.require(id);
  if (dx === 0 && dy === 0) return;
  const moving = new Set(ids);
  for (const id of ids) {
    tx.update(id, (n) => {
      const moved = translateNode(n, dx, dy);
      if (!isConnector(moved)) return moved;
      // An arrow dragged on its own lets go of shapes that stay put.
      const next: Json = { ...moved };
      if (moved.start !== undefined && !moving.has(moved.start.node))
        delete next["start"];
      if (moved.end !== undefined && !moving.has(moved.end.node)) delete next["end"];
      return next as unknown as FigureNode;
    });
  }
}

function scalePoint(p: Vec, from: Rect, to: Rect): Vec {
  const sx = from.width > 0 ? to.width / from.width : 1;
  const sy = from.height > 0 ? to.height / from.height : 1;
  return { x: to.x + (p.x - from.x) * sx, y: to.y + (p.y - from.y) * sy };
}

function resize(tx: Tx, id: string, to: Rect): void {
  const from = nodeRect(tx.require(id));
  tx.update(id, (n) => {
    switch (n.kind) {
      case "arrow":
      case "line":
        return { ...n, points: n.points.map((p) => scalePoint(p, from, to)) };
      case "freedraw":
        return {
          ...n,
          points: n.points.map((p) => {
            const q = scalePoint(p, from, to);
            return { x: q.x, y: q.y, pressure: p.pressure };
          }),
        };
      case "math": {
        // A formula scales as a whole: by whichever side moved more.
        const kw = from.width > 0 && to.width > 0 ? to.width / from.width : 1;
        const kh = from.height > 0 && to.height > 0 ? to.height / from.height : 1;
        const k = Math.abs(kw - 1) > Math.abs(kh - 1) ? kw : kh;
        return {
          ...n,
          size: n.size * k,
          width: from.width * k,
          height: from.height * k,
          x: to.x,
          y: to.y,
        };
      }
      default:
        return { ...n, x: to.x, y: to.y, width: to.width, height: to.height };
    }
  });
}

function reorder(tx: Tx, ids: readonly string[], to: ZOrder): void {
  for (const id of ids) tx.require(id);
  const chosen = new Set(ids);
  let order = tx.nodes.map((n) => n.id);
  if (to === "front" || to === "back") {
    const picked = order.filter((id) => chosen.has(id));
    const rest = order.filter((id) => !chosen.has(id));
    order = to === "front" ? [...rest, ...picked] : [...picked, ...rest];
  } else if (to === "forward") {
    for (let i = order.length - 2; i >= 0; i -= 1) {
      const a = order[i] as string;
      const b = order[i + 1] as string;
      if (chosen.has(a) && !chosen.has(b)) {
        order[i] = b;
        order[i + 1] = a;
      }
    }
  } else {
    for (let i = 1; i < order.length; i += 1) {
      const a = order[i] as string;
      const b = order[i - 1] as string;
      if (chosen.has(a) && !chosen.has(b)) {
        order[i] = b;
        order[i - 1] = a;
      }
    }
  }
  tx.setOrder(order);
}

function bind(
  tx: Tx,
  id: string,
  end: End,
  target: string | null,
  anchor: Vec | undefined,
): void {
  if (!isConnector(tx.require(id)))
    throw invalid(`\`${id}\` is not an arrow or a line`);
  if (target !== null) {
    const node = tx.require(target);
    if (!isBindable(node) || target === id) {
      throw invalid(`an arrow cannot attach to \`${target}\` (a ${node.kind})`);
    }
  }
  if (
    anchor !== undefined &&
    !(anchor.x >= 0 && anchor.x <= 1 && anchor.y >= 0 && anchor.y <= 1)
  ) {
    throw invalid("an anchor is a fraction of the box: x and y between 0 and 1");
  }
  tx.update(id, (n) => {
    const next: Json = { ...n };
    if (target === null) delete next[end];
    else next[end] = anchor === undefined ? { node: target } : { node: target, anchor };
    return next as unknown as FigureNode;
  });
}

function boundsOf(tx: Tx, ids: readonly string[]): [string, Rect][] {
  return ids.map((id) => [id, nodeBounds(tx.require(id))]);
}

function align(tx: Tx, ids: readonly string[], edge: AlignEdge): void {
  if (ids.length < 2) throw invalid("align needs at least two nodes");
  const boxes = boundsOf(tx, ids);
  const all = boxes.reduce<Rect>((acc, [, b]) => unionRect(acc, b), EMPTY_RECT);
  const ac = rectCenter(all);
  for (const [id, b] of boxes) {
    const bc = rectCenter(b);
    let dx = 0;
    let dy = 0;
    switch (edge) {
      case "left":
        dx = all.x - b.x;
        break;
      case "center":
        dx = ac.x - bc.x;
        break;
      case "right":
        dx = all.x + all.width - (b.x + b.width);
        break;
      case "top":
        dy = all.y - b.y;
        break;
      case "middle":
        dy = ac.y - bc.y;
        break;
      case "bottom":
        dy = all.y + all.height - (b.y + b.height);
        break;
    }
    if (dx !== 0 || dy !== 0) tx.update(id, (n) => translateNode(n, dx, dy));
  }
}

function distribute(tx: Tx, ids: readonly string[], axis: Axis): void {
  if (ids.length < 3) throw invalid("distribute needs at least three nodes");
  const horizontal = axis === "horizontal";
  const key = (r: Rect) => (horizontal ? r.x : r.y);
  const size = (r: Rect) => (horizontal ? r.width : r.height);
  const boxes = boundsOf(tx, ids).sort(
    (a, b) => key(a[1]) - key(b[1]) || (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0),
  );
  const first = (boxes[0] as [string, Rect])[1];
  const last = (boxes[boxes.length - 1] as [string, Rect])[1];
  const span = key(last) + size(last) - key(first);
  const total = boxes.reduce((s, [, b]) => s + size(b), 0);
  const gap = (span - total) / (boxes.length - 1);
  let at = key(first);
  for (const [id, b] of boxes) {
    const d = at - key(b);
    at += size(b) + gap;
    if (Math.abs(d) > 1e-9) {
      tx.update(id, (n) =>
        horizontal ? translateNode(n, d, 0) : translateNode(n, 0, d),
      );
    }
  }
}

/** Re-routes connectors touched in this operation or bound to a touched shape. */
function reroute(tx: Tx): void {
  const touched = new Set(tx.before.keys());
  if (touched.size === 0) return;
  const scene = tx.scene;
  const updates: [string, Vec[]][] = [];
  for (const node of tx.nodes) {
    if (!isConnector(node)) continue;
    if (node.start === undefined && node.end === undefined) continue;
    const relevant =
      touched.has(node.id) ||
      [node.start, node.end].some((b) => b !== undefined && touched.has(b.node));
    if (!relevant) continue;
    const points = route(scene, node);
    if (!deepEqual(points, node.points)) updates.push([node.id, points]);
  }
  for (const [id, points] of updates) {
    tx.update(id, (n) => (isConnector(n) ? { ...n, points } : n));
  }
}
