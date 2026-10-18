/**
 * Manang figure scene graph, format version 2 (ADR 0012).
 *
 * Mirrors `crates/manang-figure/src/scene.rs` and `migrate.rs`. The scene is
 * plain data: a page size and nodes in paint order (first is bottom), stored
 * as `<name>.figure.json`. Optional fields are omitted at their default.
 * Geometry is in scene units (1 unit = 1 CSS pixel at 100% zoom), origin top
 * left, like SVG.
 */

export interface Vec {
  readonly x: number;
  readonly y: number;
}

export interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** A sampled point on a freehand path, with the pressure that produced it. */
export interface StrokeSample extends Vec {
  readonly pressure: number;
}

export type NodeId = string;

export type StrokeStyle = "solid" | "dashed" | "dotted";
export type FontFamily = "sans" | "mono";
export type TextAlign = "left" | "center" | "right";

/** Per-node style; every field optional, `undefined` means the default. */
export interface Style {
  /** A palette token or `#rrggbb`. Default `ink`. */
  readonly stroke?: string;
  /** `none`, a palette token or `#rrggbb`. Default `none`. */
  readonly fill?: string;
  readonly strokeWidth?: number;
  readonly strokeStyle?: StrokeStyle;
  /** 0 to 100. */
  readonly opacity?: number;
  /** 0 clean, 1 sketchy, 2 very sketchy. */
  readonly roughness?: number;
  readonly fontFamily?: FontFamily;
  readonly fontSize?: number;
  readonly textAlign?: TextAlign;
}

export const DEFAULT_STROKE_WIDTH = 1.5;
export const DEFAULT_FONT_SIZE = 16;

/** Key order of `Style` as Rust writes it. */
export const STYLE_KEYS = [
  "stroke",
  "fill",
  "strokeWidth",
  "strokeStyle",
  "opacity",
  "roughness",
  "fontFamily",
  "fontSize",
  "textAlign",
] as const;

export type ShapeKind =
  "rect" | "diamond" | "ellipse" | "text" | "frame" | "image" | "mermaid";

export interface ShapeNode {
  readonly kind: ShapeKind;
  readonly id: NodeId;
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  /** Degrees, clockwise, about the box centre. */
  readonly angle?: number;
  /** Shapes: label; text: the text; frames: the title. */
  readonly label?: string;
  /** Images: a path relative to the figure or a `data:` URI. */
  readonly src?: string;
  /** Mermaid blocks: their source. */
  readonly source?: string;
  readonly style?: Style;
  /** Group ids, innermost first. */
  readonly groups?: readonly string[];
  readonly locked?: boolean;
}

export interface Binding {
  readonly node: NodeId;
  /** Fractions of the bound box; absent = floating along the outline. */
  readonly anchor?: Vec;
}

export type Routing = "straight" | "elbow" | "curved";
export type Head = "none" | "arrow" | "triangle" | "dot" | "bar";

export interface Heads {
  readonly start?: Head;
  readonly end?: Head;
}

export interface ConnectorNode {
  readonly kind: "arrow" | "line";
  readonly id: NodeId;
  /** Absolute points, at least two. */
  readonly points: readonly Vec[];
  readonly start?: Binding;
  readonly end?: Binding;
  readonly routing?: Routing;
  readonly heads?: Heads;
  readonly label?: string;
  readonly style?: Style;
  readonly groups?: readonly string[];
  readonly locked?: boolean;
}

export interface FreedrawNode {
  readonly kind: "freedraw";
  readonly id: NodeId;
  readonly points: readonly StrokeSample[];
  /** Base diameter in scene units, before pressure thinning. */
  readonly size: number;
  readonly style?: Style;
  readonly groups?: readonly string[];
  readonly locked?: boolean;
}

/** A stamped formula. `tex` is the source; rendering happens downstream. */
export interface MathNode {
  readonly kind: "math";
  readonly id: NodeId;
  readonly x: number;
  /** Baseline. */
  readonly y: number;
  readonly tex: string;
  readonly size: number;
  readonly style?: Style;
  readonly groups?: readonly string[];
  readonly locked?: boolean;
}

export type FigureNode = ShapeNode | ConnectorNode | FreedrawNode | MathNode;

export const SCENE_VERSION = 2;

export interface FigureScene {
  readonly version: number;
  readonly width: number;
  readonly height: number;
  readonly nodes: readonly FigureNode[];
}

export function createScene(width: number, height: number): FigureScene {
  return { version: SCENE_VERSION, width, height, nodes: [] };
}

/* -------------------------------------------------------------------------- */
/* Accessors                                                                   */
/* -------------------------------------------------------------------------- */

const SHAPE_KINDS: ReadonlySet<string> = new Set([
  "rect",
  "diamond",
  "ellipse",
  "text",
  "frame",
  "image",
  "mermaid",
]);

export function isShape(node: FigureNode): node is ShapeNode {
  return SHAPE_KINDS.has(node.kind);
}

export function isConnector(node: FigureNode): node is ConnectorNode {
  return node.kind === "arrow" || node.kind === "line";
}

export function shapeOf(node: FigureNode | undefined): ShapeNode | undefined {
  return node !== undefined && isShape(node) ? node : undefined;
}

export function connectorOf(node: FigureNode | undefined): ConnectorNode | undefined {
  return node !== undefined && isConnector(node) ? node : undefined;
}

/** Whether arrows may bind to this node. */
export function isBindable(node: FigureNode): boolean {
  return isShape(node);
}

/** The visible text: label, text content, frame title or formula. */
export function labelOf(node: FigureNode): string | undefined {
  switch (node.kind) {
    case "freedraw":
      return undefined;
    case "math":
      return node.tex;
    default:
      return node.label;
  }
}

export function styleOf(node: FigureNode): Style {
  return node.style ?? {};
}

export function groupsOf(node: FigureNode): readonly string[] {
  return node.groups ?? [];
}

export function nodeById(scene: FigureScene, id: NodeId): FigureNode | undefined {
  return scene.nodes.find((node) => node.id === id);
}

export function replaceNode(scene: FigureScene, next: FigureNode): FigureScene {
  return {
    ...scene,
    nodes: scene.nodes.map((node) => (node.id === next.id ? next : node)),
  };
}

export function appendNode(scene: FigureScene, node: FigureNode): FigureScene {
  return { ...scene, nodes: [...scene.nodes, node] };
}

export function removeNode(scene: FigureScene, id: NodeId): FigureScene {
  return { ...scene, nodes: scene.nodes.filter((node) => node.id !== id) };
}

/** Style resolution with defaults, as `Style`'s accessors in Rust. */
export const styleDefaults = {
  stroke: (s: Style): string => s.stroke ?? "ink",
  fill: (s: Style): string => s.fill ?? "none",
  strokeWidth: (s: Style): number => s.strokeWidth ?? DEFAULT_STROKE_WIDTH,
  strokeStyle: (s: Style): StrokeStyle => s.strokeStyle ?? "solid",
  opacity: (s: Style): number => Math.min(Math.max((s.opacity ?? 100) / 100, 0), 1),
  roughness: (s: Style): number => Math.min(s.roughness ?? 0, 2),
  fontFamily: (s: Style): FontFamily => s.fontFamily ?? "sans",
  fontSize: (s: Style): number => s.fontSize ?? DEFAULT_FONT_SIZE,
};

/** `other` merged over `base`: every field `other` sets wins. Key order as Rust. */
export function mergeStyle(base: Style, other: Style): Style {
  const out: Record<string, unknown> = {};
  for (const key of STYLE_KEYS) {
    const value = other[key] ?? base[key];
    if (value !== undefined) out[key] = value;
  }
  return out as Style;
}

/* -------------------------------------------------------------------------- */
/* Ids                                                                         */
/* -------------------------------------------------------------------------- */

/**
 * Ids come from a counter, not randomness, so replaying the same interaction
 * produces byte-identical scene JSON.
 */
export function formatNodeId(counter: number): NodeId {
  return `n${counter.toString(36)}`;
}

export function parseNodeId(id: string): number | undefined {
  const digits = id.startsWith("n") ? id.slice(1) : "";
  if (digits === "" || digits.length > 10 || !/^[0-9a-z]+$/.test(digits))
    return undefined;
  return Number.parseInt(digits, 36);
}

export function highestCounter(scene: FigureScene): number {
  let highest = 0;
  for (const node of scene.nodes) {
    const parsed = parseNodeId(node.id);
    if (parsed !== undefined) highest = Math.max(highest, parsed);
  }
  return highest;
}

export function nextNodeId(scene: FigureScene): NodeId {
  return formatNodeId(highestCounter(scene) + 1);
}

/* -------------------------------------------------------------------------- */
/* Reading older formats (migrate.rs)                                          */
/* -------------------------------------------------------------------------- */

type Json = Record<string, unknown>;

function isObject(value: unknown): value is Json {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function num(map: Json, key: string): number {
  const value = map[key];
  return typeof value === "number" ? value : 0;
}

function styleFrom(stroke: unknown, fill: unknown): Json | undefined {
  const style: Json = {};
  if (stroke !== undefined && stroke !== null) style["stroke"] = stroke;
  if (fill !== undefined && fill !== null) style["fill"] = fill;
  return Object.keys(style).length > 0 ? style : undefined;
}

function v1Node(node: unknown): Json {
  if (!isObject(node))
    throw new Error("invalid argument: every node must be a JSON object");
  const kind = typeof node["kind"] === "string" ? node["kind"] : "";
  const id = node["id"] ?? null;
  const out: Json = {};
  const withStyle = (style: Json | undefined) => {
    if (style !== undefined) out["style"] = style;
  };
  switch (kind) {
    case "stroke":
      out["kind"] = "freedraw";
      out["id"] = id;
      out["points"] = node["points"] ?? [];
      out["size"] = node["size"] ?? 2.5;
      withStyle(styleFrom(node["color"], undefined));
      break;
    case "rect": {
      const [x, y, w, h] = [
        num(node, "x"),
        num(node, "y"),
        num(node, "width"),
        num(node, "height"),
      ];
      out["kind"] = "rect";
      out["id"] = id;
      out["x"] = w < 0 ? x + w : x;
      out["y"] = h < 0 ? y + h : y;
      out["width"] = Math.abs(w);
      out["height"] = Math.abs(h);
      withStyle(styleFrom(node["stroke"], node["fill"]));
      break;
    }
    case "ellipse": {
      const [cx, cy, rx, ry] = [
        num(node, "cx"),
        num(node, "cy"),
        num(node, "rx"),
        num(node, "ry"),
      ];
      out["kind"] = "ellipse";
      out["id"] = id;
      out["x"] = cx - Math.abs(rx);
      out["y"] = cy - Math.abs(ry);
      out["width"] = Math.abs(rx) * 2;
      out["height"] = Math.abs(ry) * 2;
      withStyle(styleFrom(node["stroke"], node["fill"]));
      break;
    }
    case "arrow":
      out["kind"] = "arrow";
      out["id"] = id;
      out["points"] = [node["from"] ?? { x: 0, y: 0 }, node["to"] ?? { x: 0, y: 0 }];
      if (node["label"] !== undefined && node["label"] !== null)
        out["label"] = node["label"];
      withStyle(styleFrom(node["color"], undefined));
      break;
    case "math":
      out["kind"] = "math";
      out["id"] = id;
      for (const key of ["x", "y", "tex", "size"]) {
        if (node[key] !== undefined) out[key] = node[key];
      }
      withStyle(styleFrom(node["color"], undefined));
      break;
    default:
      throw new Error(
        `invalid argument: unknown node kind \`${kind}\` in a version 1 figure`,
      );
  }
  return out;
}

/**
 * A parsed scene of any supported version as a v2 scene. Throws (with the
 * Rust plugin's wording) on newer versions, unknown v1 kinds and non-objects.
 */
export function migrateScene(value: unknown): FigureScene {
  if (!isObject(value))
    throw new Error("invalid argument: a figure must be a JSON object");
  const rawVersion = value["version"];
  let version = 1;
  if (rawVersion !== undefined && rawVersion !== null) {
    if (
      typeof rawVersion !== "number" ||
      !Number.isInteger(rawVersion) ||
      rawVersion < 0
    ) {
      throw new Error("invalid argument: `version` must be a whole number");
    }
    version = rawVersion;
  }
  if (version > SCENE_VERSION) {
    throw new Error(
      `invalid argument: this figure is format version ${version}; this Manang reads up to ${SCENE_VERSION}. Update Manang to open it.`,
    );
  }
  let scene: Json = value;
  if (version < 2) {
    const nodes = value["nodes"];
    if (nodes !== undefined && !Array.isArray(nodes)) {
      throw new Error("invalid argument: `nodes` must be an array");
    }
    scene = { ...value, version: SCENE_VERSION, nodes: (nodes ?? []).map(v1Node) };
  }
  const width = scene["width"];
  const height = scene["height"];
  if (
    typeof width !== "number" ||
    typeof height !== "number" ||
    !(width > 0) ||
    !(height > 0)
  ) {
    throw new Error("invalid argument: the page size must be positive and finite");
  }
  if (!Array.isArray(scene["nodes"]))
    throw new Error("JSON error: missing field `nodes`");
  return scene as unknown as FigureScene;
}
