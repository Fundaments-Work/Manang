/**
 * In-memory `writing` plugin for the browser.
 *
 * Not a spell checker or a formatter: a tiny word list and a few rules that
 * return the exact shapes of `crates/manang-writing` (`manifest.toml`,
 * `NOTES.md`): offsets are Unicode scalars, spelling gives `{language, count,
 * misspelled}`, lint gives `{source: "writing", diagnostics}` with a `code` and
 * an `info` severity, and so on. Enough to exercise squiggles, the quick-fix
 * menu, Problems, completions, snippets and the word count without the engine.
 */

import type { Args, KernelError } from "./client";
import type { MockPlugin, MockPluginFactory } from "./mock";
import { scalarLength } from "./offsets";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["writing.format.document", "Format document"],
  ["writing.format.range", "Format selection"],
  ["writing.spell.check", "Check spelling"],
  ["writing.spell.suggest", "Suggest spellings"],
  ["writing.spell.add-word", "Add word to user dictionary"],
  ["writing.spell.remove-word", "Remove word from user dictionary"],
  ["writing.spell.words", "List user dictionary words"],
  ["writing.spell.languages", "List spelling languages"],
  ["writing.lint.prose", "Lint prose"],
  ["writing.symbols.search", "Search math symbols"],
  ["writing.snippets.list", "List snippets"],
  ["writing.snippets.get", "Get snippet"],
  ["writing.snippets.expand", "Expand snippet"],
  ["writing.stats.count", "Word count and statistics"],
];

function fail(message: string): KernelError {
  return { message };
}

function str(args: Args, key: string): string {
  const value = args[key];
  if (typeof value !== "string") throw fail(`bad argument \`${key}\`: required string`);
  return value;
}

function optStr(args: Args, key: string): string | undefined {
  const value = args[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string")
    throw fail(`bad argument \`${key}\`: expected a string`);
  return value;
}

function optNum(args: Args, key: string): number | undefined {
  const value = args[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0) {
    throw fail(`bad argument \`${key}\`: expected a non-negative integer`);
  }
  return value;
}

type Syntax = "typst" | "latex" | "markdown" | "plain";

function syntaxOf(args: Args, text: string): Syntax {
  const given = optStr(args, "syntax");
  if (given === undefined || given === "auto") {
    if (/\\(documentclass|begin\{|usepackage)/.test(text)) return "latex";
    if (/^#{1,6} /m.test(text) && !/^#(set|import|let|show)\b/m.test(text)) {
      return "markdown";
    }
    return "typst";
  }
  if (given === "typst" || given === "typ") return "typst";
  if (given === "latex" || given === "tex") return "latex";
  if (given === "markdown" || given === "md") return "markdown";
  if (given === "plain" || given === "text") return "plain";
  throw fail(
    `bad argument \`syntax\`: unknown syntax \`${given}\`; use typst, latex, markdown or plain`,
  );
}

/* -------------------------------------------------------------------------- */
/* Masking: markup is not prose                                                */
/* -------------------------------------------------------------------------- */

/** A non-letter astral character, so masking never changes a scalar count. */
const ASTRAL_FILL = "\u{1F4A0}";

function blank(match: string): string {
  let out = "";
  for (const ch of match) {
    if (ch === "\n") out += "\n";
    else out += ch.length === 2 ? ASTRAL_FILL : " ";
  }
  return out;
}

const MASKS: Record<Syntax, readonly RegExp[]> = {
  typst: [
    /\/\*[\s\S]*?\*\//g,
    /\/\/[^\n]*/g,
    /```[\s\S]*?```/g,
    /`[^`\n]*`/g,
    /\$[^$]*\$/g,
    /#[A-Za-z_][\w.-]*/g,
    /@[A-Za-z][\w:-]*/g,
    /<[A-Za-z][\w:-]*>/g,
    /"[^"\n]*"/g,
  ],
  markdown: [
    /```[\s\S]*?```/g,
    /`[^`\n]*`/g,
    /\$[^$\n]*\$/g,
    /\]\([^)\n]*\)/g,
    /<[^>\n]+>/g,
  ],
  latex: [
    /%[^\n]*/g,
    /\$\$[\s\S]*?\$\$/g,
    /\$[^$]*\$/g,
    /\\begin\{(equation|align|verbatim|lstlisting)\*?\}[\s\S]*?\\end\{\1\*?\}/g,
    /\\[A-Za-z]+\*?(\[[^\]]*\])?(\{[^}]*\})?/g,
  ],
  plain: [],
};

export function maskProse(text: string, syntax: Syntax): string {
  let out = text;
  for (const pattern of MASKS[syntax]) out = out.replace(pattern, blank);
  return out;
}

/* -------------------------------------------------------------------------- */
/* Spelling                                                                    */
/* -------------------------------------------------------------------------- */

/** The few words the demo knows, for suggestions. */
const VOCABULARY = [
  "the",
  "their",
  "there",
  "which",
  "because",
  "believe",
  "receive",
  "address",
  "definitely",
  "separate",
  "occurred",
  "necessary",
  "environment",
  "government",
  "accommodate",
  "experiment",
  "temperature",
  "entropy",
  "equation",
  "analysis",
  "approximately",
  "beginning",
  "calendar",
  "embarrass",
  "february",
  "friend",
  "ocurrence",
  "quantum",
  "writing",
];

/** The demo's misspellings: what hunspell would flag in the sample documents. */
const KNOWN_TYPOS: Readonly<Record<string, readonly string[]>> = {
  teh: ["the", "tech", "tea"],
  recieve: ["receive"],
  adress: ["address", "address's"],
  definately: ["definitely", "defiantly"],
  seperate: ["separate"],
  occured: ["occurred", "occurred"],
  wich: ["which", "witch"],
  thier: ["their", "there"],
  becuase: ["because"],
  beleive: ["believe"],
  neccessary: ["necessary"],
  enviroment: ["environment"],
  goverment: ["government"],
  accomodate: ["accommodate"],
  experment: ["experiment"],
  temprature: ["temperature"],
  equasion: ["equation"],
  freind: ["friend"],
  begining: ["beginning"],
  ocurrence: ["occurrence"],
  quantem: ["quantum"],
  entropie: ["entropy"],
  writting: ["writing"],
};

function editDistance(a: string, b: string): number {
  const row = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i += 1) {
    let prev = row[0] ?? 0;
    row[0] = i;
    for (let j = 1; j <= b.length; j += 1) {
      const above = row[j] ?? 0;
      row[j] = Math.min(
        above + 1,
        (row[j - 1] ?? 0) + 1,
        prev + (a[i - 1] === b[j - 1] ? 0 : 1),
      );
      prev = above;
    }
  }
  return row[b.length] ?? 0;
}

function suggestionsFor(word: string, limit: number): string[] {
  const lower = word.toLowerCase();
  const direct = KNOWN_TYPOS[lower] ?? [];
  const near = VOCABULARY.filter((w) => editDistance(lower, w) <= 2 && w !== lower);
  const matchCase = (w: string): string =>
    word[0] !== undefined &&
    word[0] === word[0].toUpperCase() &&
    word[0] !== word[0].toLowerCase()
      ? w.charAt(0).toUpperCase() + w.slice(1)
      : w;
  return [...new Set([...direct, ...near])].slice(0, limit).map(matchCase);
}

interface Found {
  word: string;
  range: { start: number; end: number };
  line: number;
  column: number;
  suggestions: string[];
}

function misspellings(
  text: string,
  syntax: Syntax,
  user: ReadonlySet<string>,
  maxSuggestions: number,
): Found[] {
  const masked = maskProse(text, syntax);
  const found: Found[] = [];
  const pattern = /\p{L}[\p{L}'’]*/gu;
  let lineNumber = 1;
  let lineStart = 0;
  let cursor = 0;
  for (const match of masked.matchAll(pattern)) {
    const word = match[0].replace(/['’]+$/, "");
    const at = match.index;
    if (!(word.toLowerCase() in KNOWN_TYPOS) || user.has(word.toLowerCase())) continue;
    // Advance the line bookkeeping lazily; matches arrive in order.
    for (let i = cursor; i < at; i += 1) {
      if (masked.charCodeAt(i) === 10) {
        lineNumber += 1;
        lineStart = i + 1;
      }
    }
    cursor = at;
    const start = scalarLength(masked.slice(0, at));
    found.push({
      word,
      range: { start, end: start + scalarLength(word) },
      line: lineNumber,
      column: scalarLength(masked.slice(lineStart, at)) + 1,
      suggestions: suggestionsFor(word, maxSuggestions),
    });
  }
  return found;
}

/* -------------------------------------------------------------------------- */
/* Lint                                                                        */
/* -------------------------------------------------------------------------- */

interface Diag {
  severity: "error" | "warning" | "info";
  message: string;
  file: string | null;
  line: number | null;
  column: number | null;
  range: { start: number; end: number } | null;
  hints: string[];
  trace: string[];
  code: string;
}

function lintProse(text: string, syntax: Syntax, file: string | null): Diag[] {
  const masked = maskProse(text, syntax);
  const out: Diag[] = [];
  const place = (
    at: number,
    length: number,
    severity: Diag["severity"],
    code: string,
    message: string,
    hints: string[],
  ): void => {
    const before = masked.slice(0, at);
    const lineStart = before.lastIndexOf("\n") + 1;
    const start = scalarLength(before);
    out.push({
      severity,
      message,
      file,
      line: before.split("\n").length,
      column: scalarLength(before.slice(lineStart)) + 1,
      range: { start, end: start + length },
      hints,
      trace: [],
      code,
    });
  };

  for (const match of masked.matchAll(/\b(\p{L}+)(\s+)\1\b/giu)) {
    const word = match[1] ?? "";
    // `...` and `\n\n` between the words end the sentence; only plain gaps count.
    if (/\n\s*\n/.test(match[2] ?? "")) continue;
    place(
      match.index,
      scalarLength(match[0]),
      "warning",
      "repeated-word",
      `Repeated word: "${word}"`,
      [`Delete the second "${word}"`],
    );
  }
  for (const match of masked.matchAll(/\b(very|really|basically|obviously)\b/gi)) {
    place(
      match.index,
      scalarLength(match[0]),
      "info",
      "weak-word",
      `"${match[0]}" adds little; consider a stronger word or cutting it`,
      [],
    );
  }
  return out.sort((a, b) => (a.range?.start ?? 0) - (b.range?.start ?? 0));
}

/* -------------------------------------------------------------------------- */
/* Formatting                                                                  */
/* -------------------------------------------------------------------------- */

/** Trailing spaces, runs of blank lines and a missing final newline: the safe parts. */
function formatText(source: string, language: Syntax): string {
  if (language === "latex") {
    throw fail("unsupported: LaTeX formatting is not available yet");
  }
  const lines = source.split("\n").map((line) => line.replace(/[ \t]+$/, ""));
  let text = lines.join("\n").replace(/\n{3,}/g, "\n\n");
  if (text !== "" && !text.endsWith("\n")) text += "\n";
  return text;
}

/* -------------------------------------------------------------------------- */
/* Symbols                                                                     */
/* -------------------------------------------------------------------------- */

interface SymbolRow {
  name: string;
  typst: string | null;
  latex: string | null;
  preview: string;
  unicode: string | null;
  category: string;
  requires: string | null;
  keywords: string[];
}

const SYMBOLS: readonly SymbolRow[] = [
  ["alpha", "alpha", "\\alpha", "α", "U+03B1", "greek"],
  ["beta", "beta", "\\beta", "β", "U+03B2", "greek"],
  ["gamma", "gamma", "\\gamma", "γ", "U+03B3", "greek"],
  ["delta", "delta", "\\delta", "δ", "U+03B4", "greek"],
  ["epsilon", "epsilon", "\\epsilon", "ϵ", "U+03F5", "greek"],
  ["lambda", "lambda", "\\lambda", "λ", "U+03BB", "greek"],
  ["mu", "mu", "\\mu", "μ", "U+03BC", "greek"],
  ["pi", "pi", "\\pi", "π", "U+03C0", "greek"],
  ["sigma", "sigma", "\\sigma", "σ", "U+03C3", "greek"],
  ["omega", "omega", "\\omega", "ω", "U+03C9", "greek"],
  ["Delta", "Delta", "\\Delta", "Δ", "U+0394", "greek"],
  ["Sigma", "Sigma", "\\Sigma", "Σ", "U+03A3", "greek"],
  ["infinity", "infinity", "\\infty", "∞", "U+221E", "symbols"],
  ["integral", "integral", "\\int", "∫", "U+222B", "operators"],
  ["sum", "sum", "\\sum", "∑", "U+2211", "operators"],
  ["partial", "partial", "\\partial", "∂", "U+2202", "operators"],
  ["nabla", "nabla", "\\nabla", "∇", "U+2207", "operators"],
  ["arrow.r", "arrow.r", "\\rightarrow", "→", "U+2192", "arrows"],
  ["arrow.l", "arrow.l", "\\leftarrow", "←", "U+2190", "arrows"],
  ["arrow.r.double", "arrow.r.double", "\\Rightarrow", "⇒", "U+21D2", "arrows"],
  ["gt.eq", "gt.eq", "\\geq", "≥", "U+2265", "relations"],
  ["lt.eq", "lt.eq", "\\leq", "≤", "U+2264", "relations"],
  ["approx", "approx", "\\approx", "≈", "U+2248", "relations"],
  ["times", "times", "\\times", "×", "U+00D7", "operators"],
  ["in", "in", "\\in", "∈", "U+2208", "relations"],
].map(([name, typst, latex, preview, unicode, category]) => ({
  name: name ?? "",
  typst: typst ?? null,
  latex: latex ?? null,
  preview: preview ?? "",
  unicode: unicode ?? null,
  category: category ?? "",
  requires: null,
  keywords: [name ?? ""],
}));

function searchSymbols(
  query: string,
  syntax: "typst" | "latex",
  limit: number,
  category: string | undefined,
) {
  const needle = query.replace(/^\\/, "").toLowerCase();
  const hits: { row: SymbolRow; score: number }[] = [];
  for (const row of SYMBOLS) {
    if (category !== undefined && row.category !== category) continue;
    const insert = syntax === "typst" ? row.typst : row.latex;
    if (insert === null) continue;
    const names = [row.name.toLowerCase(), insert.replace(/^\\/, "").toLowerCase()];
    let score = 0;
    if (needle === "") score = 1;
    else if (names.some((n) => n === needle)) score = 100;
    else if (names.some((n) => n.startsWith(needle))) score = 60;
    else if (names.some((n) => n.includes(needle))) score = 30;
    if (score > 0) hits.push({ row, score });
  }
  hits.sort((a, b) => b.score - a.score || a.row.name.localeCompare(b.row.name));
  return hits.slice(0, limit).map(({ row, score }) => ({
    name: row.name,
    insert: (syntax === "typst" ? row.typst : row.latex) ?? "",
    typst: row.typst,
    latex: row.latex,
    preview: row.preview,
    unicode: row.unicode,
    category: row.category,
    requires: null,
    keywords: row.keywords,
    score,
  }));
}

/* -------------------------------------------------------------------------- */
/* Snippets                                                                    */
/* -------------------------------------------------------------------------- */

interface SnippetRow {
  trigger: string;
  aliases: string[];
  syntax: "typst" | "latex" | "markdown";
  name: string;
  description: string;
  body: string;
  requires: string | null;
  source: "builtin";
}

const SNIPPETS: readonly SnippetRow[] = [
  {
    trigger: "theorem",
    aliases: ["thm"],
    syntax: "typst",
    name: "Theorem",
    description: "Theorem block with a name and a label",
    body: "#block(width: 100%, inset: (left: 1em), stroke: (left: 0.5pt))[\n  *Theorem (${1:name}).* ${2:Statement.}\n] <thm:${3:label}>\n$0",
    requires: null,
    source: "builtin",
  },
  {
    trigger: "theorem",
    aliases: ["thm"],
    syntax: "latex",
    name: "Theorem",
    description: "Theorem environment (needs amsthm)",
    body: "\\begin{theorem}[${1:name}]\\label{thm:${2:label}}\n  ${3:Statement.}\n\\end{theorem}\n$0",
    requires: "amsthm",
    source: "builtin",
  },
  {
    trigger: "theorem",
    aliases: ["thm"],
    syntax: "markdown",
    name: "Theorem",
    description: "Theorem as a block quote",
    body: "> **Theorem (${1:name}).** ${2:Statement.}\n\n$0",
    requires: null,
    source: "builtin",
  },
  {
    trigger: "equation",
    aliases: ["eq"],
    syntax: "typst",
    name: "Equation",
    description: "Numbered display equation",
    body: "$ ${1:x} = ${2:y} $ <eq:${3:label}>\n$0",
    requires: null,
    source: "builtin",
  },
  {
    trigger: "equation",
    aliases: ["eq"],
    syntax: "markdown",
    name: "Equation",
    description: "Display equation",
    body: "$$\n${1:x} = ${2:y}\n$$\n$0",
    requires: null,
    source: "builtin",
  },
];

interface Stop {
  index: number;
  placeholder: string;
  ranges: { start: number; end: number }[];
}

/** The template syntax of `snippets.toml`: `${1:text}`, `${2}`, `$3`, `$0`, `\$`. */
function expandBody(body: string, indent: string) {
  const pad = (s: string): string =>
    indent === "" ? s : s.replaceAll("\n", `\n${indent}`);
  let text = "";
  let at = 0;
  let finalCursor: number | null = null;
  const stops: Stop[] = [];
  const pattern = /\\\$|\$\{(\d+)(?::([^}]*))?\}|\$(\d+)/g;
  let last = 0;
  const push = (s: string): void => {
    text += s;
    at += scalarLength(s);
  };
  for (const match of body.matchAll(pattern)) {
    push(pad(body.slice(last, match.index)));
    last = match.index + match[0].length;
    if (match[0] === "\\$") {
      push("$");
      continue;
    }
    const index = Number(match[1] ?? match[3]);
    const existing = stops.find((s) => s.index === index);
    const given = match[2] ?? "";
    const shown =
      existing !== undefined && given === "" ? existing.placeholder : pad(given);
    if (index === 0) {
      finalCursor = at;
      continue;
    }
    const range = { start: at, end: at + scalarLength(shown) };
    push(shown);
    if (existing !== undefined) existing.ranges.push(range);
    else stops.push({ index, placeholder: shown, ranges: [range] });
  }
  push(pad(body.slice(last)));
  stops.sort((a, b) => a.index - b.index);
  return {
    text,
    tab_stops: stops,
    final_cursor: finalCursor ?? scalarLength(text),
  };
}

function snippetsFor(syntax: string | undefined): SnippetRow[] {
  return SNIPPETS.filter((s) => syntax === undefined || s.syntax === syntax);
}

/* -------------------------------------------------------------------------- */
/* Statistics                                                                  */
/* -------------------------------------------------------------------------- */

function countText(text: string, syntax: Syntax) {
  const masked = maskProse(text, syntax);
  const words = (masked.match(/[\p{L}\p{N}][\p{L}\p{N}'’-]*/gu) ?? []).length;
  const squashed = masked.replace(/[\s\u{1F4A0}]+/gu, " ").trim();
  const sentences = (squashed.match(/[.!?]+(\s|$)/g) ?? []).length;
  const paragraphs = masked
    .split(/\n\s*\n/)
    .filter((p) => /[\p{L}\p{N}]/u.test(p)).length;
  const reading = Math.round((words / 238) * 60);
  return {
    words,
    characters: scalarLength(squashed),
    characters_no_spaces: scalarLength(squashed.replace(/\s/g, "")),
    sentences,
    paragraphs,
    reading_time_seconds: reading,
    reading_time_minutes: Math.round((reading / 60) * 10) / 10,
    speaking_time_seconds: Math.round((words / 150) * 60),
  };
}

/* -------------------------------------------------------------------------- */

export function writingPlugin(): MockPluginFactory {
  return (): MockPlugin => {
    const user = new Set<string>();
    const language = "en_US";

    const editing = (args: Args, key: string): "typst" | "latex" | "markdown" => {
      const given = optStr(args, key);
      if (given === undefined) throw fail(`bad argument \`${key}\`: required string`);
      const syntax = syntaxOf({ syntax: given }, "");
      if (syntax === "plain") {
        throw fail(`bad argument \`${key}\`: expected typst, latex or markdown`);
      }
      return syntax;
    };

    return {
      name: "writing",
      commands: COMMANDS,
      handlers: {
        "writing.format.document": (args) => {
          const source = str(args, "source");
          const formatted = formatText(source, editing(args, "language"));
          return { formatted, changed: formatted !== source };
        },
        "writing.format.range": (args) => {
          const source = str(args, "source");
          const formatted = formatText(source, editing(args, "language"));
          return {
            formatted,
            changed: formatted !== source,
            range: { start: 0, end: scalarLength(source) },
            replacement: formatted,
          };
        },
        "writing.spell.check": (args) => {
          const text = str(args, "text");
          const found = misspellings(
            text,
            syntaxOf(args, text),
            user,
            Math.min(optNum(args, "max_suggestions") ?? 5, 20),
          );
          return { language, count: found.length, misspelled: found };
        },
        "writing.spell.suggest": (args) => {
          const word = str(args, "word");
          const limit = Math.min(optNum(args, "limit") ?? 5, 20);
          return { language, word, suggestions: suggestionsFor(word, limit) };
        },
        "writing.spell.add-word": (args) => {
          const word = str(args, "word").trim().toLowerCase();
          const added = word !== "" && !user.has(word);
          if (word !== "") user.add(word);
          return { added, count: user.size };
        },
        "writing.spell.remove-word": (args) => {
          const removed = user.delete(str(args, "word").trim().toLowerCase());
          return { removed, count: user.size };
        },
        "writing.spell.words": () => ({ words: [...user].sort() }),
        "writing.spell.languages": () => ({
          default: language,
          languages: [{ code: language, name: "English (US)", source: "bundled" }],
        }),
        "writing.lint.prose": (args) => {
          const text = optStr(args, "text") ?? str(args, "source");
          const syntax = syntaxOf(args, text);
          const file = optStr(args, "file") ?? null;
          const diagnostics = lintProse(text, syntax, file);
          if (args["spell"] === true) {
            for (const m of misspellings(text, syntax, user, 0)) {
              diagnostics.push({
                severity: "warning",
                message: `Possible misspelling: "${m.word}"`,
                file,
                line: m.line,
                column: m.column,
                range: m.range,
                hints: [],
                trace: [],
                code: "spelling",
              });
            }
          }
          return { source: "writing", diagnostics };
        },
        "writing.symbols.search": (args) => {
          const syntax = optStr(args, "syntax");
          if (syntax !== "typst" && syntax !== "latex") {
            throw fail("bad argument `syntax`: expected typst or latex");
          }
          return {
            symbols: searchSymbols(
              optStr(args, "query") ?? "",
              syntax,
              optNum(args, "limit") ?? 20,
              optStr(args, "category"),
            ),
          };
        },
        "writing.snippets.list": (args) => {
          const syntax = optStr(args, "syntax");
          if (syntax !== undefined) editing(args, "syntax");
          return { snippets: snippetsFor(syntax), errors: [] };
        },
        "writing.snippets.get": (args) => {
          const trigger = str(args, "trigger");
          const syntax = editing(args, "syntax");
          const found = snippetsFor(syntax).find(
            (s) => s.trigger === trigger || s.aliases.includes(trigger),
          );
          if (found === undefined)
            throw fail(`bad argument \`trigger\`: no snippet \`${trigger}\``);
          return found;
        },
        "writing.snippets.expand": (args) => {
          const trigger = str(args, "trigger");
          const syntax = editing(args, "syntax");
          const found = snippetsFor(syntax).find(
            (s) => s.trigger === trigger || s.aliases.includes(trigger),
          );
          if (found === undefined)
            throw fail(`bad argument \`trigger\`: no snippet \`${trigger}\``);
          return {
            ...expandBody(found.body, optStr(args, "indent") ?? ""),
            trigger: found.trigger,
            name: found.name,
            requires: found.requires,
          };
        },
        "writing.stats.count": (args) => {
          const text = str(args, "text");
          return countText(text, syntaxOf(args, text));
        },
      },
    };
  };
}
