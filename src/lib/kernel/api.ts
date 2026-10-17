/**
 * Typed wrappers over `kernel.run`, one per command the workbench uses.
 *
 * They add no behaviour: argument names and result shapes are exactly the
 * manifests' (`project.*`, `typst.*`). Keeping them in one place means a change
 * in a Rust plugin touches this file and `types.ts`, nothing else.
 */

import type { Kernel } from "./client";
import type {
  CompileReport,
  Engine,
  LatexReport,
  MarkdownRender,
  GitStatus,
  ProjectEntry,
  ProjectListQuery,
  ProjectPatch,
  ProjectSummary,
  RecentProject,
  TreePage,
  TreeQuery,
} from "./types";

export interface CompileRequest {
  /** Entry file, relative to `root`. */
  readonly main: string;
  readonly root?: string;
  /** Unsaved text keyed by project-relative path. */
  readonly sources?: Readonly<Record<string, string>>;
}

export function projectApi(kernel: Kernel) {
  return {
    open: (path: string) => kernel.run<ProjectSummary>("project.open", { path }),
    create: (path: string, name: string, engine: Engine) =>
      kernel.run<ProjectSummary>("project.create", { path, name, engine }),
    close: () => kernel.run<null>("project.close"),
    tree: (query: TreeQuery = {}) => kernel.run<TreePage>("project.tree", { ...query }),
    readFile: (path: string) => kernel.run<string>("project.read-file", { path }),
    writeFile: (path: string, content: string) =>
      kernel.run<{ bytes: number }>("project.write-file", { path, content }),
    rename: (from: string, to: string) =>
      kernel.run<null>("project.rename", { from, to }),
    trash: (path: string) =>
      kernel.run<{ trashed_to: string }>("project.delete", { path }),
    recent: () => kernel.run<RecentProject[]>("project.recent"),
    gitStatus: () => kernel.run<GitStatus>("project.git-status"),
    /** The registry: every known project with its metadata. */
    list: (query: ProjectListQuery = {}) =>
      kernel.run<ProjectEntry[]>("project.list", { ...query }),
    register: (path: string) => kernel.run<ProjectEntry>("project.register", { path }),
    update: (path: string, patch: ProjectPatch) =>
      kernel.run<ProjectEntry>("project.update", { path, ...patch }),
    /** Drops the project from the list; never touches its folder. */
    forget: (path: string) =>
      kernel.run<{ removed: boolean }>("project.forget", { path }),
    /** SVG bytes. Only call when the entry has `has_thumbnail`. */
    thumbnail: (path: string) => kernel.runBinary("project.thumbnail-get", { path }),
    setThumbnail: (path: string, svg: string) =>
      kernel.run<null>("project.thumbnail-set", { path, svg }),
  };
}

export function typstApi(kernel: Kernel) {
  return {
    compile: (request: CompileRequest) =>
      kernel.run<CompileReport>("typst.compile", { ...request }),
    /** One page as SVG bytes. */
    render: (page: number) => kernel.runBinary("typst.render", { page }),
    exportPdf: () => kernel.runBinary("typst.export-pdf"),
  };
}

export type ProjectApi = ReturnType<typeof projectApi>;
export type TypstApi = ReturnType<typeof typstApi>;

/* -------------------------------------------------------------------------- */
/* Writing tools (`writing.*`). Offsets are Unicode scalars, like the rest.   */
/* -------------------------------------------------------------------------- */

export type WritingSyntax = "typst" | "latex" | "markdown" | "plain" | "auto";

/** Half-open range of Unicode scalar values. */
export interface ScalarRange {
  readonly start: number;
  readonly end: number;
}

export interface Misspelling {
  readonly word: string;
  readonly range: ScalarRange;
  readonly line: number;
  readonly column: number;
  readonly suggestions: readonly string[];
}

export interface SpellCheckResult {
  readonly language: string;
  readonly count: number;
  readonly misspelled: readonly Misspelling[];
}

export interface WritingDiagnostic {
  readonly severity: "error" | "warning" | "info";
  readonly message: string;
  readonly file: string | null;
  readonly line: number | null;
  readonly column: number | null;
  readonly range: ScalarRange | null;
  readonly hints: readonly string[];
  readonly trace: readonly string[];
  readonly code?: string;
}

export interface ProseLintResult {
  readonly source: "writing";
  readonly diagnostics: readonly WritingDiagnostic[];
}

export interface FormatResult {
  readonly formatted: string;
  readonly changed: boolean;
  readonly skipped?: string;
}

export interface SymbolHit {
  readonly name: string;
  /** Exact text to insert for the requested syntax. */
  readonly insert: string;
  readonly typst: string | null;
  readonly latex: string | null;
  readonly preview: string;
  readonly unicode: string | null;
  readonly category: string;
  readonly requires: string | null;
  readonly keywords: readonly string[];
  readonly score: number;
}

export interface SnippetInfo {
  readonly trigger: string;
  readonly aliases: readonly string[];
  readonly syntax: WritingSyntax;
  readonly name: string;
  readonly description: string;
  readonly body: string;
  readonly requires: string | null;
}

export interface TabStop {
  readonly index: number;
  readonly placeholder: string;
  readonly ranges: readonly ScalarRange[];
}

export interface SnippetExpansion {
  readonly text: string;
  readonly tab_stops: readonly TabStop[];
  readonly final_cursor: number;
  readonly trigger: string;
  readonly name: string;
  readonly requires: string | null;
}

export interface TextStats {
  readonly words: number;
  readonly characters: number;
  readonly characters_no_spaces: number;
  readonly sentences: number;
  readonly paragraphs: number;
  readonly reading_time_seconds: number;
  readonly reading_time_minutes: number;
  readonly speaking_time_seconds: number;
}

export function writingApi(kernel: Kernel) {
  return {
    spellCheck: (text: string, syntax: WritingSyntax) =>
      kernel.run<SpellCheckResult>("writing.spell.check", {
        text,
        syntax,
        max_suggestions: 0,
      }),
    suggest: (word: string, limit = 5) =>
      kernel.run<{ language: string; word: string; suggestions: string[] }>(
        "writing.spell.suggest",
        { word, limit },
      ),
    addWord: (word: string) =>
      kernel.run<{ added: boolean; count: number }>("writing.spell.add-word", { word }),
    lint: (text: string, syntax: WritingSyntax) =>
      kernel.run<ProseLintResult>("writing.lint.prose", { text, syntax }),
    format: (source: string, language: "typst" | "latex" | "markdown") =>
      kernel.run<FormatResult>("writing.format.document", { source, language }),
    symbols: (query: string, syntax: "typst" | "latex", limit = 20) =>
      kernel.run<{ symbols: SymbolHit[] }>("writing.symbols.search", {
        query,
        syntax,
        limit,
      }),
    snippets: (syntax: "typst" | "latex" | "markdown") =>
      kernel.run<{ snippets: SnippetInfo[]; errors: unknown[] }>(
        "writing.snippets.list",
        { syntax },
      ),
    expand: (trigger: string, syntax: "typst" | "latex" | "markdown") =>
      kernel.run<SnippetExpansion>("writing.snippets.expand", { trigger, syntax }),
    stats: (text: string, syntax: WritingSyntax) =>
      kernel.run<TextStats>("writing.stats.count", { text, syntax }),
  };
}

export type WritingApi = ReturnType<typeof writingApi>;

export interface LatexCompileRequest {
  /** Absolute path of the main file; the plugin compiles what is on disk. */
  readonly main: string;
  /** Let the engine fetch missing TeX files once (needs the network). */
  readonly allow_network?: boolean;
}

export function latexApi(kernel: Kernel) {
  return {
    compile: (request: LatexCompileRequest) =>
      kernel.run<LatexReport>("latex.compile", { ...request }),
    /** The PDF of the latest build of `main`, as raw bytes. */
    exportPdf: (main: string) => kernel.runBinary("latex.export-pdf", { main }),
  };
}

export function markdownApi(kernel: Kernel) {
  return {
    renderHtml: (source: string) =>
      kernel.run<MarkdownRender>("markdown.render-html", { source }),
  };
}

export type LatexApi = ReturnType<typeof latexApi>;
export type MarkdownApi = ReturnType<typeof markdownApi>;
