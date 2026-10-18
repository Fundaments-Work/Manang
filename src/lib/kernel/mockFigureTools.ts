/**
 * The figure plugin's agent tools for the browser mock.
 *
 * Mirrors `crates/manang-figure/src/tools.rs` (same inputs, ids, error tags
 * and messages) so the editor's Mermaid paste and anything else calling the
 * tools behave in `pnpm dev` and verify-ui as in the desktop app. Two parts
 * are simpler than the Rust plugin, and say so: `figure.layout` ranks by
 * longest path instead of running dagre, and Mermaid layout comes from the
 * diagram mock's flowchart subset (`mockMermaidLayout`).
 */

import {
  frameChildren,
  nodeRect,
  sceneBounds,
  unionRect,
  nodeBounds,
} from "$lib/figure/geometry";
import { approxMath } from "$lib/figure/mathApprox";
import type { FigureOp } from "$lib/figure/ops";
import { labelWidth, LABEL_PADDING } from "$lib/figure/render";
import {
  connectorOf,
  formatNodeId,
  highestCounter,
  isBindable,
  labelOf,
  mergeStyle,
  type ConnectorNode,
  type FigureNode,
  type FigureScene,
  type Head,
  type MathLanguage,
  type Rect,
  type Routing,
  type ShapeNode,
  type Style,
  type Vec,
} from "$lib/figure/scene";
import { LINE_HEIGHT, naturalSize, wrap } from "$lib/figure/text";

type Args = Record<string, unknown>;

/** A tool failure: `[code] message`, as the Rust plugin formats it. */
export function toolError(code: string, message: string): { message: string } {
  return { message: `[${code}] ${message}` };
}

const invalid = (message: string) => toolError("invalid_argument", message);

function norm(s: string): string {
  return s.split(/\s+/).filter(Boolean).join(" ").toLowerCase();
}

function editDistance(a: string, b: string): number {
  const x = [...a];
  const y = [...b];
  let prev = Array.from({ length: y.length + 1 }, (_, i) => i);
  x.forEach((ca, i) => {
    const cur = [i + 1];
    y.forEach((cb, j) => {
      cur.push(
        Math.min(
          (prev[j] ?? 0) + (ca === cb ? 0 : 1),
          (prev[j + 1] ?? 0) + 1,
          (cur[j] ?? 0) + 1,
        ),
      );
    });
    prev = cur;
  });
  return prev[y.length] ?? 0;
}

/** The id a reference names: an id, else a unique label (shapes before arrows). */
export function resolveRef(scene: FigureScene, reference: string): string {
  if (scene.nodes.some((n) => n.id === reference)) return reference;
  const wanted = norm(reference);
  const matches = scene.nodes.filter((n) => {
    const l = labelOf(n);
    return l !== undefined && norm(l) === wanted;
  });
  const shapes = matches.filter((n) => connectorOf(n) === undefined);
  const pick = shapes.length > 0 ? shapes : matches;
  if (pick.length === 1 && pick[0] !== undefined) return pick[0].id;
  if (pick.length > 1) {
    throw toolError(
      "ambiguous",
      `"${reference}" is the label of ${pick.length} shapes (${pick.map((n) => n.id).join(", ")}); use an id`,
    );
  }
  let best: { d: number; id: string; label: string } | undefined;
  for (const n of scene.nodes) {
    const l = labelOf(n);
    if (l === undefined) continue;
    const d = editDistance(norm(l), wanted);
    if (best === undefined || d < best.d) best = { d, id: n.id, label: l };
  }
  const hint =
    best !== undefined && best.d <= Math.max(Math.floor(wanted.length / 3), 2)
      ? `; did you mean "${best.label}" (${best.id})?`
      : "; call figure_describe to see the shapes";
  throw toolError("not_found", `no shape has the id or label "${reference}"${hint}`);
}

const PRESETS: Record<string, Style> = {
  clean: { roughness: 0, strokeStyle: "solid", fontFamily: "sans" },
  sketch: { roughness: 1 },
  emphasis: { stroke: "accent", strokeWidth: 2.5 },
  muted: { stroke: "muted", opacity: 80 },
  dashed: { strokeStyle: "dashed" },
  blueprint: { stroke: "blue", fill: "none", fontFamily: "mono", roughness: 0 },
  warning: { stroke: "vermillion", fill: "vermillion" },
  success: { stroke: "green", fill: "green" },
};

export function preset(name: string): Style {
  const style = PRESETS[name];
  if (style === undefined) {
    throw invalid(
      `unknown preset \`${name}\`; use clean, sketch, emphasis, muted, dashed, blueprint, warning or success`,
    );
  }
  return style;
}

const SHAPE_KINDS = ["rect", "diamond", "ellipse", "text", "frame", "mermaid"] as const;

/** Default font size of a formula (`FORMULA_SIZE`). */
const FORMULA_SIZE = 24;

function mathLanguage(name: string | undefined): MathLanguage {
  if (name === undefined || name === "typst") return "typst";
  if (name === "latex") return "latex";
  throw invalid(`unknown math language \`${name}\`; use typst or latex`);
}

/**
 * Renders a formula the way the plugin does for a tool (measuring it; a
 * broken formula is a positioned `[math_error]`), here with the browser
 * mock's approximate renderer (`mathApprox.ts`).
 */
function measureFormula(
  id: string,
  language: MathLanguage,
  source: string,
  size: number,
): { width: number; height: number } {
  try {
    const m = approxMath(language, source, size);
    return { width: m.width, height: m.height };
  } catch (error) {
    const e = error as { message?: string; line?: number; column?: number };
    const place =
      e.line === undefined ? "" : ` (line ${e.line}, column ${e.column ?? 1})`;
    throw toolError(
      "math_error",
      `formula \`${id}\`: ${e.message ?? String(error)}${place}`,
    );
  }
}

function round2(v: number): number {
  return Math.round(v * 100) / 100;
}

export function shapeSummary(node: FigureNode): Record<string, unknown> {
  const r = nodeRect(node);
  const out: Record<string, unknown> = {
    id: node.id,
    kind: node.kind,
    x: round2(r.x),
    y: round2(r.y),
    width: round2(r.width),
    height: round2(r.height),
  };
  if (node.kind === "math") {
    out["source"] = node.source;
    out["language"] = node.language ?? "typst";
    out["size"] = node.size;
  } else {
    const label = labelOf(node);
    if (label !== undefined) out["label"] = label;
  }
  if ((node.groups ?? []).length > 0) out["groups"] = node.groups;
  return out;
}

function autoSize(
  kind: string,
  label: string | undefined,
  style: Style,
): [number, number] {
  const size = style.fontSize ?? 16;
  const family = style.fontFamily ?? "sans";
  const text = label ?? "";
  const natural = naturalSize(text, size, family);
  if (kind === "text")
    return [Math.max(natural.width, 8), Math.max(natural.height, size * LINE_HEIGHT)];
  if (kind === "frame" || kind === "mermaid") return [320, 200];
  let width = Math.min(Math.max(natural.width + 2 * LABEL_PADDING + 24, 120), 280);
  if (kind === "diamond") width = Math.max(width * 1.6, 140);
  else if (kind === "ellipse") width *= 1.3;
  const lines = wrap(text, labelWidth(kind, width), size, family).length;
  let height = Math.max(lines * size * LINE_HEIGHT + 2 * LABEL_PADDING + 16, 60);
  if (kind === "diamond") height = Math.max(height * 1.6, 90);
  return [width, height];
}

function list(args: Args, key: string): unknown[] {
  const value = args[key];
  if (!Array.isArray(value)) throw invalid(`\`${key}\` must be an array`);
  return value;
}

function obj(value: unknown, where: string): Args {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw invalid(`${where} must be an object`);
  }
  return value as Args;
}

function optNum(a: Args, key: string): number | undefined {
  const v = a[key];
  if (v === undefined || v === null) return undefined;
  if (typeof v !== "number" || !Number.isFinite(v))
    throw invalid(`\`${key}\` must be a number`);
  return v;
}

function optStr(a: Args, key: string): string | undefined {
  const v = a[key];
  if (v === undefined || v === null) return undefined;
  if (typeof v !== "string") throw invalid(`\`${key}\` must be a string`);
  return v;
}

export interface ToolEdit {
  readonly op: FigureOp;
  readonly ids: string[];
}

export function addShapes(
  scene: FigureScene,
  args: Args,
): ToolEdit & { shapes: FigureNode[] } {
  const shapes = list(args, "shapes");
  if (shapes.length === 0) throw invalid("`shapes` must list at least one shape");
  const bounds = sceneBounds(scene);
  const empty = scene.nodes.length === 0;
  const rowX = empty ? 40 : bounds.x + bounds.width + 60;
  const rowY = empty ? 40 : bounds.y;
  let x = rowX;
  let y = rowY;
  let rowH = 0;
  let inRow = 0;
  let counter = highestCounter(scene);
  const nodes: FigureNode[] = [];
  for (const raw of shapes) {
    const s = obj(raw, "each shape");
    const kind = optStr(s, "kind") ?? "rect";
    if (kind === "formula") {
      const source = optStr(s, "source");
      if (source === undefined || source.trim() === "") {
        throw invalid(
          "a formula needs `source` (Typst or LaTeX math, without $ signs)",
        );
      }
      const language = mathLanguage(optStr(s, "language"));
      const size = optNum(s, "size") ?? FORMULA_SIZE;
      const style: Style =
        s["style"] === undefined ? {} : (obj(s["style"], "`style`") as Style);
      const guess = {
        width: Math.max(source.length * size * 0.6, size),
        height: size * 1.4,
      };
      const px = optNum(s, "x");
      const py = optNum(s, "y");
      let at: Vec;
      if (px !== undefined && py !== undefined) {
        at = { x: px, y: py };
      } else {
        if (inRow === 5) {
          x = rowX;
          y += rowH + 60;
          rowH = 0;
          inRow = 0;
        }
        at = { x: px ?? x, y: py ?? y };
        x += guess.width + 60;
        rowH = Math.max(rowH, guess.height);
        inRow += 1;
      }
      counter += 1;
      const id = formatNodeId(counter);
      const box = measureFormula(id, language, source, size);
      nodes.push({
        kind: "math",
        id,
        x: at.x,
        y: at.y,
        source,
        ...(language === "typst" ? {} : { language }),
        size,
        width: box.width,
        height: box.height,
        ...(Object.keys(style).length === 0 ? {} : { style }),
      });
      continue;
    }
    if (!(SHAPE_KINDS as readonly string[]).includes(kind)) {
      throw invalid(
        `unknown shape kind \`${kind}\`; use ${SHAPE_KINDS.join(", ")}, formula`,
      );
    }
    let style: Style =
      optStr(s, "preset") !== undefined ? preset(optStr(s, "preset") ?? "") : {};
    if (s["style"] !== undefined)
      style = mergeStyle(style, obj(s["style"], "`style`") as Style);
    if (style.fill === undefined && ["rect", "diamond", "ellipse"].includes(kind)) {
      style = { ...style, fill: "surface" };
    }
    const label = optStr(s, "label");
    const [aw, ah] = autoSize(kind, label, style);
    const w = optNum(s, "width") ?? aw;
    const h = optNum(s, "height") ?? ah;
    const px = optNum(s, "x");
    const py = optNum(s, "y");
    let at: Vec;
    if (px !== undefined && py !== undefined) {
      at = { x: px, y: py };
    } else {
      if (inRow === 5) {
        x = rowX;
        y += rowH + 60;
        rowH = 0;
        inRow = 0;
      }
      at = { x: px ?? x, y: py ?? y };
      x += w + 60;
      rowH = Math.max(rowH, h);
      inRow += 1;
    }
    counter += 1;
    const node: ShapeNode = {
      kind: kind as ShapeNode["kind"],
      id: formatNodeId(counter),
      x: at.x,
      y: at.y,
      width: w,
      height: h,
      ...(label === undefined ? {} : { label }),
      ...(Object.keys(style).length === 0 ? {} : { style }),
      ...(kind === "mermaid"
        ? {
            source:
              optStr(s, "source") ??
              (() => {
                throw invalid("a mermaid shape needs `source` (the Mermaid text)");
              })(),
          }
        : {}),
    };
    nodes.push(node);
  }
  return { op: { op: "add", nodes }, ids: nodes.map((n) => n.id), shapes: nodes };
}

export function connect(scene: FigureScene, args: Args): ToolEdit {
  const items = list(args, "connections");
  if (items.length === 0)
    throw invalid("`connections` must list at least one connection");
  let counter = highestCounter(scene);
  const nodes: FigureNode[] = [];
  for (const raw of items) {
    const c = obj(raw, "each connection");
    const from = resolveRef(scene, optStr(c, "from") ?? "");
    const to = resolveRef(scene, optStr(c, "to") ?? "");
    for (const id of [from, to]) {
      const n = scene.nodes.find((m) => m.id === id);
      if (n !== undefined && !isBindable(n)) {
        throw invalid(`\`${id}\` is an arrow or a drawing; connect shapes`);
      }
    }
    counter += 1;
    const center = (id: string): Vec => {
      const n = scene.nodes.find((m) => m.id === id);
      const r = n === undefined ? { x: 0, y: 0, width: 0, height: 0 } : nodeRect(n);
      return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
    };
    let style: Style =
      optStr(c, "preset") !== undefined ? preset(optStr(c, "preset") ?? "") : {};
    if (c["style"] !== undefined)
      style = mergeStyle(style, obj(c["style"], "`style`") as Style);
    const { fill: _fill, ...unfilled } = style;
    const kind = optStr(c, "kind") ?? "arrow";
    if (kind !== "arrow" && kind !== "line") {
      throw invalid(`unknown connection kind \`${kind}\`; use arrow or line`);
    }
    const label = optStr(c, "label");
    const routing = optStr(c, "routing") as Routing | undefined;
    const heads = c["heads"] as ConnectorNode["heads"] | undefined;
    const node: ConnectorNode = {
      kind,
      id: formatNodeId(counter),
      points: [center(from), center(to)],
      start: { node: from },
      end: { node: to },
      ...(routing === undefined || routing === "straight" ? {} : { routing }),
      ...(heads === undefined ? {} : { heads }),
      ...(label === undefined ? {} : { label }),
      ...(Object.keys(unfilled).length === 0 ? {} : { style: unfilled }),
    };
    nodes.push(node);
  }
  return { op: { op: "add", nodes }, ids: nodes.map((n) => n.id) };
}

export function update(scene: FigureScene, args: Args): ToolEdit {
  const ops: FigureOp[] = [];
  const ids: string[] = [];
  for (const raw of list(args, "updates")) {
    const c = obj(raw, "each update");
    const id = resolveRef(scene, optStr(c, "ref") ?? "");
    const node = scene.nodes.find((n) => n.id === id);
    if (node === undefined) throw invalid(`\`${id}\` vanished`);
    const set: Record<string, unknown> = {};
    for (const key of ["label", "kind", "angle", "routing", "source", "style"]) {
      if (c[key] !== undefined) set[key] = c[key];
    }
    if (c["language"] !== undefined || c["size"] !== undefined) {
      if (node.kind !== "math") {
        throw invalid(
          `\`${id}\` is not a formula; only formulas have \`language\` and \`size\``,
        );
      }
      if (c["language"] !== undefined)
        set["language"] = mathLanguage(optStr(c, "language"));
      if (c["size"] !== undefined) set["size"] = c["size"];
    }
    if (node.kind === "math" && ["source", "language", "size"].some((k) => k in set)) {
      const next = {
        language:
          (set["language"] as MathLanguage | undefined) ?? node.language ?? "typst",
        source: (set["source"] as string | undefined) ?? node.source,
        size: (set["size"] as number | undefined) ?? node.size,
      };
      const box = measureFormula(id, next.language, next.source, next.size);
      set["width"] = box.width;
      set["height"] = box.height;
    }
    if (
      typeof set["kind"] === "string" &&
      (!("x" in node) ||
        node.kind === "math" ||
        !(SHAPE_KINDS as readonly string[]).includes(set["kind"]))
    ) {
      throw invalid(
        `\`${id}\` cannot become a \`${set["kind"]}\`; shapes switch between ${SHAPE_KINDS.join(", ")}`,
      );
    }
    if (Object.keys(set).length > 0) ops.push({ op: "update", id, set });
    const box = ["x", "y", "width", "height"].map((k) => optNum(c, k));
    if (box.some((v) => v !== undefined)) {
      const r = nodeRect(node);
      ops.push({
        op: "resize",
        id,
        x: box[0] ?? r.x,
        y: box[1] ?? r.y,
        width: box[2] ?? r.width,
        height: box[3] ?? r.height,
      });
    }
    ids.push(id);
  }
  if (ops.length === 0) {
    throw invalid(
      "nothing to change: give label, kind, x, y, width, height, angle, style, routing, source, language or size",
    );
  }
  return { op: { op: "batch", ops }, ids };
}

export function remove(scene: FigureScene, args: Args): ToolEdit {
  const ids = list(args, "refs").map((r) => resolveRef(scene, String(r)));
  return { op: { op: "remove", ids }, ids };
}

export function style(scene: FigureScene, args: Args): ToolEdit {
  let patch: Style =
    optStr(args, "preset") !== undefined ? preset(optStr(args, "preset") ?? "") : {};
  if (args["style"] !== undefined)
    patch = mergeStyle(patch, obj(args["style"], "`style`") as Style);
  if (Object.keys(patch).length === 0) throw invalid("give a `preset` or a `style`");
  const ids =
    args["refs"] === undefined
      ? scene.nodes.map((n) => n.id)
      : list(args, "refs").map((r) => resolveRef(scene, String(r)));
  return { op: { op: "set-style", ids, style: patch }, ids };
}

type Direction = "TB" | "LR" | "BT" | "RL";

/** A simple layered layout: longest-path ranks, insertion order within a rank. */
export function layout(scene: FigureScene, args: Args): ToolEdit {
  const direction = (optStr(args, "direction") ?? "TB") as Direction;
  if (!["TB", "LR", "BT", "RL"].includes(direction)) {
    throw invalid("`direction` must be TB, LR, BT or RL");
  }
  const spacing = optNum(args, "spacing") ?? 60;
  const only =
    args["refs"] === undefined
      ? undefined
      : new Set(list(args, "refs").map((r) => resolveRef(scene, String(r))));
  const members = scene.nodes.filter(
    (n) =>
      isBindable(n) &&
      n.kind !== "frame" &&
      n.locked !== true &&
      (only === undefined || only.has(n.id)),
  );
  const ids = new Set(members.map((n) => n.id));
  const edges: [string, string][] = [];
  for (const n of scene.nodes) {
    const c = connectorOf(n);
    if (
      c?.start !== undefined &&
      c.end !== undefined &&
      c.start.node !== c.end.node &&
      ids.has(c.start.node) &&
      ids.has(c.end.node)
    ) {
      edges.push([c.start.node, c.end.node]);
    }
  }
  const rank = new Map(members.map((n) => [n.id, 0]));
  for (let round = 0; round < members.length; round += 1) {
    for (const [a, b] of edges) {
      const next = (rank.get(a) ?? 0) + 1;
      if (next > (rank.get(b) ?? 0) && next < members.length) rank.set(b, next);
    }
  }
  const ranks = new Map<number, FigureNode[]>();
  for (const n of members) {
    const r = rank.get(n.id) ?? 0;
    ranks.set(r, [...(ranks.get(r) ?? []), n]);
  }
  const horizontal = direction === "LR" || direction === "RL";
  const reversed = direction === "BT" || direction === "RL";
  const order = [...ranks.keys()].sort((a, b) => (reversed ? b - a : a - b));
  const before = members.reduce<Rect>((acc, n) => unionRect(acc, nodeRect(n)), {
    x: 0,
    y: 0,
    width: 0,
    height: 0,
  });
  const origin =
    members.length > 0
      ? {
          x: Math.min(...members.map((n) => nodeRect(n).x)),
          y: Math.min(...members.map((n) => nodeRect(n).y)),
        }
      : { x: before.x, y: before.y };
  const placed = new Map<string, Vec>();
  let along = 0;
  for (const r of order) {
    const row = ranks.get(r) ?? [];
    const depth = Math.max(
      ...row.map((n) => (horizontal ? nodeRect(n).width : nodeRect(n).height)),
    );
    const total =
      row.reduce(
        (s, n) => s + (horizontal ? nodeRect(n).height : nodeRect(n).width),
        0,
      ) +
      spacing * (row.length - 1);
    let across = -total / 2;
    for (const n of row) {
      const box = nodeRect(n);
      const size = horizontal ? box.height : box.width;
      const thick = horizontal ? box.width : box.height;
      const a = along + (depth - thick) / 2;
      placed.set(n.id, horizontal ? { x: a, y: across } : { x: across, y: a });
      across += size + spacing;
    }
    along += depth + spacing * 1.2;
  }
  const minX = Math.min(...[...placed.values()].map((p) => p.x));
  const minY = Math.min(...[...placed.values()].map((p) => p.y));
  const ops: FigureOp[] = [];
  const moved: string[] = [];
  for (const n of members) {
    const p = placed.get(n.id);
    if (p === undefined) continue;
    const r = nodeRect(n);
    const dx = p.x - minX + origin.x - r.x;
    const dy = p.y - minY + origin.y - r.y;
    if (Math.abs(dx) > 1e-6 || Math.abs(dy) > 1e-6) {
      ops.push({ op: "move", ids: [n.id], dx, dy });
      moved.push(n.id);
    }
  }
  const routing = optStr(args, "routing") as Routing | undefined;
  if (routing !== undefined) {
    for (const n of scene.nodes) {
      const c = connectorOf(n);
      if (
        c?.start !== undefined &&
        c.end !== undefined &&
        (c.routing ?? "straight") !== routing
      ) {
        ops.push({ op: "update", id: c.id, set: { routing } });
      }
    }
  }
  return { op: { op: "batch", ops }, ids: moved };
}

export function describe(scene: FigureScene): Record<string, unknown> {
  const shapes: Record<string, unknown>[] = [];
  const connections: Record<string, unknown>[] = [];
  const frames: Record<string, unknown>[] = [];
  const other: Record<string, unknown>[] = [];
  const sentences: string[] = [];
  const name = (id: string): string => {
    const n = scene.nodes.find((m) => m.id === id);
    const l = n === undefined ? undefined : labelOf(n);
    return l === undefined ? id : `"${l.replaceAll("\n", " ")}" (${id})`;
  };
  for (const n of scene.nodes) {
    const c = connectorOf(n);
    if (c !== undefined) {
      const v: Record<string, unknown> = { id: c.id, kind: c.kind };
      if (c.start !== undefined) v["from"] = c.start.node;
      if (c.end !== undefined) v["to"] = c.end.node;
      if (c.label !== undefined) v["label"] = c.label;
      if ((c.routing ?? "straight") !== "straight") v["routing"] = c.routing;
      if (c.start !== undefined && c.end !== undefined) {
        sentences.push(
          `${name(c.start.node)} -> ${name(c.end.node)}${c.label === undefined ? "" : ` labelled "${c.label}"`}`,
        );
      }
      connections.push(v);
    } else if (n.kind === "frame") {
      frames.push({ ...shapeSummary(n), contains: frameChildren(scene, n.id) });
    } else if (n.kind === "freedraw" || n.kind === "math" || n.kind === "image") {
      other.push(shapeSummary(n));
    } else {
      shapes.push(shapeSummary(n));
    }
  }
  const plural = (k: number, word: string) => `${k} ${word}${k === 1 ? "" : "s"}`;
  let summary = `${plural(shapes.length, "shape")}, ${plural(connections.length, "connection")}, ${plural(frames.length, "frame")}, ${plural(other.length, "other element")}.`;
  if (sentences.length > 0) summary += ` Connections: ${sentences.join("; ")}.`;
  const b = sceneBounds(scene);
  return {
    page: { width: scene.width, height: scene.height },
    bounds: {
      x: round2(b.x),
      y: round2(b.y),
      width: round2(b.width),
      height: round2(b.height),
    },
    shapes,
    connections,
    frames,
    other,
    summary,
  };
}

/** Where converted Mermaid lands: the given point, else below the content. */
export function insertionPoint(scene: FigureScene, x?: number, y?: number): Vec {
  if (x !== undefined && y !== undefined) return { x, y };
  if (scene.nodes.length === 0) return { x: x ?? 40, y: y ?? 40 };
  const b = scene.nodes.reduce<Rect>((acc, n) => unionRect(acc, nodeBounds(n)), {
    x: 0,
    y: 0,
    width: 0,
    height: 0,
  });
  return { x: x ?? b.x, y: y ?? b.y + b.height + 60 };
}

/** `diagram.layout` output to native nodes (mirror of `mermaid.rs`). */
export function nodesFromLayout(
  layout: Args,
  origin: Vec,
  start: number,
): FigureNode[] {
  const arr = (key: string) =>
    Array.isArray(layout[key]) ? (layout[key] as Args[]) : [];
  const nodes = arr("nodes");
  const edges = arr("edges");
  const clusters = arr("clusters");
  if (nodes.length === 0) throw invalid("the diagram has no nodes to convert");
  const n = (v: Args, k: string) => (typeof v[k] === "number" ? (v[k] as number) : 0);
  const minX = Math.min(...[...nodes, ...clusters].map((v) => n(v, "x")));
  const minY = Math.min(...[...nodes, ...clusters].map((v) => n(v, "y")));
  const dx = origin.x - minX;
  const dy = origin.y - minY;
  let counter = start;
  const next = () => formatNodeId(++counter);
  const out: FigureNode[] = [];
  for (const c of clusters) {
    const title = typeof c["title"] === "string" ? c["title"] : undefined;
    out.push({
      kind: "frame",
      id: next(),
      x: n(c, "x") + dx,
      y: n(c, "y") + dy,
      width: n(c, "width"),
      height: n(c, "height"),
      ...(title ? { label: title } : {}),
    });
  }
  const ids = new Map<string, string>();
  for (const v of nodes) {
    if (typeof v["id"] !== "string") continue;
    const id = next();
    const shape = typeof v["shape"] === "string" ? v["shape"] : "rect";
    const fill =
      shape === "start" || shape === "end"
        ? "ink"
        : shape === "note"
          ? "yellow"
          : "surface";
    const kind =
      shape === "diamond"
        ? "diamond"
        : ["circle", "start", "end"].includes(shape)
          ? "ellipse"
          : "rect";
    const label =
      typeof v["label"] === "string" && v["label"] !== "" ? v["label"] : undefined;
    out.push({
      kind,
      id,
      x: n(v, "x") + dx,
      y: n(v, "y") + dy,
      width: Math.max(n(v, "width"), 1),
      height: Math.max(n(v, "height"), 1),
      ...(label ? { label } : {}),
      style: { fill },
    });
    ids.set(v["id"], id);
  }
  for (const e of edges) {
    const points = (Array.isArray(e["points"]) ? (e["points"] as Args[]) : []).map(
      (p) => ({ x: n(p, "x") + dx, y: n(p, "y") + dy }),
    );
    if (points.length < 2) continue;
    const from = typeof e["from"] === "string" ? ids.get(e["from"]) : undefined;
    const to = typeof e["to"] === "string" ? ids.get(e["to"]) : undefined;
    const startHead = (typeof e["start"] === "string" ? e["start"] : "none") as Head;
    const endHead = (typeof e["end"] === "string" ? e["end"] : "none") as Head;
    const label =
      typeof e["label"] === "string" && e["label"] !== "" ? e["label"] : undefined;
    const base = {
      id: next(),
      points,
      ...(from ? { start: { node: from } } : {}),
      ...(to ? { end: { node: to } } : {}),
      ...(points.length > 2 ? { routing: "curved" as const } : {}),
      ...(label ? { label } : {}),
      ...(e["dashed"] === true ? { style: { strokeStyle: "dashed" as const } } : {}),
    };
    if (startHead === "none" && endHead === "none") {
      out.push({ kind: "line", ...base });
    } else {
      const heads = {
        ...(startHead !== "none" ? { start: startHead } : {}),
        ...(endHead !== "arrow" ? { end: endHead } : {}),
      };
      out.push({
        kind: "arrow",
        ...base,
        ...(Object.keys(heads).length > 0 ? { heads } : {}),
      });
    }
  }
  return out;
}

function mermaidLabel(label: string): string {
  return label
    .replaceAll('"', "#quot;")
    .replaceAll("\n", "<br/>")
    .replace(/[[\]{}()|]/g, " ")
    .trim();
}

/** Mirror of `mermaid::to_mermaid`. */
export function toMermaid(scene: FigureScene): { source: string; skipped: string[] } {
  const b = sceneBounds(scene);
  const lines = [`flowchart ${b.width > b.height * 1.3 ? "LR" : "TD"}`];
  const skipped: string[] = [];
  const included = new Set<string>();
  const shapes = scene.nodes
    .filter((n) => n.kind === "rect" || n.kind === "diamond" || n.kind === "ellipse")
    .sort(
      (p, q) => nodeBounds(p).y - nodeBounds(q).y || nodeBounds(p).x - nodeBounds(q).x,
    );
  for (const n of shapes) {
    const label = mermaidLabel(labelOf(n) ?? n.id);
    lines.push(
      n.kind === "diamond"
        ? `  ${n.id}{"${label}"}`
        : n.kind === "ellipse"
          ? `  ${n.id}(("${label}"))`
          : `  ${n.id}["${label}"]`,
    );
    included.add(n.id);
  }
  for (const n of scene.nodes) {
    if (n.kind === "rect" || n.kind === "diamond" || n.kind === "ellipse") continue;
    const c = connectorOf(n);
    if (
      c?.start !== undefined &&
      c.end !== undefined &&
      included.has(c.start.node) &&
      included.has(c.end.node)
    ) {
      const dashed = (c.style?.strokeStyle ?? "solid") !== "solid";
      const link =
        c.kind === "arrow" ? (dashed ? "-.->" : "-->") : dashed ? "-.-" : "---";
      const label = c.label === undefined ? "" : `|"${mermaidLabel(c.label)}"|`;
      lines.push(`  ${c.start.node} ${link}${label} ${c.end.node}`);
    } else {
      skipped.push(n.id);
    }
  }
  return { source: `${lines.join("\n")}\n`, skipped };
}

/** True when `text` reads as Mermaid (its first meaningful line names a diagram). */
export function looksLikeMermaid(text: string): boolean {
  const first = text
    .split("\n")
    .map((l) => l.trim())
    .find((l) => l !== "" && !l.startsWith("%%"));
  return (
    first !== undefined &&
    /^(flowchart|graph|sequenceDiagram|stateDiagram(-v2)?|classDiagram|erDiagram|gantt|pie|mindmap|journey|gitGraph|timeline|quadrantChart|requirementDiagram|C4Context|sankey-beta|xychart-beta|block-beta|packet-beta|kanban|architecture-beta)\b/.test(
      first,
    )
  );
}

export { highestCounter };
