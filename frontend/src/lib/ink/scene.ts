/**
 * Manang figure scene graph.
 *
 * Per ADR-0001 the canvas is a typed, JSON-serialisable scene rather than a
 * canvas library's internal model. Two properties follow, and both are load
 * bearing:
 *
 *  1. The Rust core can render any scene to deterministic SVG/PDF headlessly,
 *     because the format is plain data rather than a live drawing surface.
 *  2. The format is stable, diffable, and safe to commit to git.
 *
 * Geometry is stored in scene units (1 unit = 1 CSS pixel at 100% zoom) with
 * the origin at the top left, matching SVG's coordinate system so the Rust
 * renderer is a direct serialisation.
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

/** Where an arrow may magnetically attach — DESIGN.md §4.1. */
export type AnchorSide = "top" | "right" | "bottom" | "left";

export interface Anchor extends Vec {
  readonly side: AnchorSide;
  readonly nodeId: NodeId;
}

export interface StrokeNode {
  readonly kind: "stroke";
  readonly id: NodeId;
  readonly points: readonly StrokeSample[];
  /** Base diameter in scene units, before pressure thinning. */
  readonly size: number;
  readonly color: string;
}

export interface RectNode {
  readonly kind: "rect";
  readonly id: NodeId;
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  readonly stroke: string;
  readonly fill?: string;
}

export interface EllipseNode {
  readonly kind: "ellipse";
  readonly id: NodeId;
  readonly cx: number;
  readonly cy: number;
  readonly rx: number;
  readonly ry: number;
  readonly stroke: string;
  readonly fill?: string;
}

export interface ArrowNode {
  readonly kind: "arrow";
  readonly id: NodeId;
  readonly from: Vec;
  readonly to: Vec;
  readonly color: string;
  readonly label?: string;
}

/** A stamped formula. `tex` is the source; rendering happens downstream. */
export interface MathNode {
  readonly kind: "math";
  readonly id: NodeId;
  readonly x: number;
  readonly y: number;
  readonly tex: string;
  readonly size: number;
  readonly color: string;
}

export type FigureNode = StrokeNode | RectNode | EllipseNode | ArrowNode | MathNode;

export const SCENE_VERSION = 1;

export interface FigureScene {
  readonly version: typeof SCENE_VERSION;
  readonly width: number;
  readonly height: number;
  readonly nodes: readonly FigureNode[];
}

export function createScene(width: number, height: number): FigureScene {
  return { version: SCENE_VERSION, width, height, nodes: [] };
}

/* -------------------------------------------------------------------------- */
/* Node accessors                                                              */
/* -------------------------------------------------------------------------- */

/** Stroke outlines are derived, so they are excluded from hit bounds. */
export function isStroked(node: FigureNode): node is StrokeNode {
  return node.kind === "stroke";
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

/**
 * Ids are derived from a counter rather than randomness so that replaying the
 * same interaction sequence produces byte-identical scene JSON. Deterministic
 * output is a prerequisite for the headless Rust renderer being reproducible.
 */
export function formatNodeId(counter: number): NodeId {
  return `n${counter.toString(36)}`;
}
