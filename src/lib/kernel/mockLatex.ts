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
  ["latex.compile", "Compile LaTeX"],
  ["latex.export-pdf", "Export PDF"],
  ["latex.synctex", "SyncTeX source/PDF lookup"],
];

const LINES_PER_PAGE = 42;

interface Built {
  readonly pdf: Uint8Array;
  readonly pages: number;
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
  readonly lines: string[];
  readonly diagnostics: LatexDiagnostic[];
}

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
    out.lines.push(line);
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
function prose(lines: readonly string[]): string[] {
  const out: string[] = [];
  let inBody = false;
  for (const line of lines) {
    if (/\\begin\{document\}/.test(line)) {
      inBody = true;
      continue;
    }
    if (/\\end\{document\}/.test(line)) break;
    if (!inBody) continue;
    const heading = /\\(section|chapter|subsection)\*?\{([^}]*)\}/.exec(line);
    if (heading !== null) {
      out.push("", (heading[2] ?? "").toUpperCase(), "");
      continue;
    }
    const clean = line
      .replace(/\\[a-zA-Z]+\*?(\[[^\]]*\])?/g, "")
      .replace(/[{}$]/g, "")
      .trim();
    if (clean !== "") out.push(clean);
  }
  return out;
}

function wrap(lines: readonly string[], width = 74): string[] {
  const out: string[] = [];
  for (const paragraph of lines) {
    if (paragraph === "") {
      out.push("");
      continue;
    }
    let current = "";
    for (const word of paragraph.split(/\s+/)) {
      if (current !== "" && current.length + word.length + 1 > width) {
        out.push(current);
        current = word;
      } else {
        current = current === "" ? word : `${current} ${word}`;
      }
    }
    if (current !== "") out.push(current);
  }
  return out;
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

export function latexPlugin(host: MockProjectHost): MockPluginFactory {
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
          const entry = main.startsWith(`${fs.root}/`)
            ? main.slice(fs.root.length + 1)
            : main;
          if (text(fs, entry) === undefined) {
            throw { message: `[io] ${main}: No such file or directory (os error 2)` };
          }
          const base = dirOf(entry);
          const out: Expansion = { lines: [], diagnostics: [] };
          expand(fs, entry, base, bibKeys(fs), out, 0);
          const ok = out.diagnostics.every((d) => d.severity !== "error");
          let pages: number | null = null;
          if (ok) {
            const made = makePdf(wrap(prose(out.lines)));
            const built = { pdf: made.bytes, pages: made.pages };
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
        "latex.synctex": () => {
          throw {
            message:
              "[synctex_unavailable] SyncTeX data was not produced for this build",
          };
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
