/**
 * What each node draws, as plain primitives: the TypeScript side of
 * `crates/manang-figure/src/svg.rs`. The canvas renders these primitives
 * with Svelte and `renderSceneSvg` serialises them, so the editor draws the
 * same geometry (labels, hand-drawn strokes, heads) the export writes.
 */

import { endDirections } from "./binding";
import {
  formatCoord,
  freedrawPath,
  nodeBounds,
  normalizeRect,
  unionRect,
  EMPTY_RECT,
  rectContainsRect,
} from "./geometry";
import { fillPaint, strokeColour, type FigureTheme } from "./palette";
import {
  Rng,
  connectorPath,
  diamondPoints,
  ellipse,
  polygon,
  rectPoints,
} from "./sketch";
import { LINE_HEIGHT, wrap } from "./text";
import {
  styleDefaults as sd,
  type ConnectorNode,
  type FigureNode,
  type FigureScene,
  type FontFamily,
  type Head,
  type Rect,
  type ShapeNode,
  type Style,
  type Vec,
} from "./scene";

export const CONNECTOR_FONT_SIZE = 14;
export const LABEL_PADDING = 8;

export type Primitive =
  | {
      readonly type: "path";
      readonly d: string;
      readonly fill: string;
      readonly fillOpacity?: number;
      readonly stroke?: string;
      readonly strokeWidth?: number;
      readonly dash?: string;
      readonly linecap?: "round";
      readonly linejoin?: "round";
    }
  | {
      readonly type: "rect";
      readonly x: number;
      readonly y: number;
      readonly width: number;
      readonly height: number;
      readonly fill: string;
      readonly fillOpacity?: number;
      readonly stroke: string;
      readonly dash?: string;
    }
  | {
      readonly type: "circle";
      readonly cx: number;
      readonly cy: number;
      readonly r: number;
      readonly fill: string;
    }
  | {
      readonly type: "image";
      readonly href: string;
      readonly x: number;
      readonly y: number;
      readonly width: number;
      readonly height: number;
    }
  | {
      readonly type: "text";
      readonly fill: string;
      readonly family: string;
      readonly size: number;
      readonly anchor: "start" | "middle" | "end";
      readonly halo?: string;
      readonly lines: readonly {
        readonly x: number;
        readonly y: number;
        readonly text: string;
      }[];
    }
  | {
      readonly type: "embed";
      readonly id: string;
      readonly x: number;
      readonly y: number;
      readonly width: number;
      readonly height: number;
    };

export interface Visual {
  readonly id: string;
  readonly rotate?: {
    readonly angle: number;
    readonly cx: number;
    readonly cy: number;
  };
  readonly opacity?: number;
  readonly items: readonly Primitive[];
}

export function fontStack(family: FontFamily): string {
  return family === "mono" ? "DM Mono, monospace" : "DM Sans, sans-serif";
}

/** Width a label may use inside a box of `width` (`label_width`). */
export function labelWidth(kind: string, width: number): number {
  if (kind === "text") return Math.abs(width);
  const usable =
    kind === "diamond"
      ? width * 0.55
      : kind === "ellipse"
        ? width * 0.72
        : Math.abs(width);
  return Math.max(usable - 2 * LABEL_PADDING, 1);
}

function dash(style: Style): string | undefined {
  const w = sd.strokeWidth(style);
  switch (sd.strokeStyle(style)) {
    case "solid":
      return undefined;
    case "dashed":
      return `${formatCoord(w * 4 + 2)} ${formatCoord(w * 3 + 2)}`;
    case "dotted":
      return `${formatCoord(w * 0.5)} ${formatCoord(w * 2.5 + 1)}`;
  }
}

function fillOf(
  style: Style,
  theme: FigureTheme,
): { fill: string; fillOpacity?: number } {
  const paint = fillPaint(sd.fill(style), theme);
  return paint.alpha < 1
    ? { fill: paint.colour, fillOpacity: paint.alpha }
    : { fill: paint.colour };
}

function group(
  id: string,
  style: Style,
  items: Primitive[],
  rotate?: { angle: number; c: Vec },
): Visual {
  const opacity = sd.opacity(style);
  return {
    id,
    ...(rotate !== undefined && rotate.angle !== 0
      ? { rotate: { angle: rotate.angle, cx: rotate.c.x, cy: rotate.c.y } }
      : {}),
    ...(opacity < 1 ? { opacity } : {}),
    items,
  };
}

function textBlock(
  lines: readonly string[],
  x: number,
  top: number,
  size: number,
  family: FontFamily,
  anchor: "start" | "middle" | "end",
  fill: string,
  halo?: string,
): Primitive {
  const lh = size * LINE_HEIGHT;
  return {
    type: "text",
    fill,
    family: fontStack(family),
    size,
    anchor,
    ...(halo === undefined ? {} : { halo }),
    lines: lines.map((text, i) => ({
      x,
      y: top + lh * i + lh / 2 + size * 0.35,
      text,
    })),
  };
}

function label(
  kind: string,
  s: ShapeNode,
  text: string,
  theme: FigureTheme,
): Primitive {
  const r = normalizeRect(s.x, s.y, s.width, s.height);
  const style = s.style ?? {};
  const size = sd.fontSize(style);
  const family = sd.fontFamily(style);
  const lines = wrap(text, labelWidth(kind, r.width), size, family);
  const total = size * LINE_HEIGHT * lines.length;
  const top = kind === "text" ? r.y : r.y + r.height / 2 - total / 2;
  const align = style.textAlign ?? (kind === "text" ? "left" : "center");
  const pad = kind === "text" ? 0 : LABEL_PADDING;
  const [x, anchor] =
    align === "left"
      ? [r.x + pad, "start" as const]
      : align === "center"
        ? [r.x + r.width / 2, "middle" as const]
        : [r.x + r.width - pad, "end" as const];
  return textBlock(
    lines,
    x,
    top,
    size,
    family,
    anchor,
    strokeColour(sd.stroke(style), theme),
  );
}

/** The point halfway along a polyline. */
export function pathMidpoint(points: readonly Vec[]): Vec {
  let total = 0;
  for (let i = 0; i + 1 < points.length; i += 1) {
    const [a, b] = [points[i] as Vec, points[i + 1] as Vec];
    total += Math.hypot(b.x - a.x, b.y - a.y);
  }
  let left = total / 2;
  for (let i = 0; i + 1 < points.length; i += 1) {
    const [a, b] = [points[i] as Vec, points[i + 1] as Vec];
    const len = Math.hypot(b.x - a.x, b.y - a.y);
    if (len >= left && len > 0) {
      const t = left / len;
      return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
    }
    left -= len;
  }
  return points[0] ?? { x: 0, y: 0 };
}

/** The primitive of one arrow head at `tip` pointing along `dir`. */
export function headPrimitive(
  head: Head,
  tip: Vec,
  dir: Vec,
  width: number,
  colour: string,
): Primitive | undefined {
  const len = Math.hypot(dir.x, dir.y);
  if (head === "none" || len < 1e-9) return undefined;
  const [ux, uy] = [dir.x / len, dir.y / len];
  const size = 7 + width * 2;
  const back = (k: number, spread: number): Vec => {
    const s = Math.sin(spread);
    const c = Math.cos(spread);
    const [bx, by] = [-ux, -uy];
    return {
      x: tip.x + size * k * (bx * c - by * s),
      y: tip.y + size * k * (bx * s + by * c),
    };
  };
  const spread = Math.PI / 7;
  const f = formatCoord;
  switch (head) {
    case "arrow": {
      const [a, b] = [back(1, spread), back(1, -spread)];
      return {
        type: "path",
        d: `M${f(a.x)} ${f(a.y)}L${f(tip.x)} ${f(tip.y)}L${f(b.x)} ${f(b.y)}`,
        fill: "none",
        stroke: colour,
        strokeWidth: width,
        linecap: "round",
        linejoin: "round",
      };
    }
    case "triangle": {
      const [a, b] = [back(1, spread), back(1, -spread)];
      return {
        type: "path",
        d: `M${f(tip.x)} ${f(tip.y)}L${f(a.x)} ${f(a.y)}L${f(b.x)} ${f(b.y)}Z`,
        fill: colour,
        stroke: colour,
        strokeWidth: width,
        linejoin: "round",
      };
    }
    case "dot": {
      const r = size / 3;
      return {
        type: "circle",
        cx: tip.x - ux * r,
        cy: tip.y - uy * r,
        r,
        fill: colour,
      };
    }
    case "bar": {
      const h = size / 2;
      return {
        type: "path",
        d: `M${f(tip.x - uy * h)} ${f(tip.y + ux * h)}L${f(tip.x + uy * h)} ${f(tip.y - ux * h)}`,
        fill: "none",
        stroke: colour,
        strokeWidth: width,
        linecap: "round",
      };
    }
  }
}

function shapeVisual(s: ShapeNode, theme: FigureTheme): Visual {
  const r = normalizeRect(s.x, s.y, s.width, s.height);
  const style = s.style ?? {};
  const roughness = sd.roughness(style);
  const rng = Rng.forId(s.id);
  const outline = (rough: number) =>
    s.kind === "diamond"
      ? polygon(diamondPoints(r), rough, rng)
      : s.kind === "ellipse"
        ? ellipse(r, rough, rng)
        : polygon(rectPoints(r), rough, rng);
  const clean = outline(0);
  const stroke = strokeColour(sd.stroke(style), theme);
  const strokeWidth = sd.strokeWidth(style);
  const d = dash(style);
  const items: Primitive[] = [];
  if (roughness === 0) {
    items.push({
      type: "path",
      d: clean,
      ...fillOf(style, theme),
      stroke,
      strokeWidth,
      ...(d === undefined ? {} : { dash: d }),
      ...(sd.strokeStyle(style) === "dotted" ? { linecap: "round" as const } : {}),
    });
  } else {
    if (sd.fill(style) !== "none")
      items.push({ type: "path", d: clean, ...fillOf(style, theme) });
    items.push({
      type: "path",
      d: outline(roughness),
      fill: "none",
      stroke,
      strokeWidth,
      linecap: "round",
      ...(d === undefined ? {} : { dash: d }),
    });
  }
  if (s.label !== undefined) items.push(label(s.kind, s, s.label, theme));
  return group(s.id, style, items, {
    angle: s.angle ?? 0,
    c: { x: r.x + r.width / 2, y: r.y + r.height / 2 },
  });
}

function connectorVisual(
  scene: FigureScene,
  c: ConnectorNode,
  theme: FigureTheme,
): Visual | undefined {
  if (c.points.length < 2) return undefined;
  const style = c.style ?? {};
  const rng = Rng.forId(c.id);
  const [ds, de] = endDirections(scene, c);
  const path = connectorPath(
    c.points,
    c.routing ?? "straight",
    ds,
    de,
    sd.roughness(style),
    rng,
  );
  const stroke = strokeColour(sd.stroke(style), theme);
  const width = sd.strokeWidth(style);
  const d = dash(style);
  const items: Primitive[] = [
    {
      type: "path",
      d: path.d,
      fill: "none",
      stroke,
      strokeWidth: width,
      linecap: "round",
      linejoin: "round",
      ...(d === undefined ? {} : { dash: d }),
    },
  ];
  const n = c.points.length;
  const startHead = c.heads?.start ?? "none";
  const endHead = c.heads?.end ?? (c.kind === "arrow" ? "arrow" : "none");
  for (const [head, tip, dir] of [
    [startHead, c.points[0] as Vec, path.startDir],
    [endHead, c.points[n - 1] as Vec, path.endDir],
  ] as const) {
    const p = headPrimitive(head, tip, dir, width, stroke);
    if (p !== undefined) items.push(p);
  }
  if (c.label !== undefined) {
    const mid = pathMidpoint(c.points);
    const size = style.fontSize ?? CONNECTOR_FONT_SIZE;
    const family = sd.fontFamily(style);
    const lines = wrap(c.label, 0, size, family);
    const total = size * LINE_HEIGHT * lines.length;
    items.push(
      textBlock(
        lines,
        mid.x,
        mid.y - total / 2,
        size,
        family,
        "middle",
        stroke,
        strokeColour("paper", theme),
      ),
    );
  }
  return group(c.id, style, items);
}

/** What `node` draws. `exportFrame` hides that frame's own border. */
export function nodeVisual(
  scene: FigureScene,
  node: FigureNode,
  theme: FigureTheme,
  exportFrame?: string,
): Visual | undefined {
  const style = node.style ?? {};
  switch (node.kind) {
    case "rect":
    case "diamond":
    case "ellipse":
      return shapeVisual(node, theme);
    case "text": {
      const items =
        node.label === undefined ? [] : [label("text", node, node.label, theme)];
      return group(node.id, style, items, {
        angle: node.angle ?? 0,
        c: { x: node.x + node.width / 2, y: node.y + node.height / 2 },
      });
    }
    case "frame": {
      if (exportFrame === node.id) return undefined;
      const r = normalizeRect(node.x, node.y, node.width, node.height);
      const stroke = strokeColour(style.stroke ?? "muted", theme);
      const items: Primitive[] = [
        { type: "rect", ...r, ...fillOf(style, theme), stroke },
      ];
      if (node.label !== undefined) {
        items.push({
          type: "text",
          fill: stroke,
          family: fontStack("mono"),
          size: 12,
          anchor: "start",
          lines: [{ x: r.x, y: r.y - 6, text: node.label }],
        });
      }
      return group(node.id, style, items, {
        angle: node.angle ?? 0,
        c: { x: r.x + r.width / 2, y: r.y + r.height / 2 },
      });
    }
    case "image": {
      const r = normalizeRect(node.x, node.y, node.width, node.height);
      return group(node.id, style, [{ type: "image", href: node.src ?? "", ...r }], {
        angle: node.angle ?? 0,
        c: { x: node.x + node.width / 2, y: node.y + node.height / 2 },
      });
    }
    case "mermaid": {
      const r = normalizeRect(node.x, node.y, node.width, node.height);
      return group(node.id, style, [{ type: "embed", id: node.id, ...r }], {
        angle: node.angle ?? 0,
        c: { x: r.x + r.width / 2, y: r.y + r.height / 2 },
      });
    }
    case "arrow":
    case "line":
      return connectorVisual(scene, node, theme);
    case "freedraw": {
      const d = freedrawPath(node);
      if (d === "") return undefined;
      return group(node.id, style, [
        { type: "path", d, fill: strokeColour(sd.stroke(style), theme) },
      ]);
    }
    case "math":
      return group(node.id, style, [
        {
          type: "text",
          fill: strokeColour(sd.stroke(style), theme),
          family: fontStack("mono"),
          size: node.size,
          anchor: "start",
          lines: [{ x: node.x, y: node.y, text: node.tex }],
        },
      ]);
  }
}

/** The export region: a frame, else the page grown to hold everything. */
export function exportRegion(scene: FigureScene, frame?: string): Rect {
  const f = frame === undefined ? undefined : scene.nodes.find((n) => n.id === frame);
  if (f !== undefined) return nodeBounds(f);
  const page = { x: 0, y: 0, width: scene.width, height: scene.height };
  const content = scene.nodes.reduce<Rect>(
    (acc, n) => unionRect(acc, nodeBounds(n)),
    EMPTY_RECT,
  );
  if ((content.width === 0 && content.height === 0) || rectContainsRect(page, content))
    return page;
  return unionRect(page, {
    x: content.x - 16,
    y: content.y - 16,
    width: content.width + 32,
    height: content.height + 32,
  });
}

export function escapeXml(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

function primitiveSvg(p: Primitive): string {
  const f = formatCoord;
  switch (p.type) {
    case "path": {
      let s = `<path d="${p.d}" fill="${p.fill}"`;
      if (p.fillOpacity !== undefined) s += ` fill-opacity="${f(p.fillOpacity)}"`;
      if (p.stroke !== undefined)
        s += ` stroke="${p.stroke}" stroke-width="${f(p.strokeWidth ?? 1)}"`;
      if (p.linecap !== undefined) s += ` stroke-linecap="${p.linecap}"`;
      if (p.linejoin !== undefined) s += ` stroke-linejoin="${p.linejoin}"`;
      if (p.dash !== undefined) s += ` stroke-dasharray="${p.dash}"`;
      return `${s}/>`;
    }
    case "rect": {
      let s = `<rect x="${f(p.x)}" y="${f(p.y)}" width="${f(p.width)}" height="${f(p.height)}" fill="${p.fill}"`;
      if (p.fillOpacity !== undefined) s += ` fill-opacity="${f(p.fillOpacity)}"`;
      s += ` stroke="${p.stroke}" stroke-width="1"`;
      if (p.dash !== undefined) s += ` stroke-dasharray="${p.dash}"`;
      return `${s}/>`;
    }
    case "circle":
      return `<circle cx="${f(p.cx)}" cy="${f(p.cy)}" r="${f(p.r)}" fill="${p.fill}"/>`;
    case "image":
      return `<image href="${escapeXml(p.href)}" x="${f(p.x)}" y="${f(p.y)}" width="${f(p.width)}" height="${f(p.height)}" preserveAspectRatio="xMidYMid meet"/>`;
    case "text": {
      const halo =
        p.halo === undefined
          ? ""
          : ` stroke="${p.halo}" stroke-width="4" stroke-linejoin="round" paint-order="stroke"`;
      const spans = p.lines
        .map((l) => `<tspan x="${f(l.x)}" y="${f(l.y)}">${escapeXml(l.text)}</tspan>`)
        .join("");
      return `<text fill="${p.fill}" font-family="${p.family}" font-size="${f(p.size)}" text-anchor="${p.anchor}"${halo}>${spans}</text>`;
    }
    case "embed":
      return `<rect x="${f(p.x)}" y="${f(p.y)}" width="${f(p.width)}" height="${f(p.height)}" fill="none" stroke="currentColor" stroke-width="1" stroke-dasharray="4 4"/>`;
  }
}

/** The scene as an SVG document (the browser mock's export). */
export function renderSceneSvg(
  scene: FigureScene,
  theme: FigureTheme = "print",
): string {
  const region = exportRegion(scene);
  const [x, y, w, h] = [region.x, region.y, region.width, region.height].map(
    formatCoord,
  );
  let svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${x} ${y} ${w} ${h}" width="${w}" height="${h}">\n`;
  for (const node of scene.nodes) {
    const v = nodeVisual(scene, node, theme);
    if (v === undefined) continue;
    let open = `  <g id="${escapeXml(v.id)}"`;
    if (v.rotate !== undefined) {
      open += ` transform="rotate(${formatCoord(v.rotate.angle)} ${formatCoord(v.rotate.cx)} ${formatCoord(v.rotate.cy)})"`;
    }
    if (v.opacity !== undefined) open += ` opacity="${formatCoord(v.opacity)}"`;
    svg += `${open}>\n${v.items.map((p) => `    ${primitiveSvg(p)}\n`).join("")}  </g>\n`;
  }
  return `${svg}</svg>\n`;
}
