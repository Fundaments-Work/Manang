/**
 * In-memory stand-in for the `diagram` plugin (`crates/manang-diagram`), so the
 * diagram preview can be exercised in `pnpm dev` and verify-ui.
 *
 * Clearly a mock: it draws a small Mermaid flowchart subset (`flowchart TD/LR`,
 * `A --> B`, `A[Label]`) as boxes in ranks, a placeholder box for anything
 * else, and reports a parse error wherever a line contains `-->>>`, so the
 * error path can be tested. The desktop app renders with the Rust plugin.
 */

import type { Args } from "./client";
import type { MockPlugin, MockPluginFactory } from "./mock";
import type {
  DiagramDiagnostic,
  DiagramLanguage,
  DiagramLanguageId,
  DiagramTheme,
} from "./types";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["diagram.check", "Check a diagram source for errors"],
  ["diagram.key", "Cache key for a rendered diagram"],
  ["diagram.languages", "List supported diagram languages"],
  ["diagram.layout", "Lay out a Mermaid diagram as shapes"],
  ["diagram.render", "Render a diagram to SVG"],
];

export const MOCK_LANGUAGES: readonly DiagramLanguage[] = [
  {
    id: "mermaid",
    name: "Mermaid",
    extensions: [".mmd", ".mermaid"],
    fences: ["mermaid"],
    engine: "merman 0.9.0 (browser mock)",
  },
  {
    id: "pikchr",
    name: "Pikchr",
    extensions: [".pikchr"],
    fences: ["pikchr"],
    engine: "pikru (browser mock)",
  },
  {
    id: "svgbob",
    name: "ASCII diagram",
    extensions: [".bob"],
    fences: ["bob", "svgbob"],
    engine: "svgbob (browser mock)",
  },
];

const BAD_TOKEN = "-->>>";

/** Ink and ground per theme, from the design tokens. */
const COLOURS: Record<DiagramTheme, { ground: string; ink: string; line: string }> = {
  dark: { ground: "#141416", ink: "#e8e6e1", line: "#3d3d42" },
  light: { ground: "#faf9f7", ink: "#1c1c1c", line: "#d4d3ce" },
  print: { ground: "#faf9f7", ink: "#1c1c1c", line: "#1c1c1c" },
};

function language(args: Args): DiagramLanguageId {
  const value = args["language"];
  if (value === "mermaid" || value === "pikchr" || value === "svgbob") return value;
  throw new Error(`[unknown_language] unknown diagram language \`${String(value)}\``);
}

function text(args: Args, key: string): string {
  const value = args[key];
  if (typeof value !== "string") throw new Error(`\`${key}\` must be a string`);
  return value;
}

function theme(args: Args): DiagramTheme {
  const value = args["theme"];
  if (value === "dark" || value === "light" || value === "print") return value;
  throw new Error("`theme` must be dark, light or print");
}

/** Diagnostics of the mock's one rule: `-->>>` is not an arrow. */
export function mockCheck(source: string): DiagramDiagnostic[] {
  const out: DiagramDiagnostic[] = [];
  source.split("\n").forEach((line, index) => {
    const at = line.indexOf(BAD_TOKEN);
    if (at === -1) return;
    const column = Array.from(line.slice(0, at)).length + 1;
    out.push({
      severity: "error",
      message: `unexpected token \`${BAD_TOKEN}\`; an arrow is \`-->\``,
      line: index + 1,
      column,
      endLine: index + 1,
      endColumn: column + BAD_TOKEN.length,
      code: "parse",
    });
  });
  return out;
}

function escapeXml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

interface Flowchart {
  readonly direction: "TD" | "LR";
  readonly labels: Map<string, string>;
  readonly edges: readonly (readonly [string, string])[];
}

const NODE = /^([A-Za-z_][\w-]*)(?:\[([^\]]*)\])?$/;

/** The supported flowchart subset, or null when the source is something else. */
export function parseFlowchart(source: string): Flowchart | null {
  const lines = source
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l !== "" && !l.startsWith("%%"));
  const header = /^(?:flowchart|graph)\s+(TD|TB|LR)\s*$/.exec(lines[0] ?? "");
  if (header === null) return null;
  const labels = new Map<string, string>();
  const edges: [string, string][] = [];
  const node = (part: string): string | null => {
    const m = NODE.exec(part.trim());
    if (m === null || m[1] === undefined) return null;
    if (m[2] !== undefined || !labels.has(m[1])) labels.set(m[1], m[2] ?? m[1]);
    return m[1];
  };
  for (const line of lines.slice(1)) {
    const parts = line.split("-->");
    const ids = parts.map(node);
    if (ids.some((id) => id === null)) continue;
    for (let i = 1; i < ids.length; i += 1) {
      const from = ids[i - 1];
      const to = ids[i];
      if (from != null && to != null) edges.push([from, to]);
    }
  }
  return { direction: header[1] === "LR" ? "LR" : "TD", labels, edges };
}

const BOX_W = 140;
const BOX_H = 44;
const GAP = 56;
const PAD = 24;

function flowchartSvg(chart: Flowchart, colours: (typeof COLOURS)["dark"]): string {
  // Rank = longest path from a source; cycles stop at the node count.
  const ids = [...chart.labels.keys()];
  const rank = new Map(ids.map((id) => [id, 0]));
  for (let round = 0; round < ids.length; round += 1) {
    for (const [from, to] of chart.edges) {
      const next = (rank.get(from) ?? 0) + 1;
      if (next > (rank.get(to) ?? 0) && next < ids.length) rank.set(to, next);
    }
  }
  const slot = new Map<string, number>();
  const perRank = new Map<number, number>();
  for (const id of ids) {
    const r = rank.get(id) ?? 0;
    slot.set(id, perRank.get(r) ?? 0);
    perRank.set(r, (perRank.get(r) ?? 0) + 1);
  }
  const ranks = Math.max(1, ...[...rank.values()].map((r) => r + 1));
  const widest = Math.max(1, ...perRank.values());
  const lr = chart.direction === "LR";
  const at = (id: string): { x: number; y: number } => {
    const along = (rank.get(id) ?? 0) * ((lr ? BOX_W : BOX_H) + GAP);
    const across = (slot.get(id) ?? 0) * ((lr ? BOX_H : BOX_W) + GAP / 2);
    return lr
      ? { x: PAD + along, y: PAD + across }
      : { x: PAD + across, y: PAD + along };
  };
  const alongSize = ranks * ((lr ? BOX_W : BOX_H) + GAP) - GAP;
  const acrossSize = widest * ((lr ? BOX_H : BOX_W) + GAP / 2) - GAP / 2;
  const width = 2 * PAD + (lr ? alongSize : acrossSize);
  const height = 2 * PAD + (lr ? acrossSize : alongSize);

  const edges = chart.edges.map(([from, to]) => {
    const a = at(from);
    const b = at(to);
    const x1 = a.x + (lr ? BOX_W : BOX_W / 2);
    const y1 = a.y + (lr ? BOX_H / 2 : BOX_H);
    const x2 = b.x + (lr ? 0 : BOX_W / 2);
    const y2 = b.y + (lr ? BOX_H / 2 : 0);
    return `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="${colours.ink}" marker-end="url(#arrow)"/>`;
  });
  const nodes = ids.map((id) => {
    const p = at(id);
    const label = escapeXml(chart.labels.get(id) ?? id);
    return (
      `<rect x="${p.x}" y="${p.y}" width="${BOX_W}" height="${BOX_H}" fill="${colours.ground}" stroke="${colours.ink}"/>` +
      `<text x="${p.x + BOX_W / 2}" y="${p.y + BOX_H / 2}" fill="${colours.ink}" font-family="DM Sans, sans-serif" font-size="14" text-anchor="middle" dominant-baseline="central">${label}</text>`
    );
  });
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}">` +
    `<defs><marker id="arrow" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0 0L10 5L0 10z" fill="${colours.ink}"/></marker></defs>` +
    `<rect width="${width}" height="${height}" fill="${colours.ground}"/>` +
    edges.join("") +
    nodes.join("") +
    `</svg>`
  );
}

function placeholderSvg(name: string, colours: (typeof COLOURS)["dark"]): string {
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 120" width="320" height="120">` +
    `<rect width="320" height="120" fill="${colours.ground}"/>` +
    `<rect x="12" y="12" width="296" height="96" fill="none" stroke="${colours.line}"/>` +
    `<text x="160" y="60" fill="${colours.ink}" font-family="DM Sans, sans-serif" font-size="14" text-anchor="middle" dominant-baseline="central">${escapeXml(name)} (browser mock)</text>` +
    `</svg>`
  );
}

/** The SVG text the mock renders, or throws the plugin's `[diagram_error]`. */
export function mockRender(
  id: DiagramLanguageId,
  source: string,
  colourTheme: DiagramTheme,
): string {
  const first = mockCheck(source)[0];
  if (first !== undefined) {
    throw new Error(
      `[diagram_error] line ${first.line}:${first.column}: ${first.message}`,
    );
  }
  const colours = COLOURS[colourTheme];
  const chart = id === "mermaid" ? parseFlowchart(source) : null;
  if (chart !== null && chart.labels.size > 0) return flowchartSvg(chart, colours);
  const name = MOCK_LANGUAGES.find((l) => l.id === id)?.name ?? id;
  return placeholderSvg(name, colours);
}

interface LaidOutNode {
  id: string;
  label: string;
  shape: "rect" | "round" | "diamond" | "circle";
}

const SHAPED =
  /^([A-Za-z_][\w-]*)\s*(?:\[([^\]]*)\]|\{([^}]*)\}|\(\(([^)]*)\)\)|\(([^)]*)\))?$/;

/**
 * `diagram.layout` for the mock's flowchart subset: `[box]`, `(round)`,
 * `{diamond}`, `((circle))` nodes, `-->`/`-.->` links with `|labels|`, laid
 * out in longest-path ranks. Same output shape as the Rust plugin; anything
 * else is `[unsupported_layout]`.
 */
export function mockMermaidLayout(source: string): Record<string, unknown> {
  const lines = source
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l !== "" && !l.startsWith("%%"));
  const header = /^(?:flowchart|graph)\s+(TD|TB|LR)\s*$/.exec(lines[0] ?? "");
  if (header === null) {
    const kind = (lines[0] ?? "unknown").split(/\s/)[0] ?? "unknown";
    throw new Error(
      `[invalid_argument] [unsupported_layout] ${kind} diagrams have no shape layout; insert them as a Mermaid block`,
    );
  }
  const nodes = new Map<string, LaidOutNode>();
  const edges: { from: string; to: string; label: string | null; dashed: boolean }[] =
    [];
  const node = (part: string): string | null => {
    const m = SHAPED.exec(part.trim());
    if (m === null || m[1] === undefined) return null;
    const id = m[1];
    const label = m[2] ?? m[3] ?? m[4] ?? m[5];
    const shape: LaidOutNode["shape"] =
      m[3] !== undefined
        ? "diamond"
        : m[4] !== undefined
          ? "circle"
          : m[5] !== undefined
            ? "round"
            : "rect";
    if (label !== undefined || !nodes.has(id)) {
      nodes.set(id, { id, label: label ?? id, shape });
    }
    return id;
  };
  for (const line of lines.slice(1)) {
    const parts = line.split(/\s*(-{2}>|-\.->)\s*/);
    let previous: string | null = null;
    let dashed = false;
    for (const part of parts) {
      if (part === "-->" || part === "-.->") {
        dashed = part === "-.->";
        continue;
      }
      const labelled = /^\|([^|]*)\|\s*(.*)$/.exec(part);
      const id = node(labelled === null ? part : (labelled[2] ?? ""));
      if (id === null) break;
      if (previous !== null) {
        edges.push({ from: previous, to: id, label: labelled?.[1] ?? null, dashed });
      }
      previous = id;
    }
  }
  const ids = [...nodes.keys()];
  const rank = new Map(ids.map((id) => [id, 0]));
  for (let round = 0; round < ids.length; round += 1) {
    for (const e of edges) {
      const next = (rank.get(e.from) ?? 0) + 1;
      if (next > (rank.get(e.to) ?? 0) && next < ids.length) rank.set(e.to, next);
    }
  }
  const slot = new Map<string, number>();
  const perRank = new Map<number, number>();
  for (const id of ids) {
    const r = rank.get(id) ?? 0;
    slot.set(id, perRank.get(r) ?? 0);
    perRank.set(r, (perRank.get(r) ?? 0) + 1);
  }
  const lr = header[1] === "LR";
  const size = (n: LaidOutNode): { w: number; h: number } =>
    n.shape === "diamond"
      ? { w: 120, h: 120 }
      : n.shape === "circle"
        ? { w: 90, h: 90 }
        : { w: BOX_W, h: BOX_H };
  const centre = (id: string): { x: number; y: number } => {
    const along = (rank.get(id) ?? 0) * ((lr ? BOX_W : 120) + GAP);
    const across = (slot.get(id) ?? 0) * ((lr ? 120 : BOX_W) + GAP);
    return lr
      ? { x: PAD + along + BOX_W / 2, y: PAD + across + 60 }
      : { x: PAD + across + BOX_W / 2, y: PAD + along + 60 };
  };
  return {
    family: "flowchart",
    direction: lr ? "LR" : "TB",
    nodes: ids.map((id) => {
      const n = nodes.get(id) ?? { id, label: id, shape: "rect" as const };
      const c = centre(id);
      const { w, h } = size(n);
      return {
        id,
        label: n.label,
        shape: n.shape,
        x: c.x - w / 2,
        y: c.y - h / 2,
        width: w,
        height: h,
      };
    }),
    edges: edges.map((e, i) => ({
      id: `L_${e.from}_${e.to}_${i}`,
      from: e.from,
      to: e.to,
      label: e.label,
      points: [centre(e.from), centre(e.to)],
      dashed: e.dashed,
      start: "none",
      end: "arrow",
    })),
    clusters: [],
  };
}

export function mockKey(
  id: DiagramLanguageId,
  source: string,
  colourTheme: DiagramTheme,
): string {
  let hash = 0x811c9dc5;
  const input = `${id}\u0000${colourTheme}\u0000${source}`;
  for (let i = 0; i < input.length; i += 1) {
    hash ^= input.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

export function diagramPlugin(): MockPluginFactory {
  return (): MockPlugin => ({
    name: "diagram",
    description: "Mermaid, Pikchr and ASCII diagrams rendered to SVG (browser mock)",
    commands: COMMANDS,
    handlers: {
      "diagram.languages": () => ({ languages: MOCK_LANGUAGES }),
      "diagram.check": (args) => {
        language(args);
        return { diagnostics: mockCheck(text(args, "source")) };
      },
      "diagram.layout": (args) => {
        if (args["language"] !== undefined && args["language"] !== "mermaid") {
          throw new Error(
            "[invalid_argument] [unsupported_layout] only Mermaid has a shape layout",
          );
        }
        return mockMermaidLayout(text(args, "source"));
      },
      "diagram.key": (args) => ({
        key: mockKey(language(args), text(args, "source"), theme(args)),
      }),
    },
    binaryHandlers: {
      "diagram.render": (args) =>
        new TextEncoder().encode(
          mockRender(language(args), text(args, "source"), theme(args)),
        ),
    },
  });
}
