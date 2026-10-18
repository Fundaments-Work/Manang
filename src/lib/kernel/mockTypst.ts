/**
 * In-memory `typst` plugin.
 *
 * Not a Typst compiler: it reports plausible diagnostics for the obvious
 * mistakes (unclosed math or brackets, missing include, missing figure,
 * unknown citation) and draws the document's headings, paragraphs and
 * equations as SVG pages, so the preview and Problems panel can be exercised
 * without the Rust engine. The command shapes mirror `crates/manang-typst`.
 */

import { analyse } from "$lib/editor/diagnostics";

import type { MockPlugin, MockPluginFactory } from "./mock";
import { textWidth, writePdf, type MockLink, type MockPage } from "./mockPdf";
import type { MockProjectFs, MockProjectHost } from "./mockProject";
import { scalarLength } from "./offsets";
import type { CompileReport, WireDiagnostic } from "./types";

const PAGE_W = 595.28;
const PAGE_H = 841.89;
const MARGIN = 64;
const LINE_STEP = 17;
const WRAP = 78;

/** One line of the document after `#include`s are spliced in, with its origin. */
interface SourceLine {
  readonly text: string;
  /** Project-relative path. */
  readonly file: string;
  /** 1-based. */
  readonly line: number;
}

interface PageLine {
  readonly kind: "heading" | "body" | "math" | "figure" | "rule" | "reference";
  readonly text: string;
  /** The source line it came from (a paragraph's rows: the paragraph's first line). */
  readonly file?: string;
  readonly line?: number;
  /** Web links (`#link("url")[label]`) whose label is on this row. */
  readonly links?: readonly { readonly label: string; readonly url: string }[];
}

interface Compiled {
  readonly pages: readonly (readonly PageLine[])[];
  readonly generation: number;
}

/** Text of a project file, preferring the shell's unsaved version. */
function source(
  fs: MockProjectFs,
  sources: Readonly<Record<string, string>>,
  path: string,
): string | undefined {
  const unsaved = sources[path];
  if (unsaved !== undefined) return unsaved;
  const saved = fs.files.get(path);
  return typeof saved === "string" ? saved : undefined;
}

function bibliographyKeys(
  fs: MockProjectFs,
  sources: Record<string, string>,
): Set<string> {
  const keys = new Set<string>();
  for (const path of fs.files.keys()) {
    if (!path.endsWith(".bib")) continue;
    const text = source(fs, sources, path) ?? "";
    for (const match of text.matchAll(/@\w+\s*\{\s*([^,\s]+)\s*,/g)) {
      if (match[1] !== undefined) keys.add(match[1]);
    }
  }
  return keys;
}

function position(text: string, utf16: number): { line: number; column: number } {
  const before = text.slice(0, utf16);
  const line = before.split("\n").length;
  const column = scalarLength(before.slice(before.lastIndexOf("\n") + 1)) + 1;
  return { line, column };
}

/** Wrap prose the way a justified column would, without hyphenation. */
function wrap(text: string): string[] {
  const out: string[] = [];
  let line = "";
  for (const word of text.split(/\s+/).filter((w) => w.length > 0)) {
    if (line.length > 0 && line.length + word.length + 1 > WRAP) {
      out.push(line);
      line = word;
    } else {
      line = line.length === 0 ? word : `${line} ${word}`;
    }
  }
  if (line.length > 0) out.push(line);
  return out;
}

/** `#link("https://...")[label]`: the label is drawn, the address becomes a link. */
const LINK = /#link\("([^"]+)"\)\[([^\]]*)\]/g;

/** Strip the markup the mock does not draw, keeping the words. */
function plain(text: string): string {
  return text
    .replace(/#set [^\n]*/g, "")
    .replace(LINK, "$2")
    .replace(/\$([^$]+)\$/g, "$1")
    .replace(/#[a-z]+\([^)]*\)/g, "")
    .replace(/@([A-Za-z][\w-]*)/g, "[$1]")
    .replace(/[*_]/g, "");
}

export function layout(input: string | readonly SourceLine[]): PageLine[] {
  const source: readonly SourceLine[] =
    typeof input === "string"
      ? input.split("\n").map((text, i) => ({ text, file: "", line: i + 1 }))
      : input;
  const lines: PageLine[] = [];
  let paragraph: string[] = [];
  let start: SourceLine | null = null;
  const flush = (): void => {
    if (paragraph.length === 0) return;
    const origin = start === null ? {} : { file: start.file, line: start.line };
    const raw = paragraph.join(" ");
    const links = [...raw.matchAll(LINK)].map((m) => ({
      url: m[1] ?? "",
      label: plain(m[2] ?? ""),
    }));
    for (const row of wrap(plain(raw))) {
      const here = links.filter((l) => l.label !== "" && row.includes(l.label));
      lines.push({
        kind: "body",
        text: row,
        ...origin,
        ...(here.length > 0 ? { links: here } : {}),
      });
    }
    lines.push({ kind: "rule", text: "" });
    paragraph = [];
    start = null;
  };
  for (const from of source) {
    const line = from.text.trim();
    const origin = { file: from.file, line: from.line };
    const heading = /^(=+)\s+(.*)$/.exec(line);
    const display = /^\$\s+(.+?)\s+\$$/.exec(line);
    const figure = /image\(\s*"([^"]+)"/.exec(line);
    if (line === "" || line.startsWith("#set ") || line.startsWith("#include")) {
      flush();
    } else if (heading !== null) {
      flush();
      lines.push({ kind: "heading", text: heading[2] ?? "", ...origin });
    } else if (display !== null) {
      flush();
      lines.push({ kind: "math", text: display[1] ?? "", ...origin });
    } else if (figure !== null) {
      flush();
      lines.push({ kind: "figure", text: figure[1] ?? "", ...origin });
    } else if (line.startsWith("#bibliography")) {
      flush();
      lines.push({ kind: "reference", text: "References", ...origin });
    } else {
      start ??= from;
      paragraph.push(line);
    }
  }
  flush();
  return lines;
}

const COST: Record<PageLine["kind"], number> = {
  heading: 2.2,
  body: 1,
  math: 2,
  figure: 9,
  rule: 0.5,
  reference: 2,
};

export function paginate(lines: readonly PageLine[]): PageLine[][] {
  const capacity = (PAGE_H - MARGIN * 2) / LINE_STEP;
  const pages: PageLine[][] = [[]];
  let used = 0;
  for (const line of lines) {
    const cost = COST[line.kind];
    if (used + cost > capacity && (pages[pages.length - 1]?.length ?? 0) > 0) {
      pages.push([]);
      used = 0;
    }
    pages[pages.length - 1]?.push(line);
    used += cost;
  }
  return pages;
}

function escapeXml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** Top and height (points) of each line's band on its page, as `pageSvg` draws it. */
export function lineBands(
  page: readonly PageLine[],
): { top: number; height: number }[] {
  let y = MARGIN;
  return page.map((line) => {
    const height = LINE_STEP * COST[line.kind];
    const top = y;
    y += height;
    return { top, height };
  });
}

export function pageSvg(
  page: readonly PageLine[],
  number: number,
  total: number,
): string {
  const parts: string[] = [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${PAGE_W} ${PAGE_H}" width="${PAGE_W}" height="${PAGE_H}">`,
    `<rect width="${PAGE_W}" height="${PAGE_H}" fill="#faf9f7"/>`,
  ];
  let y = MARGIN;
  const sans = `font-family="DM Sans, sans-serif"`;
  for (const line of page) {
    const text = escapeXml(line.text);
    switch (line.kind) {
      case "heading":
        y += 8;
        parts.push(
          `<text x="${MARGIN}" y="${y + 8}" ${sans} font-size="20" font-weight="300" fill="#1c1c1c">${text}</text>`,
        );
        y += LINE_STEP * COST.heading - 8;
        break;
      case "body":
        parts.push(
          `<text x="${MARGIN}" y="${y + 10}" ${sans} font-size="10.5" fill="#1c1c1c">${text}</text>`,
        );
        y += LINE_STEP;
        break;
      case "math":
        parts.push(
          `<text x="${PAGE_W / 2}" y="${y + 14}" text-anchor="middle" font-family="DM Mono, monospace" font-size="11" fill="#1c1c1c">${text}</text>`,
        );
        y += LINE_STEP * COST.math;
        break;
      case "figure":
        parts.push(
          `<rect x="${MARGIN + 40}" y="${y}" width="${PAGE_W - MARGIN * 2 - 80}" height="${LINE_STEP * 7.5}" fill="none" stroke="#6b6b6b" stroke-width="0.5"/>`,
          `<text x="${PAGE_W / 2}" y="${y + LINE_STEP * 3.9}" text-anchor="middle" font-family="DM Mono, monospace" font-size="8.5" fill="#6b6b6b">${text}</text>`,
        );
        y += LINE_STEP * COST.figure;
        break;
      case "reference":
        parts.push(
          `<text x="${MARGIN}" y="${y + 12}" ${sans} font-size="14" font-weight="300" fill="#1c1c1c">${text}</text>`,
        );
        y += LINE_STEP * COST.reference;
        break;
      case "rule":
        y += LINE_STEP * COST.rule;
        break;
    }
  }
  parts.push(
    `<text x="${PAGE_W / 2}" y="${PAGE_H - 30}" text-anchor="middle" font-family="DM Mono, monospace" font-size="8" fill="#6b6b6b">${number} / ${total}</text>`,
    "</svg>",
  );
  return parts.join("");
}

const INK = 0.11;
const INK_60 = 0.42;

/**
 * The laid-out pages as a real PDF, drawn where `pageSvg` draws them (so
 * `typst.jump-*` points land on the text): selectable text, an outline of the
 * headings, citations (`[key]`) linking to the References heading and web
 * links from `#link`. Lets the PDF viewer run against the mock.
 */
export function pagesPdf(pages: readonly (readonly PageLine[])[]): Uint8Array {
  const outline: { title: string; page: number; y: number }[] = [];
  let references: { page: number; y: number } | null = null;
  // First pass: where each heading starts, for the outline and citations.
  pages.forEach((lines, page) => {
    lines.forEach((line, i) => {
      const top = lineBands(lines)[i]?.top ?? MARGIN;
      if (line.kind === "heading") outline.push({ title: line.text, page, y: top });
      if (line.kind === "reference") {
        outline.push({ title: line.text, page, y: top });
        references = { page, y: top };
      }
    });
  });
  const target = references as { page: number; y: number } | null;
  const out: MockPage[] = pages.map((lines, index) => {
    const texts: MockPage["texts"][number][] = [];
    const rects: { x: number; y: number; width: number; height: number }[] = [];
    const links: MockLink[] = [];
    let y = MARGIN;
    for (const line of lines) {
      switch (line.kind) {
        case "heading":
          y += 8;
          texts.push({ x: MARGIN, y: y + 8, size: 20, text: line.text, grey: INK });
          y += LINE_STEP * COST.heading - 8;
          break;
        case "body": {
          const base = y + 10;
          texts.push({ x: MARGIN, y: base, size: 10.5, text: line.text, grey: INK });
          const box = (from: number, label: string) => ({
            x: MARGIN + textWidth(line.text.slice(0, from), 10.5),
            y: base - 9,
            width: textWidth(label, 10.5),
            height: 12,
          });
          for (const link of line.links ?? []) {
            const at = line.text.indexOf(link.label);
            if (at >= 0)
              links.push({ rect: box(at, link.label), to: { url: link.url } });
          }
          if (target !== null) {
            for (const cite of line.text.matchAll(/\[[A-Za-z][\w-]*\]/g)) {
              links.push({ rect: box(cite.index, cite[0]), to: target });
            }
          }
          y += LINE_STEP;
          break;
        }
        case "math": {
          const w = textWidth(line.text, 11, "mono");
          texts.push({
            x: PAGE_W / 2 - w / 2,
            y: y + 14,
            size: 11,
            text: line.text,
            font: "mono",
            grey: INK,
          });
          y += LINE_STEP * COST.math;
          break;
        }
        case "figure": {
          rects.push({
            x: MARGIN + 40,
            y,
            width: PAGE_W - MARGIN * 2 - 80,
            height: LINE_STEP * 7.5,
          });
          const w = textWidth(line.text, 8.5, "mono");
          texts.push({
            x: PAGE_W / 2 - w / 2,
            y: y + LINE_STEP * 3.9,
            size: 8.5,
            text: line.text,
            font: "mono",
            grey: INK_60,
          });
          y += LINE_STEP * COST.figure;
          break;
        }
        case "reference":
          texts.push({ x: MARGIN, y: y + 12, size: 14, text: line.text, grey: INK });
          y += LINE_STEP * COST.reference;
          break;
        case "rule":
          y += LINE_STEP * COST.rule;
          break;
      }
    }
    const footer = `${index + 1} / ${pages.length}`;
    const fw = textWidth(footer, 8, "mono");
    texts.push({
      x: PAGE_W / 2 - fw / 2,
      y: PAGE_H - 30,
      size: 8,
      text: footer,
      font: "mono",
      grey: INK_60,
    });
    return { texts, rects, links };
  });
  return writePdf({
    width: PAGE_W,
    height: PAGE_H,
    paper: [250 / 255, 249 / 255, 247 / 255],
    pages: out,
    outline,
  });
}

/**
 * The document as Typst would see it: the entry file with `#include`d files
 * spliced in. Missing includes become diagnostics instead of text.
 */
function expand(
  fs: MockProjectFs,
  sources: Record<string, string>,
  path: string,
  depth: number,
  diagnostics: WireDiagnostic[],
  bibKeys: ReadonlySet<string>,
  knownFigures: readonly string[],
): SourceLine[] {
  const text = source(fs, sources, path);
  if (text === undefined) return [];
  const file = `/${path}`;

  for (const item of analyse(text, { knownFigures })) {
    const key = /^Missing label for @(.+)$/.exec(item.message)?.[1];
    if (key !== undefined && bibKeys.has(key)) continue;
    const { line, column } = position(text, item.from);
    const startScalar = scalarLength(text.slice(0, item.from));
    diagnostics.push({
      severity: item.severity,
      message: item.message.replace(
        /^Missing label for @(.+)$/,
        "label `$1` does not exist in the document",
      ),
      file,
      line,
      column,
      range: {
        start: startScalar,
        end: startScalar + scalarLength(text.slice(item.from, item.to)),
      },
      hints: hintFor(item.message),
      trace: [],
    });
  }

  const lines = text.split("\n");
  const own = (raw: string, index: number): SourceLine => ({
    text: raw,
    file: path,
    line: index + 1,
  });
  if (depth > 4) return lines.map(own);
  const out: SourceLine[] = [];
  lines.forEach((raw, index) => {
    const include = /^\s*#include\s+"([^"]+)"/.exec(raw);
    if (include === null || include[1] === undefined) {
      out.push(own(raw, index));
      return;
    }
    const target = include[1].replace(/^\//, "");
    if (source(fs, sources, target) === undefined) {
      const column = raw.indexOf('"') + 1;
      diagnostics.push({
        severity: "error",
        message: `file not found (searched at /${target})`,
        file,
        line: index + 1,
        column: column + 1,
        range: null,
        hints: ["check the path, or create the file from the Files panel"],
        trace: [],
      });
      return;
    }
    out.push(
      ...expand(fs, sources, target, depth + 1, diagnostics, bibKeys, knownFigures),
    );
  });
  return out;
}

function hintFor(message: string): string[] {
  if (message.startsWith("Unclosed math"))
    return ["close the formula with a matching `$`"];
  if (message.startsWith("Missing label"))
    return ["add the entry to your bibliography file"];
  return [];
}

const COMMANDS: readonly (readonly [string, string])[] = [
  ["typst.compile", "Compile Typst document"],
  ["typst.render", "Render page as SVG"],
  ["typst.export-pdf", "Export PDF"],
  ["typst.jump-from-click", "Jump from preview to source"],
  ["typst.jump-from-cursor", "Jump from source to preview"],
  ["typst.svg-to-pdf", "Convert an SVG to a one-page PDF"],
];

function uint(args: Record<string, unknown>, key: string): number {
  const value = args[key];
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0)
    throw { message: `\`${key}\` must be an unsigned integer` };
  return value;
}

function finite(args: Record<string, unknown>, key: string): number {
  const value = args[key];
  if (typeof value !== "number" || !Number.isFinite(value))
    throw { message: `\`${key}\` must be a finite number` };
  return value;
}

/** The source line of the laid-out line nearest `y` on a page. */
export function lineAt(
  page: readonly PageLine[],
  y: number,
): { file: string; line: number } | null {
  const bands = lineBands(page);
  let best: { file: string; line: number } | null = null;
  let distance = Infinity;
  page.forEach((entry, i) => {
    const band = bands[i];
    if (entry.file === undefined || entry.line === undefined || band === undefined)
      return;
    const bottom = band.top + band.height;
    const d = y < band.top ? band.top - y : y > bottom ? y - bottom : 0;
    if (d < distance) {
      distance = d;
      best = { file: entry.file, line: entry.line };
    }
  });
  return best;
}

/**
 * Where a source line landed: the last laid-out line from `file` that starts
 * at or before `line`, as the baseline start of its text (like Typst).
 */
export function pointFor(
  pages: readonly (readonly PageLine[])[],
  file: string,
  line: number,
): { page: number; x: number; y: number } | null {
  let found: { page: number; x: number; y: number } | null = null;
  let foundLine = 0;
  pages.forEach((lines, page) => {
    const bands = lineBands(lines);
    lines.forEach((entry, i) => {
      const band = bands[i];
      if (entry.file !== file || entry.line === undefined || band === undefined) return;
      // The first row of the latest source line wins (a paragraph's first row).
      if (entry.line <= line && entry.line > foundLine) {
        found = { page, x: MARGIN, y: band.top + Math.min(band.height, 13) };
        foundLine = entry.line;
      }
    });
  });
  return found;
}

export function typstPlugin(host: MockProjectHost): MockPluginFactory {
  return (): MockPlugin => {
    let latest: Compiled | null = null;
    let generation = 0;

    return {
      name: "typst",
      commands: COMMANDS,
      handlers: {
        "typst.jump-from-click": (args) => {
          if (latest === null) {
            throw { message: "no successful compile yet; run `typst.compile` first" };
          }
          const page = uint(args, "page");
          const x = finite(args, "x");
          const y = finite(args, "y");
          const lines = latest.pages[page];
          if (lines === undefined) {
            throw {
              message: `page ${page} is out of range; the document has ${latest.pages.length} page(s)`,
            };
          }
          if (x < MARGIN / 2 || x > PAGE_W - MARGIN / 2 || y < 0 || y > PAGE_H)
            return null;
          const hit = lineAt(lines, y);
          return hit === null
            ? null
            : { kind: "file", path: hit.file, line: hit.line, column: 1 };
        },
        "typst.jump-from-cursor": (args) => {
          if (latest === null) {
            throw { message: "no successful compile yet; run `typst.compile` first" };
          }
          const path = args["path"];
          if (typeof path !== "string") throw { message: "`path` must be a string" };
          const line = uint(args, "line");
          uint(args, "column");
          const point = pointFor(latest.pages, path.replace(/^\//, ""), line);
          return point === null ? [] : [point];
        },
        "typst.compile": (args): CompileReport => {
          const fs = host.require();
          const main = args["main"];
          if (typeof main !== "string")
            throw { message: "`main` must be a string path" };
          const raw = args["sources"];
          if (
            raw !== undefined &&
            raw !== null &&
            (typeof raw !== "object" || Array.isArray(raw))
          ) {
            throw { message: "`sources` must be an object of path to text" };
          }
          const sources = {
            ...((raw as Record<string, string> | null | undefined) ?? {}),
          };
          const entry = main.replace(/^\//, "");
          if (source(fs, sources, entry) === undefined) {
            throw { message: `${entry}: No such file or directory` };
          }
          const diagnostics: WireDiagnostic[] = [];
          const figures = [...fs.files.keys()].filter((path) => !path.endsWith(".typ"));
          const expanded = expand(
            fs,
            sources,
            entry,
            0,
            diagnostics,
            bibliographyKeys(fs, sources),
            figures,
          );
          const text = expanded.map((l) => l.text).join("\n");
          const ok = diagnostics.every((d) => d.severity !== "error");
          const pages = paginate(layout(expanded));
          if (ok) {
            generation += 1;
            latest = { pages, generation };
          }
          return {
            ok,
            pages: ok ? pages.length : (latest?.pages.length ?? 0),
            page_sizes: Array.from(
              { length: ok ? pages.length : (latest?.pages.length ?? 0) },
              () => [PAGE_W, PAGE_H] as const,
            ),
            diagnostics,
            // Deterministic, so tests can assert on it.
            duration_ms: Math.round((4 + text.length / 700) * 10) / 10,
            generation,
          };
        },
      },
      binaryHandlers: {
        // A PDF header around nothing: the demo cannot lay out an SVG.
        "typst.svg-to-pdf": (args) => {
          const svg = args["svg"];
          if (typeof svg !== "string" || !svg.includes("<svg")) {
            throw {
              message: "[invalid_svg] the SVG could not be placed: not an SVG document",
            };
          }
          return new TextEncoder().encode(
            "%PDF-1.7\n% Manang browser mock: one page.\n%%EOF\n",
          );
        },
        "typst.render": (args) => {
          if (latest === null) {
            throw { message: "nothing has been compiled successfully yet" };
          }
          const page = args["page"] ?? 0;
          if (typeof page !== "number" || !Number.isInteger(page) || page < 0) {
            throw { message: "`page` must be an unsigned integer" };
          }
          const lines = latest.pages[page];
          if (lines === undefined) {
            throw {
              message: `page ${page + 1} does not exist (the document has ${latest.pages.length})`,
            };
          }
          return new TextEncoder().encode(
            pageSvg(lines, page + 1, latest.pages.length),
          );
        },
        "typst.export-pdf": () => {
          if (latest === null) {
            throw { message: "nothing has been compiled successfully yet" };
          }
          // Not Typst's PDF, but a real one drawn from the mock layout: text,
          // outline and links, enough for the preview's PDF viewer.
          return pagesPdf(latest.pages);
        },
      },
    };
  };
}
