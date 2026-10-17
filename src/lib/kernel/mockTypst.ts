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
import type { MockProjectFs, MockProjectHost } from "./mockProject";
import { scalarLength } from "./offsets";
import type { CompileReport, WireDiagnostic } from "./types";

const PAGE_W = 595.28;
const PAGE_H = 841.89;
const MARGIN = 64;
const LINE_STEP = 17;
const WRAP = 78;

interface PageLine {
  readonly kind: "heading" | "body" | "math" | "figure" | "rule" | "reference";
  readonly text: string;
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

/** Strip the markup the mock does not draw, keeping the words. */
function plain(text: string): string {
  return text
    .replace(/#set [^\n]*/g, "")
    .replace(/\$([^$]+)\$/g, "$1")
    .replace(/#[a-z]+\([^)]*\)/g, "")
    .replace(/@([A-Za-z][\w-]*)/g, "[$1]")
    .replace(/[*_]/g, "");
}

export function layout(text: string): PageLine[] {
  const lines: PageLine[] = [];
  let paragraph: string[] = [];
  const flush = (): void => {
    if (paragraph.length === 0) return;
    for (const row of wrap(plain(paragraph.join(" "))))
      lines.push({ kind: "body", text: row });
    lines.push({ kind: "rule", text: "" });
    paragraph = [];
  };
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    const heading = /^(=+)\s+(.*)$/.exec(line);
    const display = /^\$\s+(.+?)\s+\$$/.exec(line);
    const figure = /image\(\s*"([^"]+)"/.exec(line);
    if (line === "" || line.startsWith("#set ") || line.startsWith("#include")) {
      flush();
    } else if (heading !== null) {
      flush();
      lines.push({ kind: "heading", text: heading[2] ?? "" });
    } else if (display !== null) {
      flush();
      lines.push({ kind: "math", text: display[1] ?? "" });
    } else if (figure !== null) {
      flush();
      lines.push({ kind: "figure", text: figure[1] ?? "" });
    } else if (line.startsWith("#bibliography")) {
      flush();
      lines.push({ kind: "reference", text: "References" });
    } else {
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
): string {
  const text = source(fs, sources, path);
  if (text === undefined) return "";
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

  if (depth > 4) return text;
  const lines = text.split("\n");
  const out: string[] = [];
  lines.forEach((raw, index) => {
    const include = /^\s*#include\s+"([^"]+)"/.exec(raw);
    if (include === null || include[1] === undefined) {
      out.push(raw);
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
      expand(fs, sources, target, depth + 1, diagnostics, bibKeys, knownFigures),
    );
  });
  return out.join("\n");
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
];

export function typstPlugin(host: MockProjectHost): MockPluginFactory {
  return (): MockPlugin => {
    let latest: Compiled | null = null;
    let generation = 0;

    return {
      name: "typst",
      commands: COMMANDS,
      handlers: {
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
          const text = expand(
            fs,
            sources,
            entry,
            0,
            diagnostics,
            bibliographyKeys(fs, sources),
            figures,
          );
          const ok = diagnostics.every((d) => d.severity !== "error");
          const pages = paginate(layout(text));
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
          return new TextEncoder().encode(
            `%PDF-1.7\n% Manang mock export: ${latest.pages.length} page(s), generation ${latest.generation}\n%%EOF\n`,
          );
        },
      },
    };
  };
}
