/**
 * In-memory `heal` plugin for the browser.
 *
 * A subset of `crates/manang-heal`, returning the same shapes (`fixes[]` with
 * UTF-16 edits, `confidence`, `rule`, `fix_all`, `findings`), enough to
 * exercise the Fix button, the chooser, Fix all, the notice and the
 * verification loop without the engine. The Rust crate is the authority and
 * has many more rules (TikZ libraries, theorems, balanced braces and
 * environments, the Typst package table, close-match misspellings); this keeps
 * the package tables to the common entries.
 */

import type { Args, KernelError } from "./client";
import type {
  HealConfidence,
  HealDiagnosticIn,
  HealEdit,
  HealFinding,
  HealFix,
  HealFixAll,
} from "./healApi";
import type { MockPlugin, MockPluginFactory } from "./mock";
import { scalarToUtf16 } from "./offsets";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["heal.suggest", "Suggest fixes for compile errors"],
  ["heal.configure", "Apply self-healing settings"],
];

function fail(message: string): KernelError {
  return { message };
}

/* -------------------------------------------------------------------------- */
/* LaTeX                                                                       */
/* -------------------------------------------------------------------------- */

/** `[command, packages]`; the first package is the usual one. */
const LATEX_COMMANDS: readonly (readonly [readonly string[], readonly string[]])[] = [
  [
    ["\\includegraphics", "\\graphicspath", "\\rotatebox", "\\scalebox", "\\resizebox"],
    ["graphicx"],
  ],
  [
    ["\\textcolor", "\\color", "\\definecolor", "\\colorbox", "\\fcolorbox"],
    ["xcolor"],
  ],
  [["\\url"], ["hyperref", "url"]],
  [["\\href", "\\hyperref", "\\autoref", "\\nameref"], ["hyperref"]],
  [["\\mathbb", "\\mathfrak", "\\checkmark", "\\leqslant", "\\geqslant"], ["amssymb"]],
  [
    [
      "\\text",
      "\\tag",
      "\\eqref",
      "\\dfrac",
      "\\tfrac",
      "\\binom",
      "\\boldsymbol",
      "\\substack",
      "\\intertext",
      "\\operatorname",
    ],
    ["amsmath"],
  ],
  [["\\toprule", "\\midrule", "\\bottomrule", "\\cmidrule"], ["booktabs"]],
  [["\\multirow"], ["multirow"]],
  [["\\SI", "\\si", "\\num", "\\qty", "\\unit"], ["siunitx"]],
  [["\\lstinline", "\\lstset"], ["listings"]],
  [["\\subfloat", "\\subcaption", "\\captionof"], ["subcaption"]],
  [["\\cref", "\\Cref", "\\crefrange"], ["cleveref"]],
  [["\\citep", "\\citet", "\\citeauthor", "\\citeyear", "\\citealp"], ["natbib"]],
  [["\\bm"], ["bm"]],
  [["\\tikz", "\\draw", "\\node", "\\fill", "\\path", "\\usetikzlibrary"], ["tikz"]],
  [["\\includepdf"], ["pdfpages"]],
  [["\\FloatBarrier"], ["placeins"]],
  [["\\xspace"], ["xspace"]],
  [["\\cancel", "\\bcancel"], ["cancel"]],
  [
    ["\\printbibliography", "\\addbibresource", "\\autocite", "\\parencite"],
    ["biblatex"],
  ],
  [["\\enquote"], ["csquotes"]],
];

const LATEX_ENVIRONMENTS: readonly (readonly [readonly string[], string])[] = [
  [
    [
      "align",
      "align*",
      "equation*",
      "gather",
      "gather*",
      "multline",
      "split",
      "cases",
      "aligned",
      "pmatrix",
      "bmatrix",
    ],
    "amsmath",
  ],
  [["tikzpicture"], "tikz"],
  [["lstlisting"], "listings"],
  [["subfigure"], "subcaption"],
  [["tabularx"], "tabularx"],
  [["longtable"], "longtable"],
  [["multicols"], "multicol"],
];

function maskLatex(text: string): string {
  return text.replace(
    /(^|[^\\])%[^\n]*/g,
    (m, lead: string) => lead + " ".repeat(m.length - lead.length),
  );
}

interface Preamble {
  packages: Set<string>;
  insertAt: number | null;
  documentStart: number;
}

function scanPreamble(masked: string): Preamble {
  const begin = masked.search(/\\begin\s*\{document\}/);
  const head = begin === -1 ? masked : masked.slice(0, begin);
  const packages = new Set<string>();
  let insertAt: number | null = null;
  for (const m of head.matchAll(
    /\\(?:usepackage|RequirePackage)\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}/g,
  )) {
    for (const name of (m[1] ?? "").split(",")) packages.add(name.trim());
    insertAt = m.index + m[0].length;
  }
  if (insertAt === null) {
    const cls = /\\documentclass\s*(?:\[[^\]]*\])?\s*\{[^}]*\}/.exec(head);
    if (cls !== null) insertAt = cls.index + cls[0].length;
  }
  return { packages, insertAt, documentStart: begin === -1 ? masked.length : begin };
}

/** Insert `lines` after the text ending at `pos`, keeping a trailing comment. */
function insertLinesAfter(text: string, masked: string, pos: number, lines: string[]) {
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  const rest = /[\r\n]|$/.exec(text.slice(pos));
  const lineEnd = pos + (rest?.index ?? text.length - pos);
  const body = lines.map((l) => `${eol}${l}`).join("");
  return masked.slice(pos, lineEnd).trim() === ""
    ? { from: lineEnd, to: lineEnd, insert: body }
    : { from: pos, to: pos, insert: `${body}${eol}` };
}

function latexFixes(text: string, diagnostics: readonly HealDiagnosticIn[]): HealFix[] {
  const masked = maskLatex(text);
  const pre = scanPreamble(masked);
  const lines = text.split("\n");
  const out: HealFix[] = [];
  const add = (
    index: number,
    pkg: string,
    why: string,
    confidence: HealConfidence,
    rule: string,
  ) => {
    if (pre.insertAt === null) return;
    const id = `latex.package:${pkg}`;
    const known = out.find((f) => f.id === id);
    if (known !== undefined) {
      (known.diagnostics as number[]).push(index);
      return;
    }
    const line = `\\usepackage{${pkg}}`;
    out.push({
      id,
      label: `Add ${line}`,
      why,
      edits: [insertLinesAfter(text, masked, pre.insertAt, [line])],
      confidence,
      rule,
      diagnostics: [index],
    });
  };
  diagnostics.forEach((d, index) => {
    if (d.severity !== "error") return;
    if (d.message.startsWith("Undefined control sequence")) {
      const named = /:\s*(\\[A-Za-z@]+)\s*$/.exec(d.message)?.[1];
      let tokens: string[] = named === undefined ? [] : [named];
      if (named === undefined) {
        const line =
          d.line === null || d.line === undefined ? "" : (lines[d.line - 1] ?? "");
        tokens = [...maskLatex(line).matchAll(/\\[A-Za-z@]+/g)].map((m) => m[0]);
      }
      const exact = named !== undefined || tokens.length === 1;
      for (const token of new Set(tokens)) {
        const row = LATEX_COMMANDS.find(([names]) => names.includes(token));
        if (row === undefined) continue;
        const packages = row[1];
        if (packages.some((p) => pre.packages.has(p))) continue;
        for (const pkg of packages) {
          add(
            index,
            pkg,
            `\`${token}\` is defined by the \`${pkg}\` package, which this document does not load.`,
            packages.length === 1 && exact ? "certain" : "likely",
            "latex.missing-package",
          );
        }
      }
    }
    const env = /Environment ([^\s]+) undefined/.exec(d.message)?.[1];
    if (env !== undefined) {
      const row = LATEX_ENVIRONMENTS.find(([names]) => names.includes(env));
      if (row !== undefined && !pre.packages.has(row[1])) {
        add(
          index,
          row[1],
          `The \`${env}\` environment is defined by the \`${row[1]}\` package, which this document does not load.`,
          "certain",
          "latex.missing-environment-package",
        );
      }
    }
  });
  return out;
}

/* -------------------------------------------------------------------------- */
/* Typst                                                                       */
/* -------------------------------------------------------------------------- */

const TYPST_PACKAGES: readonly {
  name: string;
  version: string;
  symbols: readonly string[];
}[] = [
  { name: "cetz", version: "0.5.2", symbols: ["canvas", "draw"] },
  { name: "fletcher", version: "0.5.8", symbols: ["diagram", "node", "edge"] },
  { name: "codly", version: "2.0.0", symbols: ["codly"] },
  { name: "showybox", version: "2.0.4", symbols: ["showybox"] },
];

const TYPST_KEYWORDS = ["set", "show", "let", "import", "include"];

function withinOneEdit(a: string, b: string): boolean {
  if (a === b) return false;
  if (Math.abs(a.length - b.length) > 1) return false;
  if (a.length === b.length) {
    const diffs = [...a].flatMap((c, i) => (c === b[i] ? [] : [i]));
    if (diffs.length === 1) return true;
    const [i, j] = diffs;
    return (
      diffs.length === 2 &&
      i !== undefined &&
      j !== undefined &&
      j === i + 1 &&
      a[i] === b[j] &&
      a[j] === b[i]
    );
  }
  const [short, long] = a.length < b.length ? [a, b] : [b, a];
  let skip = 0;
  while (skip < short.length && short[skip] === long[skip]) skip += 1;
  return short.slice(skip) === long.slice(skip + 1);
}

function typstFixes(text: string, diagnostics: readonly HealDiagnosticIn[]): HealFix[] {
  const out: HealFix[] = [];
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  diagnostics.forEach((d, index) => {
    if (d.severity !== "error") return;
    const name = /^unknown variable: (.+)$/.exec(d.message)?.[1]?.trim();
    if (name !== undefined && d.range !== null && d.range !== undefined) {
      const from = scalarToUtf16(text, d.range.start);
      const to = scalarToUtf16(text, d.range.end);
      if (text.slice(from, to) !== name) return;
      const pkg = TYPST_PACKAGES.find((p) => p.symbols.includes(name));
      if (pkg !== undefined) {
        const spec = `@preview/${pkg.name}:${pkg.version}`;
        const used = pkg.symbols.filter(
          (s) => s === name || new RegExp(`(^|[^\\w.\\-"@/])${s}[(.]`).test(text),
        );
        const lines = text.split("\n");
        let at = 0;
        let offset = 0;
        let lastImport: number | null = null;
        let leading = true;
        for (const line of lines) {
          const t = line.trimStart();
          if (t.startsWith("#import ")) {
            lastImport = offset + line.length + 1;
            leading = false;
          } else if (leading && (t.startsWith("//") || t.trim() === "")) {
            at = offset + line.length + 1;
          } else leading = false;
          offset += line.length + 1;
        }
        at = Math.min(lastImport ?? at, text.length);
        const lead = at > 0 && text[at - 1] !== "\n" ? eol : "";
        out.push({
          id: `typst.import:${pkg.name}@${pkg.version}:${used.join(", ")}`,
          label: `Import ${used.join(", ")} from ${spec}`,
          why: `\`${name}\` comes from the \`${pkg.name}\` package, which this document does not import.`,
          edits: [
            {
              from: at,
              to: at,
              insert: `${lead}#import "${spec}": ${used.join(", ")}${eol}`,
            },
          ],
          confidence: "certain",
          rule: "typst.missing-import",
          diagnostics: [index],
        });
        return;
      }
      if (name.length >= 3) {
        const matches = TYPST_KEYWORDS.filter((k) => withinOneEdit(name, k));
        const only = matches[0];
        if (matches.length === 1 && only !== undefined) {
          out.push({
            id: `typst.rename:${name}->${only}:${from}`,
            label: `Change ${name} to ${only}`,
            why: `\`${name}\` is not defined; \`${only}\` is the only keyword one letter away.`,
            edits: [{ from, to, insert: only }],
            confidence: "likely",
            rule: "typst.misspelled-name",
            diagnostics: [index],
          });
        }
      }
      return;
    }
    if (
      d.message === "unclosed delimiter" &&
      d.range !== null &&
      d.range !== undefined
    ) {
      const from = scalarToUtf16(text, d.range.start);
      const open = text[from];
      const close = open === "(" ? ")" : open === "[" ? "]" : open === "{" ? "}" : null;
      if (close === null) return;
      // The block ends with the last line indented deeper than the opener's.
      const lineStart = text.lastIndexOf("\n", from - 1) + 1;
      const rest = text.slice(lineStart).split("\n");
      const indent = (rest[0] ?? "").length - (rest[0] ?? "").trimStart().length;
      let end = lineStart + (rest[0] ?? "").length;
      for (let i = 1; i < rest.length; i += 1) {
        const line = rest[i] ?? "";
        if (line.trim() === "" || line.length - line.trimStart().length <= indent)
          break;
        end += 1 + line.length;
      }
      const at =
        end - (text.slice(0, end).length - text.slice(0, end).trimEnd().length);
      out.push({
        id: `typst.close:${close}:${at}`,
        label: `Add ${close} to close the \`${open}\` on line ${text.slice(0, from).split("\n").length}`,
        why: "A delimiter is opened and never closed; this closes it where the statement appears to end.",
        edits: [{ from: at, to: at, insert: close }],
        confidence: "likely",
        rule: "typst.unclosed-delimiter",
        diagnostics: [index],
      });
    }
  });
  return out;
}

/* -------------------------------------------------------------------------- */
/* Markdown                                                                    */
/* -------------------------------------------------------------------------- */

function markdownFixes(text: string): { fixes: HealFix[]; findings: HealFinding[] } {
  const lines = text.split("\n");
  let open: { line: number; start: number; end: number; fence: string } | null = null;
  let offset = 0;
  lines.forEach((raw, i) => {
    const line = raw.replace(/\r$/, "");
    const m = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line);
    if (m !== null) {
      const fence = m[1] ?? "";
      if (open === null) {
        if (!(fence.startsWith("`") && (m[2] ?? "").includes("`"))) {
          open = { line: i + 1, start: offset, end: offset + line.length, fence };
        }
      } else if (
        fence[0] === open.fence[0] &&
        fence.length >= open.fence.length &&
        (m[2] ?? "").trim() === ""
      ) {
        open = null;
      }
    }
    offset += raw.length + 1;
  });
  if (open === null) return { fixes: [], findings: [] };
  const o: { line: number; start: number; end: number; fence: string } = open;
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  const lead = text === "" || text.endsWith("\n") ? "" : eol;
  return {
    findings: [
      {
        message: "Code fence is never closed",
        severity: "warning",
        from: o.start,
        to: o.end,
        line: o.line,
        rule: "markdown.unclosed-fence",
      },
    ],
    fixes: [
      {
        id: `markdown.close-fence:${o.start}`,
        label: `Close the code fence with ${o.fence}`,
        why: "The code fence opened here is never closed, so everything after it is code.",
        edits: [
          { from: text.length, to: text.length, insert: `${lead}${o.fence}${eol}` },
        ],
        confidence: "certain",
        rule: "markdown.unclosed-fence",
        diagnostics: [],
        finding: 0,
      },
    ],
  };
}

/* -------------------------------------------------------------------------- */
/* Fix all and the plugin                                                      */
/* -------------------------------------------------------------------------- */

function fixAll(fixes: readonly HealFix[]): HealFixAll | undefined {
  const chosen: HealFix[] = [];
  for (const fix of fixes) {
    if (fix.confidence !== "certain") continue;
    if (chosen.some((c) => c.diagnostics.some((i) => fix.diagnostics.includes(i))))
      continue;
    chosen.push(fix);
  }
  if (chosen.length < 2) return undefined;
  const edits: HealEdit[] = chosen
    .flatMap((f) => [...f.edits])
    .sort((a, b) => a.from - b.from || a.to - b.to);
  return {
    label: `Fix all (${chosen.length})`,
    fixes: chosen.map((f) => f.id),
    edits,
  };
}

function diagnosticsOf(args: Args): HealDiagnosticIn[] {
  const value = args["diagnostics"];
  if (value === undefined || value === null) return [];
  if (!Array.isArray(value))
    throw fail("bad argument `diagnostics`: expected an array");
  return value as HealDiagnosticIn[];
}

export function healPlugin(): MockPluginFactory {
  return (): MockPlugin => {
    let enabled = true;
    let autoApply = false;
    return {
      name: "heal",
      commands: COMMANDS,
      handlers: {
        "heal.suggest": (args) => {
          const engine = args["engine"];
          const text = args["text"];
          if (typeof engine !== "string")
            throw fail("bad argument `engine`: required string");
          if (typeof text !== "string")
            throw fail("bad argument `text`: required string");
          if (!["typst", "latex", "markdown"].includes(engine)) {
            throw fail(
              `unknown engine \`${engine}\`: expected \`typst\`, \`latex\` or \`markdown\``,
            );
          }
          if (!enabled) return { fixes: [], findings: [], enabled };
          const diagnostics = diagnosticsOf(args);
          let fixes: HealFix[];
          let findings: HealFinding[] = [];
          if (engine === "latex") fixes = latexFixes(text, diagnostics);
          else if (engine === "typst") fixes = typstFixes(text, diagnostics);
          else ({ fixes, findings } = markdownFixes(text));
          fixes.sort(
            (a, b) =>
              (a.confidence === "certain" ? 0 : 1) -
              (b.confidence === "certain" ? 0 : 1),
          );
          const all = fixAll(fixes);
          return {
            fixes,
            ...(all === undefined ? {} : { fix_all: all }),
            findings,
            enabled,
          };
        },
        "heal.configure": (args) => {
          const values = args["values"];
          if (typeof values !== "object" || values === null) {
            throw fail("bad argument `values`: expected an object");
          }
          const v = values as Record<string, unknown>;
          for (const id of ["heal.enabled", "heal.auto-apply"]) {
            if (v[id] !== undefined && typeof v[id] !== "boolean") {
              throw fail(`bad argument \`values\`: \`${id}\` must be a boolean`);
            }
          }
          if (typeof v["heal.enabled"] === "boolean") enabled = v["heal.enabled"];
          if (typeof v["heal.auto-apply"] === "boolean")
            autoApply = v["heal.auto-apply"];
          return { "heal.enabled": enabled, "heal.auto-apply": autoApply };
        },
      },
    };
  };
}
