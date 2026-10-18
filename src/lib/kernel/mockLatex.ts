/**
 * In-memory `latex` plugin.
 *
 * Not TeX: it reads the main file from the fake disk (like the real plugin, which
 * compiles what is on disk), follows `\input` / `\include`, reports the obvious
 * mistakes the way the log parser words them (missing file, unclosed
 * environment, undefined citation) and returns a small but valid PDF, so the
 * preview, the PDF viewer and Problems can be exercised without Tectonic. The
 * shapes mirror `crates/manang-latex` (`plugin.rs` `compile_json`, `diagnostics.rs`).
 */

import type { MockPlugin, MockPluginFactory } from "./mock";
import type { MockProjectFs, MockProjectHost } from "./mockProject";
import type { LatexDiagnostic, LatexReport } from "./types";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["latex.status", "LaTeX engines available"],
  ["latex.compile", "Compile LaTeX"],
  ["latex.export-pdf", "Export PDF"],
  ["latex.synctex", "SyncTeX source/PDF lookup"],
];

const LINES_PER_PAGE = 42;
/** Page height and text geometry of `makePdf`, in points. */
const PDF_PAGE_H = 842;
const PDF_LEFT = 56;
const PDF_FIRST_BASELINE = 780;
const PDF_LEADING = 14;
const PDF_FONT = 11;

/** A source line, for SyncTeX: `file` is relative to the main file's directory. */
interface Origin {
  readonly file: string;
  readonly line: number;
}

/** One line of text with where it came from. */
interface Row {
  readonly text: string;
  readonly origin: Origin | null;
}

interface Built {
  readonly pdf: Uint8Array;
  readonly pages: number;
  /** Every printed row, in order; row `i` is on page `i / LINES_PER_PAGE`. */
  readonly rows: readonly Row[];
}

function text(fs: MockProjectFs, path: string): string | undefined {
  const value = fs.files.get(path);
  return typeof value === "string" ? value : undefined;
}

function dirOf(path: string): string {
  const at = path.lastIndexOf("/");
  return at === -1 ? "" : path.slice(0, at);
}

function joinPath(dir: string, name: string): string {
  return dir === "" ? name : `${dir}/${name}`;
}

function bibKeys(fs: MockProjectFs): Set<string> {
  const keys = new Set<string>();
  for (const [path, value] of fs.files) {
    if (!path.endsWith(".bib") || typeof value !== "string") continue;
    for (const m of value.matchAll(/@\w+\s*\{\s*([^,\s]+)\s*,/g)) {
      if (m[1] !== undefined) keys.add(m[1]);
    }
  }
  return keys;
}

interface Expansion {
  readonly lines: Row[];
  readonly diagnostics: LatexDiagnostic[];
  /** Packages named by `\\usepackage` so far (the preamble comes first). */
  readonly loaded?: Set<string>;
}

/** The few commands whose package the demo can tell is missing (see `crates/manang-heal`). */
const NEEDS_PACKAGE: readonly (readonly [RegExp, string])[] = [
  [/\\(includegraphics|rotatebox|scalebox|resizebox)\b/, "graphicx"],
  [/\\(textcolor|colorbox)\b/, "xcolor"],
  [/\\(toprule|midrule|bottomrule)\b/, "booktabs"],
  [/\\(multirow)\b/, "multirow"],
];

/** Flatten `\input` / `\include`; diagnostics name files relative to `baseDir`. */
function expand(
  fs: MockProjectFs,
  path: string,
  baseDir: string,
  keys: ReadonlySet<string>,
  out: Expansion,
  depth: number,
): void {
  const source = text(fs, path) ?? "";
  const shown =
    baseDir !== "" && path.startsWith(`${baseDir}/`)
      ? path.slice(baseDir.length + 1)
      : path;
  const open: { env: string; line: number }[] = [];
  source.split("\n").forEach((raw, index) => {
    const line = raw.replace(/(^|[^\\])%.*$/, "$1");
    const lineNo = index + 1;
    for (const m of line.matchAll(/\\usepackage\s*(?:\[[^\]]*\])?\s*\{([^}]+)\}/g)) {
      for (const name of (m[1] ?? "").split(",")) out.loaded?.add(name.trim());
    }
    if (out.loaded !== undefined) {
      for (const [pattern, pkg] of NEEDS_PACKAGE) {
        const found = pattern.exec(line);
        if (found === null || out.loaded.has(pkg)) continue;
        out.diagnostics.push({
          severity: "error",
          message: `Undefined control sequence: \\${found[1] ?? ""}`,
          file: shown,
          line: lineNo,
          hint: `\`\\${found[1] ?? ""}\` comes from the \`${pkg}\` package: add \`\\usepackage{${pkg}}\` to the preamble`,
        });
      }
    }
    for (const m of line.matchAll(/\\(?:input|include)\s*\{([^}]+)\}/g)) {
      const name = (m[1] ?? "").trim();
      const file = name.endsWith(".tex") ? name : `${name}.tex`;
      const target = joinPath(baseDir, file);
      if (text(fs, target) === undefined || depth > 8) {
        out.diagnostics.push({
          severity: "error",
          message: `File \`${file}' not found.`,
          file: shown,
          line: lineNo,
          hint: "Check the path is relative to the main file and the file exists.",
        });
      } else {
        expand(fs, target, baseDir, keys, out, depth + 1);
      }
    }
    for (const m of line.matchAll(/\\cite[a-z]*\s*\{([^}]+)\}/g)) {
      for (const key of (m[1] ?? "").split(",").map((k) => k.trim())) {
        if (key !== "" && !keys.has(key)) {
          out.diagnostics.push({
            severity: "warning",
            message: `Citation \`${key}' on page 1 undefined`,
            file: shown,
            line: lineNo,
            hint: null,
          });
        }
      }
    }
    for (const m of line.matchAll(/\\(begin|end)\s*\{([^}]+)\}/g)) {
      const env = m[2] ?? "";
      if (m[1] === "begin") open.push({ env, line: lineNo });
      else if (open.at(-1)?.env === env) open.pop();
    }
    out.lines.push({ text: line, origin: { file: shown, line: lineNo } });
  });
  for (const env of open) {
    out.diagnostics.push({
      severity: "error",
      message: `\\begin{${env.env}} on input line ${env.line} ended by end of file.`,
      file: shown,
      line: env.line,
      hint: `Add \\end{${env.env}}.`,
    });
  }
}

/** The text a reader would see, with commands stripped. */
function prose(lines: readonly Row[]): Row[] {
  const out: Row[] = [];
  const blank: Row = { text: "", origin: null };
  let inBody = false;
  for (const { text: line, origin } of lines) {
    if (/\\begin\{document\}/.test(line)) {
      inBody = true;
      continue;
    }
    if (/\\end\{document\}/.test(line)) break;
    if (!inBody) continue;
    const heading = /\\(section|chapter|subsection)\*?\{([^}]*)\}/.exec(line);
    if (heading !== null) {
      out.push(blank, { text: (heading[2] ?? "").toUpperCase(), origin }, blank);
      continue;
    }
    const clean = line
      .replace(/\\[a-zA-Z]+\*?(\[[^\]]*\])?/g, "")
      .replace(/[{}$]/g, "")
      .trim();
    if (clean !== "") out.push({ text: clean, origin });
  }
  return out;
}

function wrap(lines: readonly Row[], width = 74): Row[] {
  const out: Row[] = [];
  for (const { text: paragraph, origin } of lines) {
    if (paragraph === "") {
      out.push({ text: "", origin: null });
      continue;
    }
    let current = "";
    for (const word of paragraph.split(/\s+/)) {
      if (current !== "" && current.length + word.length + 1 > width) {
        out.push({ text: current, origin });
        current = word;
      } else {
        current = current === "" ? word : `${current} ${word}`;
      }
    }
    if (current !== "") out.push({ text: current, origin });
  }
  return out;
}

/** The SyncTeX box of row `i`: 1-based page, points from the top-left corner. */
export function rowBox(i: number, text: string) {
  const k = i % LINES_PER_PAGE;
  const baseline = PDF_PAGE_H - (PDF_FIRST_BASELINE - PDF_LEADING * k);
  return {
    page: Math.floor(i / LINES_PER_PAGE) + 1,
    x: PDF_LEFT,
    y: baseline - PDF_FONT,
    width: Math.max(PDF_FONT, text.length * PDF_FONT * 0.5),
    height: PDF_LEADING,
  };
}

/** The source line of the row nearest `y` on 1-based `page`. */
export function rowAt(rows: readonly Row[], page: number, y: number): Origin | null {
  let best: Origin | null = null;
  let distance = Infinity;
  const first = (page - 1) * LINES_PER_PAGE;
  for (let i = first; i < Math.min(rows.length, first + LINES_PER_PAGE); i += 1) {
    const row = rows[i];
    if (row?.origin === null || row === undefined) continue;
    const box = rowBox(i, row.text);
    const d = Math.abs(box.y + box.height / 2 - y);
    if (d < distance) {
      distance = d;
      best = row.origin;
    }
  }
  return best;
}

function pdfString(value: string): string {
  return value.replace(/[^\x20-\x7e]/g, "?").replace(/([\\()])/g, "\\$1");
}

/** A valid multi-page PDF (A4, Helvetica) with one text line per entry. */
export function makePdf(lines: readonly string[]): {
  bytes: Uint8Array;
  pages: number;
} {
  const chunks: string[][] = [];
  for (let i = 0; i < Math.max(lines.length, 1); i += LINES_PER_PAGE) {
    chunks.push(lines.slice(i, i + LINES_PER_PAGE));
  }
  const objects: string[] = [];
  const pageIds = chunks.map((_, i) => 4 + i * 2);
  objects[1] = "<< /Type /Catalog /Pages 2 0 R >>";
  objects[2] = `<< /Type /Pages /Kids [${pageIds.map((id) => `${id} 0 R`).join(" ")}] /Count ${chunks.length} >>`;
  objects[3] = "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>";
  chunks.forEach((chunk, i) => {
    const id = 4 + i * 2;
    const stream = [
      "BT /F1 11 Tf 14 TL 56 780 Td",
      ...chunk.map((line) => `(${pdfString(line)}) Tj T*`),
      "ET",
    ].join("\n");
    objects[id] =
      `<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 3 0 R >> >> /Contents ${id + 1} 0 R >>`;
    objects[id + 1] = `<< /Length ${stream.length} >>\nstream\n${stream}\nendstream`;
  });
  let body = "%PDF-1.4\n";
  const offsets: number[] = [];
  for (let id = 1; id < objects.length; id += 1) {
    offsets[id] = body.length;
    body += `${id} 0 obj\n${objects[id] ?? ""}\nendobj\n`;
  }
  const xref = body.length;
  body += `xref\n0 ${objects.length}\n0000000000 65535 f \n`;
  for (let id = 1; id < objects.length; id += 1) {
    body += `${String(offsets[id]).padStart(10, "0")} 00000 n \n`;
  }
  body += `trailer\n<< /Size ${objects.length} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return { bytes: new TextEncoder().encode(body), pages: chunks.length };
}

/** `tex: false` imitates a machine with no TeX installation and no built-in engine. */
export function latexPlugin(
  host: MockProjectHost,
  { tex = true }: { tex?: boolean } = {},
): MockPluginFactory {
  return (): MockPlugin => {
    const builds = new Map<string, Built>();
    let last: Built | undefined;

    return {
      name: "latex",
      commands: COMMANDS,
      handlers: {
        "latex.compile": (args): LatexReport => {
          const fs = host.require();
          const main = args["main"];
          if (typeof main !== "string") {
            throw {
              message:
                "[bad_argument] `main` is not a valid argument: must be a string",
            };
          }
          if (!tex) {
            throw {
              message:
                "[no_backend] no LaTeX backend: no TeX installation was found on PATH and the built-in engine is not installed",
            };
          }
          const entry = main.startsWith(`${fs.root}/`)
            ? main.slice(fs.root.length + 1)
            : main;
          if (text(fs, entry) === undefined) {
            throw { message: `[io] ${main}: No such file or directory (os error 2)` };
          }
          const base = dirOf(entry);
          const out: Expansion = { lines: [], diagnostics: [], loaded: new Set() };
          expand(fs, entry, base, bibKeys(fs), out, 0);
          const ok = out.diagnostics.every((d) => d.severity !== "error");
          let pages: number | null = null;
          if (ok) {
            const rows = wrap(prose(out.lines));
            const made = makePdf(rows.map((row) => row.text));
            const built = { pdf: made.bytes, pages: made.pages, rows };
            builds.set(main, built);
            last = built;
            pages = made.pages;
          } else {
            builds.delete(main);
          }
          return {
            ok,
            pages,
            diagnostics: out.diagnostics,
            duration_ms: Math.round(380 + out.lines.length * 2),
            synctex: ok,
            engine: "pdflatex",
            backend: "system",
            tex_version: "pdfTeX 3.141592653-2.6-1.40.27 (TeX Live 2025)",
          };
        },
        "latex.status": () => ({
          backends: [
            {
              id: "system",
              available: tex,
              version: tex ? "pdfTeX 3.141592653-2.6-1.40.27 (TeX Live 2025)" : null,
              engines: tex ? ["pdflatex"] : [],
              latexmk: tex,
              bibtex: tex,
              biber: false,
            },
            {
              id: "tectonic",
              available: false,
              version: null,
              engines: ["xelatex"],
              latexmk: false,
              bibtex: false,
              biber: false,
            },
          ],
          default: tex ? "system" : "none",
          default_engine: "pdflatex",
          settings: {},
        }),
        "latex.synctex": (args) => {
          const main = args["main"];
          const built = typeof main === "string" ? builds.get(main) : last;
          if (built === undefined) {
            throw {
              message: "[no_build] no build is available; run `latex.compile` first",
            };
          }
          const file = args["file"];
          if (typeof file === "string") {
            const line = args["line"];
            if (typeof line !== "number" || !Number.isInteger(line) || line < 0) {
              throw {
                message:
                  "[bad_argument] `line` is not a valid argument: must be a number",
              };
            }
            const wanted = file.replace(/^\.\//, "");
            const candidates = built.rows
              .map((row, i) => ({ row, i }))
              .filter(({ row }) => row.origin?.file === wanted);
            if (candidates.length === 0) {
              throw {
                message: `[synctex_no_match] no SyncTeX match: \`${file}\` is not part of the build`,
              };
            }
            // The line itself, else the nearest following line that printed.
            const lines = candidates.map(({ row }) => row.origin?.line ?? 0);
            const best =
              lines.filter((l) => l >= line).sort((a, b) => a - b)[0] ??
              Math.max(...lines);
            const hit = candidates.find(({ row }) => row.origin?.line === best);
            return {
              direction: "forward",
              positions: hit === undefined ? [] : [rowBox(hit.i, hit.row.text)],
            };
          }
          const page = args["page"];
          const y = args["y"];
          if (typeof page !== "number" || typeof y !== "number") {
            throw {
              message:
                "[bad_argument] `file|page` is not a valid argument: give `file` and `line` (forward) or `page`, `x` and `y` (reverse)",
            };
          }
          const at = rowAt(built.rows, page, y);
          if (at === null) {
            throw {
              message: `[synctex_no_match] no SyncTeX match: page ${page} has no mapped content`,
            };
          }
          return { direction: "reverse", file: at.file, line: at.line };
        },
      },
      binaryHandlers: {
        "latex.export-pdf": (args) => {
          const main = args["main"];
          const built = typeof main === "string" ? builds.get(main) : last;
          if (built === undefined) {
            throw {
              message:
                typeof main === "string"
                  ? "[compile_failed] the last compile produced no PDF: see diagnostics"
                  : "[no_build] no build is available; run `latex.compile` first",
            };
          }
          return built.pdf;
        },
      },
    };
  };
}
