/**
 * Typed wrappers over `kernel.run`, one per command the workbench uses.
 *
 * They add no behaviour: argument names and result shapes are exactly the
 * manifests' (`project.*`, `typst.*`). Keeping them in one place means a change
 * in a Rust plugin touches this file and `types.ts`, nothing else.
 */

import type { FigureEdit } from "$lib/figure/edit";
import type { FigureNode, FigureScene } from "$lib/figure/scene";

import type { Kernel } from "./client";
import type {
  ChangeState,
  MarkdownDiagnostic,
  CompileReport,
  DiagramDiagnostic,
  DiagramExport,
  DiagramLanguage,
  DiagramLanguageId,
  DiagramTheme,
  Engine,
  FileStat,
  LatexReport,
  MarkdownFence,
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
    /** Any project file as raw bytes (images, PDFs); never base64. */
    readBytes: (path: string) => kernel.runBinary("project.read-bytes", { path }),
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
    /** Size, date and kind of a project entry, without reading it. */
    stat: (path: string) => kernel.run<FileStat>("project.stat", { path }),
    /**
     * Write the diagram file's print SVG next to it (and its PDF in LaTeX
     * projects). Skips files that are current; a source with errors answers
     * `ok: false` and leaves the old files.
     */
    exportDiagram: (path: string) =>
      kernel.run<DiagramExport>("project.export-diagram", { path }),
    /** An inline diagram's print SVG under `.manang/cache/diagrams/`. */
    cacheDiagram: (language: DiagramLanguageId, source: string) =>
      kernel.run<DiagramExport>("project.cache-diagram", { language, source }),
    /** The PDF next to a figure's SVG, in projects that include PDFs. */
    exportPdf: (path: string) =>
      kernel.run<DiagramExport>("project.export-pdf", { path }),
  };
}

/** A point on a rendered page: 0-based page, points from the top-left corner. */
export interface PagePoint {
  readonly page: number;
  readonly x: number;
  readonly y: number;
}

/**
 * Where a click on a Typst page leads (`crates/manang-typst` `SourceJump`).
 * `path` is project-relative, or `@ns/name:version/path` inside a package;
 * line and column are 1-based, the column counted in characters.
 */
export type TypstJump =
  | {
      readonly kind: "file";
      readonly path: string;
      readonly line: number;
      readonly column: number;
    }
  | { readonly kind: "url"; readonly url: string }
  | ({ readonly kind: "position" } & PagePoint);

export function typstApi(kernel: Kernel) {
  return {
    compile: (request: CompileRequest) =>
      kernel.run<CompileReport>("typst.compile", { ...request }),
    /** One page as SVG bytes. */
    render: (page: number) => kernel.runBinary("typst.render", { page }),
    exportPdf: () => kernel.runBinary("typst.export-pdf"),
    /** What produced the point `(x, y)` (pt) on 0-based `page`; null for blank space. */
    jumpFromClick: (page: number, x: number, y: number) =>
      kernel.run<TypstJump | null>("typst.jump-from-click", { page, x, y }),
    /** Where the text at a 1-based line and column landed on the pages. */
    jumpFromCursor: (path: string, line: number, column: number) =>
      kernel.run<PagePoint[]>("typst.jump-from-cursor", { path, line, column }),
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

/** One spelling dictionary the writing plugin can check with. */
export interface SpellLanguage {
  /** Normalised code such as `en_US`. */
  readonly code: string;
  /** Shipped with the app, or a Hunspell pair in the config folder. */
  readonly source: "bundled" | "user";
}

export interface SpellLanguages {
  /** The configured language (`writing.spell-language`). */
  readonly default: string;
  readonly spellcheck?: boolean;
  readonly languages: readonly SpellLanguage[];
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
    removeWord: (word: string) =>
      kernel.run<{ removed: boolean; count: number }>("writing.spell.remove-word", {
        word,
      }),
    /** The personal dictionary, sorted. */
    words: () => kernel.run<{ words: string[] }>("writing.spell.words"),
    languages: () => kernel.run<SpellLanguages>("writing.spell.languages"),
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

/** One backend in `latex.status`. */
export interface LatexBackendStatus {
  readonly id: "system" | "tectonic";
  readonly available: boolean;
  readonly version: string | null;
  readonly engines: readonly string[];
}

/** What `latex.status` answers with; `default` is `none` when nothing can compile. */
export interface LatexStatus {
  readonly backends: readonly LatexBackendStatus[];
  readonly default: "system" | "tectonic" | "none";
  readonly default_engine: string;
}

/** A box in the PDF from SyncTeX: 1-based page, points from the top-left corner. */
export interface SynctexBox {
  readonly page: number;
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export function latexApi(kernel: Kernel) {
  return {
    compile: (request: LatexCompileRequest) =>
      kernel.run<LatexReport>("latex.compile", { ...request }),
    /** The PDF of the latest build of `main`, as raw bytes. */
    exportPdf: (main: string) => kernel.runBinary("latex.export-pdf", { main }),
    /** Which TeX backends this machine has. */
    status: () => kernel.run<LatexStatus>("latex.status"),
    /**
     * Source line behind a point on 1-based `page` of the latest build of `main`
     * (absolute). `file` is relative to the main file's directory, unless it lies
     * outside it.
     */
    synctexReverse: (main: string, page: number, x: number, y: number) =>
      kernel.run<{ readonly file: string; readonly line: number }>("latex.synctex", {
        main,
        page,
        x,
        y,
      }),
    /** Where a 1-based line of `file` (relative to the main file's directory) landed. */
    synctexForward: (main: string, file: string, line: number) =>
      kernel.run<{ readonly positions: readonly SynctexBox[] }>("latex.synctex", {
        main,
        file,
        line,
      }),
  };
}

export function markdownApi(kernel: Kernel) {
  return {
    renderHtml: (source: string) =>
      kernel.run<MarkdownRender>("markdown.render-html", { source }),
    outline: (source: string) =>
      kernel.run<{ headings: readonly MarkdownHeading[] }>("markdown.outline", {
        source,
      }),
    /** Typst markup for a Markdown source; frontmatter is dropped. */
    toTypst: (source: string) =>
      kernel.run<MarkdownToTypst>("markdown.to-typst", { source }),
    /** The diagram fences of a document, in order. */
    diagrams: (source: string) =>
      kernel.run<{ diagrams: readonly MarkdownFence[] }>("markdown.diagrams", {
        source,
      }),
  };
}

/** `diagram.*`: Mermaid, Pikchr and ASCII (svgbob) diagrams rendered to SVG. */
export function diagramApi(kernel: Kernel) {
  return {
    languages: () =>
      kernel.run<{ languages: readonly DiagramLanguage[] }>("diagram.languages", {}),
    check: (language: DiagramLanguageId, source: string) =>
      kernel.run<{ diagnostics: readonly DiagramDiagnostic[] }>("diagram.check", {
        language,
        source,
      }),
    /**
     * SVG bytes. A source that does not parse rejects with a message starting
     * `[diagram_error] line L:C: ...`.
     */
    render: (language: DiagramLanguageId, source: string, theme: DiagramTheme) =>
      kernel.runBinary("diagram.render", { language, source, theme }),
    /** A cache key for the rendered SVG of these inputs. */
    key: (language: DiagramLanguageId, source: string, theme: DiagramTheme) =>
      kernel.run<{ key: string }>("diagram.key", { language, source, theme }),
  };
}

export type DiagramApi = ReturnType<typeof diagramApi>;

export interface MarkdownToTypst {
  readonly typst: string;
  /** Frontmatter that was dropped, constructs Typst has no equivalent for. */
  readonly diagnostics: readonly MarkdownDiagnostic[];
}

export type LatexApi = ReturnType<typeof latexApi>;
export type MarkdownApi = ReturnType<typeof markdownApi>;

export interface SearchFilesRequest {
  readonly root: string;
  readonly query: string;
  readonly limit?: number | undefined;
  readonly hidden?: boolean | undefined;
}

export interface SearchFileResult {
  readonly path: string;
  readonly name_start: number;
  readonly score: number;
  readonly indices: readonly number[];
}

export interface SearchFilesResponse {
  readonly results: readonly SearchFileResult[];
  readonly total_indexed: number;
  readonly index_truncated: boolean;
  readonly elapsed_ms: number;
}

export interface SearchTextRequest {
  readonly root: string;
  readonly query: string;
  readonly regex?: boolean | undefined;
  readonly case_sensitive?: boolean | undefined;
  readonly whole_word?: boolean | undefined;
  readonly include?: readonly string[] | undefined;
  readonly exclude?: readonly string[] | undefined;
  readonly limit?: number | undefined;
  readonly context_lines?: number | undefined;
  readonly hidden?: boolean | undefined;
}

export interface SearchRange {
  readonly start: number;
  readonly end: number;
}

export interface SearchContextLine {
  readonly line: number;
  readonly text: string;
}

export interface SearchLineMatch {
  readonly line: number;
  readonly column: number;
  readonly byte_offset: number;
  readonly text: string;
  readonly text_offset?: number | undefined;
  readonly ranges: readonly SearchRange[];
  readonly before: readonly SearchContextLine[];
  readonly after: readonly SearchContextLine[];
}

export interface SearchFileMatches {
  readonly path: string;
  readonly matches: readonly SearchLineMatch[];
  readonly truncated: boolean;
}

export interface SearchStats {
  readonly bytes_scanned: number;
  readonly files_searched: number;
  readonly files_matched: number;
  readonly files_binary: number;
  readonly files_unreadable: number;
  readonly files_too_large: number;
  readonly duration_ms: number;
}

export interface SearchTextResponse {
  readonly id: number;
  readonly files: readonly SearchFileMatches[];
  readonly total_matches: number;
  readonly total_ranges: number;
  readonly truncated: boolean;
  readonly cancelled: boolean;
  readonly stopped: string | null;
  readonly stats: SearchStats;
}

export interface SearchPreviewRequest {
  readonly root: string;
  readonly query: string;
  readonly replacement: string;
  readonly regex?: boolean | undefined;
  readonly case_sensitive?: boolean | undefined;
  readonly whole_word?: boolean | undefined;
  readonly include?: readonly string[] | undefined;
  readonly exclude?: readonly string[] | undefined;
  readonly limit?: number | undefined;
  readonly hidden?: boolean | undefined;
}

export interface SearchPreviewEdit {
  readonly line: number;
  readonly column: number;
  readonly start: number;
  readonly end: number;
  readonly offset_start: number;
  readonly offset_end: number;
  readonly matched: string;
  readonly replacement: string;
  readonly line_text: string;
}

export interface SearchPreviewFile {
  readonly path: string;
  readonly hash: string;
  readonly edits: readonly SearchPreviewEdit[];
}

export interface SearchPreviewResponse {
  readonly id: number;
  readonly plan_id: string;
  readonly files: readonly SearchPreviewFile[];
  readonly total_edits: number;
  readonly truncated: boolean;
  readonly cancelled: boolean;
  readonly stopped: string | null;
  readonly stats: SearchStats;
}

export interface SearchApplyRequest {
  readonly plan_id: string;
  readonly files?: readonly string[] | undefined;
}

export interface SearchApplyFileResult {
  readonly path: string;
  readonly ok: boolean;
  readonly replacements: number;
  readonly error?: string | undefined;
}

export interface SearchApplyResponse {
  readonly applied: number;
  readonly failed: number;
  readonly files: readonly SearchApplyFileResult[];
}

export interface SearchInTextRequest {
  readonly text: string;
  readonly query: string;
  readonly regex?: boolean | undefined;
  readonly case_sensitive?: boolean | undefined;
  readonly whole_word?: boolean | undefined;
  readonly limit?: number | undefined;
}

export function searchApi(kernel: Kernel) {
  return {
    files: (req: SearchFilesRequest) =>
      kernel.run<SearchFilesResponse>("search.files", { ...req }),
    text: (req: SearchTextRequest) =>
      kernel.run<SearchTextResponse>("search.text", { ...req }),
    cancel: (id?: number) =>
      kernel.run<{ cancelled: number }>(
        "search.cancel",
        id !== undefined ? { id } : {},
      ),
    replacePreview: (req: SearchPreviewRequest) =>
      kernel.run<SearchPreviewResponse>("search.replace-preview", { ...req }),
    replaceApply: (req: SearchApplyRequest) =>
      kernel.run<SearchApplyResponse>("search.replace-apply", { ...req }),
    inText: (req: SearchInTextRequest) =>
      kernel.run<SearchTextResponse>("search.in-text", { ...req }),
  };
}

export type SearchApi = ReturnType<typeof searchApi>;

/* -------------------------------------------------------------------------- */
/* Git API                                                                     */
/* -------------------------------------------------------------------------- */

export interface GitStatusFile {
  readonly path: string;
  readonly staged: ChangeState | null;
  readonly unstaged: ChangeState | null;
  readonly untracked: boolean;
  readonly conflicted: boolean;
}

export interface GitUpstream {
  readonly name: string;
  readonly ahead: number;
  readonly behind: number;
}

export interface GitStatusReport {
  readonly branch: string | null;
  readonly detached: boolean;
  readonly unborn: boolean;
  readonly head: string | null;
  readonly upstream: GitUpstream | null;
  readonly files: readonly GitStatusFile[];
  readonly truncated: boolean;
}

export interface GitDiffLine {
  readonly kind: "context" | "add" | "remove";
  readonly text: string;
  readonly old_line: number | null;
  readonly new_line: number | null;
  readonly old_offset: number | null;
  readonly new_offset: number | null;
  readonly no_newline: boolean;
}

export interface GitDiffHunk {
  readonly old_start: number;
  readonly old_lines: number;
  readonly new_start: number;
  readonly new_lines: number;
  readonly header: string;
  readonly lines: readonly GitDiffLine[];
}

export interface GitFileDiff {
  readonly path: string;
  readonly status: "added" | "deleted" | "modified";
  readonly binary: boolean;
  readonly too_large: boolean;
  readonly additions: number;
  readonly deletions: number;
  readonly hunks: readonly GitDiffHunk[];
}

export interface GitDiffResponse {
  readonly staged: boolean;
  readonly files: readonly GitFileDiff[];
}

export interface GitCommitInfo {
  readonly hash: string;
  readonly short: string;
  readonly author_name: string;
  readonly author_email: string;
  readonly time: number;
  readonly subject: string;
  readonly parents: readonly string[];
}

export interface GitLogResponse {
  readonly commits: readonly GitCommitInfo[];
  readonly has_more: boolean;
}

export interface GitBranchInfo {
  readonly name: string;
  readonly remote: boolean;
  readonly current: boolean;
  readonly hash: string;
  readonly upstream: string | null;
}

export interface GitBlameRange {
  readonly start: number;
  readonly lines: number;
  readonly commit: string;
  readonly source_path: string | null;
}

export interface GitBlameResponse {
  readonly path: string;
  readonly total_lines: number;
  readonly ranges: readonly GitBlameRange[];
}

export function gitApi(kernel: Kernel) {
  return {
    open: (root: string) =>
      kernel.run<{ root: string; is_repo: boolean }>("git.open", { root }),
    status: (req: { root?: string | undefined } = {}) =>
      kernel.run<GitStatusReport>("git.status", { ...req }),
    diff: (
      req: {
        root?: string | undefined;
        path?: string | undefined;
        staged?: boolean | undefined;
      } = {},
    ) => kernel.run<GitDiffResponse>("git.diff", { ...req }),
    diffText: (req: {
      path: string;
      staged?: boolean | undefined;
      root?: string | undefined;
    }) => kernel.run<{ text: string }>("git.diff-text", { ...req }),
    log: (
      req: {
        root?: string | undefined;
        path?: string | undefined;
        limit?: number | undefined;
        offset?: number | undefined;
      } = {},
    ) => kernel.run<GitLogResponse>("git.log", { ...req }),
    blame: (req: { path: string; root?: string | undefined }) =>
      kernel.run<GitBlameResponse>("git.blame", { ...req }),
    branches: (req: { root?: string | undefined } = {}) =>
      kernel.run<readonly GitBranchInfo[]>("git.branches", { ...req }),
    show: (req: { rev: string; path: string; root?: string | undefined }) =>
      kernel.run<{ rev: string; path: string; text: string }>("git.show", { ...req }),
    stage: (req: { paths: readonly string[]; root?: string | undefined }) =>
      kernel.run<{ staged: number }>("git.stage", { ...req }),
    unstage: (req: { paths: readonly string[]; root?: string | undefined }) =>
      kernel.run<{ unstaged: number }>("git.unstage", { ...req }),
    discard: (req: {
      paths: readonly string[];
      confirm: boolean;
      root?: string | undefined;
    }) =>
      kernel.run<{ discarded: readonly string[]; trash: string }>("git.discard", {
        ...req,
      }),
    commit: (req: {
      message: string;
      amend?: boolean | undefined;
      hooks?: boolean | undefined;
      root?: string | undefined;
    }) =>
      kernel.run<{ hash: string; short: string; subject: string; amended: boolean }>(
        "git.commit",
        { ...req },
      ),
    checkoutBranch: (req: {
      name: string;
      create?: boolean | undefined;
      root?: string | undefined;
    }) =>
      kernel.run<{ branch: string; created: boolean }>("git.checkout-branch", {
        ...req,
      }),
    init: (req: { root?: string | undefined } = {}) =>
      kernel.run<GitStatusReport>("git.init", { ...req }),
    sync: (req: {
      action: "fetch" | "pull" | "push";
      remote?: string | undefined;
      timeout_secs?: number | undefined;
      hooks?: boolean | undefined;
      root?: string | undefined;
    }) =>
      kernel.run<{ action: string; remote: string; output: string }>("git.sync", {
        ...req,
      }),
  };
}

export type GitApi = ReturnType<typeof gitApi>;

// ---------------------------------------------------------------------------
// Settings, Keybindings & Theme
// ---------------------------------------------------------------------------

export type SettingScope = "user" | "project";

export interface SettingDef {
  readonly id: string;
  /** Short label from the declaring manifest; derived from the id when absent. */
  readonly title?: string | undefined;
  readonly type: "bool" | "int" | "float" | "enum" | "string" | "path" | "list";
  readonly min?: number | undefined;
  readonly max?: number | undefined;
  readonly values?: readonly string[] | undefined;
  readonly item?: unknown | undefined;
  readonly default: unknown;
  readonly description: string;
  readonly scope: SettingScope;
  readonly "requires-restart": boolean;
  readonly owner: string;
  readonly category: string;
}

export interface SettingsSchema {
  readonly version: number;
  readonly categories: readonly string[];
  readonly settings: readonly SettingDef[];
}

export interface SettingDescription {
  readonly id: string;
  readonly value: unknown;
  readonly source: "default" | "user" | "project";
  readonly default: unknown;
  readonly "requires-restart"?: boolean | undefined;
  readonly written?: string | undefined;
}

export interface SettingsGetAllResponse {
  readonly values: Record<
    string,
    { readonly value: unknown; readonly source: "default" | "user" | "project" }
  >;
  readonly warnings: readonly {
    readonly layer: string;
    readonly id: string | null;
    readonly message: string;
  }[];
}

export interface KeybindingItem {
  readonly command: string;
  readonly key: string;
  readonly display: string;
  readonly when: string;
  readonly source: "default" | "user";
}

export interface KeybindingConflict {
  readonly key: string;
  readonly kind: "conflict" | "shadow";
  readonly commands: readonly string[];
}

export interface KeybindingsListResponse {
  readonly platform: string;
  readonly bindings: readonly KeybindingItem[];
  readonly conflicts: readonly KeybindingConflict[];
  readonly warnings?: readonly string[] | undefined;
}

export interface ThemeSummary {
  readonly id: string;
  readonly name: string;
  readonly scheme: "dark" | "light";
  readonly builtin: boolean;
  readonly base: string | null;
}

export interface ThemeDetail {
  readonly id: string;
  readonly name: string;
  readonly scheme: "dark" | "light";
  readonly builtin: boolean;
  readonly base: string | null;
  readonly tokens: Record<string, string>;
  readonly overrides: Record<string, string>;
}

export interface ThemeListResponse {
  readonly themes: readonly ThemeSummary[];
  readonly warnings: readonly string[];
}

export function settingsApi(kernel: Kernel) {
  function get(id: string): Promise<SettingDescription>;
  function get(): Promise<SettingsGetAllResponse>;
  function get(
    id?: string | undefined,
  ): Promise<SettingDescription | SettingsGetAllResponse> {
    return id !== undefined
      ? kernel.run<SettingDescription>("settings.get", { id })
      : kernel.run<SettingsGetAllResponse>("settings.get", {});
  }

  return {
    get,
    set: (req: { id: string; value: unknown; scope?: SettingScope | undefined }) =>
      kernel.run<SettingDescription>("settings.set", { ...req }),
    reset: (req: { id: string; scope?: SettingScope | undefined }) =>
      kernel.run<SettingDescription>("settings.reset", { ...req }),
    schema: () => kernel.run<SettingsSchema>("settings.schema", {}),
    register: (req: { settings: readonly SettingDef[]; owner?: string | undefined }) =>
      kernel.run<{ registered: number; warnings: readonly unknown[] }>(
        "settings.register",
        { ...req },
      ),
    openFile: (scope: "user" | "project" | "keybindings") =>
      kernel.run<{ path: string; created: boolean }>("settings.open-file", { scope }),
    useProject: (path: string | null) =>
      kernel.run<{ warnings: readonly unknown[] }>("settings.use-project", { path }),
  };
}

export type SettingsApi = ReturnType<typeof settingsApi>;

export function keybindingsApi(kernel: Kernel) {
  return {
    list: () => kernel.run<KeybindingsListResponse>("keybindings.list", {}),
    resolve: (req: { key: string; context?: string | readonly string[] | undefined }) =>
      kernel.run<unknown>("keybindings.resolve", { ...req }),
    set: (req: {
      command: string;
      key?: string | undefined;
      keys?: readonly string[] | undefined;
      when?: string | undefined;
    }) =>
      kernel.run<{
        command: string;
        keys: readonly string[];
        when: string;
        conflicts: readonly KeybindingConflict[];
      }>("keybindings.set", { ...req }),
    reset: (command?: string | undefined) =>
      kernel.run<{ command: string | null }>(
        "keybindings.reset",
        command ? { command } : {},
      ),
  };
}

export type KeybindingsApi = ReturnType<typeof keybindingsApi>;

export function themeApi(kernel: Kernel) {
  return {
    list: () => kernel.run<ThemeListResponse>("theme.list", {}),
    get: (id: string) => kernel.run<ThemeDetail>("theme.get", { id }),
  };
}

export type ThemeApi = ReturnType<typeof themeApi>;

export interface CitationAuthor {
  readonly name: string;
  readonly family?: string | undefined;
  readonly given?: string | undefined;
}

export interface CitationEntry {
  readonly key: string;
  readonly entry_type: string;
  readonly title?: string | undefined;
  readonly authors?: readonly CitationAuthor[] | undefined;
  readonly year?: number | undefined;
  readonly month?: number | undefined;
  readonly journal_or_publisher?: string | undefined;
  readonly volume?: string | undefined;
  readonly number?: string | undefined;
  readonly pages?: string | undefined;
  readonly doi?: string | undefined;
  readonly url?: string | undefined;
  readonly arxiv_id?: string | undefined;
  readonly abstract_text?: string | undefined;
  readonly raw_fields?: Readonly<Record<string, string>> | undefined;
}

export interface CitationDifference {
  readonly field: string;
  readonly local: string;
  readonly remote: string;
}

export type CitationVerificationStatus =
  "verified" | "mismatch" | "not_found" | "error";

export interface CitationVerificationResult {
  readonly status: CitationVerificationStatus;
  readonly matched?: CitationEntry | null | undefined;
  readonly differences: readonly CitationDifference[];
  readonly message?: string | null | undefined;
}

export interface CitationParseDiagnostic {
  readonly file?: string | null | undefined;
  readonly line: number;
  readonly column: number;
  readonly message: string;
}

export interface CitationLoadResponse {
  readonly loaded: number;
  readonly total: number;
  readonly errors: readonly CitationParseDiagnostic[];
}

export interface CitationSearchResultItem {
  readonly key: string;
  readonly score: number;
  readonly entry: CitationEntry;
}

export interface CitationSearchResponse {
  readonly results: readonly CitationSearchResultItem[];
}

export interface CitationFormatResponse {
  readonly key: string;
  readonly style: string;
  readonly formatted: string;
}

export interface CitationExtractKeysResponse {
  readonly keys: readonly string[];
}

/** Styles `citations.format` accepts (`ieee` is an alias of `numeric`). */
export type CitationStyle = "apa" | "numeric" | "ieee";

export function citationsApi(kernel: Kernel) {
  return {
    load: (req: { paths: readonly string[] } | { path: string }) =>
      kernel.run<CitationLoadResponse>("citations.load", { ...req }),
    search: (req: { query: string; limit?: number | undefined }) =>
      kernel.run<CitationSearchResponse>("citations.search", { ...req }),
    get: (key: string) => kernel.run<CitationEntry>("citations.get", { key }),
    /** Without `style`, the plugin uses the configured `citations.style`. */
    format: (key: string, style?: CitationStyle) =>
      kernel.run<CitationFormatResponse>(
        "citations.format",
        style === undefined ? { key } : { key, style },
      ),
    extractKeys: (source: string, syntax: "typst" | "latex" | "markdown") =>
      kernel.run<CitationExtractKeysResponse>("citations.extract-keys", {
        source,
        syntax,
      }),
    verify: (key: string) =>
      kernel.run<CitationVerificationResult>("citations.verify", { key }),
  };
}

export type CitationsApi = ReturnType<typeof citationsApi>;

export interface MarkdownHeading {
  readonly title: string;
  readonly level: number;
  readonly id?: string | undefined;
  readonly line: number;
  readonly offset: number;
}

/* -------------------------------------------------------------------------- */
/* Figures (`figure.*`). Paths are absolute: the plugin is not project-bound. */
/* -------------------------------------------------------------------------- */

/** One operation, exactly as `figure.apply` takes it (`op` tag, kebab-case). */
export type FigureOp = FigureEdit | { readonly op: "undo" } | { readonly op: "redo" };

export interface FigureApplyResult {
  readonly revision: number;
  /** False when the op changed nothing (undo with no history, a zero move). */
  readonly applied: boolean;
  readonly can_undo: boolean;
  readonly can_redo: boolean;
}

export interface FigureSaved {
  /** Absolute paths of the written scene and its SVG. */
  readonly json: string;
  readonly svg: string;
  readonly revision: number;
  /** Project-relative path of the PDF written for LaTeX projects, else null. */
  readonly pdf?: string | null;
}

export interface FigureInfo {
  readonly id: number;
  readonly path: string | null;
  readonly nodes: number;
  readonly revision: number;
  readonly dirty: boolean;
}

export interface FigureHit {
  readonly hit: boolean;
  readonly node_id: string | null;
  readonly node: FigureNode | null;
}

export function figureApi(kernel: Kernel) {
  return {
    /** A new figure written to `path` straight away (scene and SVG). */
    createAt: (path: string, width?: number, height?: number) =>
      kernel.run<{ id: number } & Omit<FigureSaved, "revision">>("figure.create", {
        path,
        ...(width === undefined ? {} : { width }),
        ...(height === undefined ? {} : { height }),
      }),
    open: (path: string) => kernel.run<{ id: number }>("figure.open", { path }),
    get: (id: number) => kernel.run<FigureScene>("figure.get", { id }),
    apply: (id: number, op: FigureOp) =>
      kernel.run<FigureApplyResult>("figure.apply", { id, op }),
    /** Writes `<name>.figure.json` and `<name>.svg`; `path` follows a rename. */
    save: (id: number, path?: string) =>
      kernel.run<FigureSaved>("figure.save", {
        id,
        ...(path === undefined ? {} : { path }),
      }),
    close: (id: number) => kernel.run<{ closed: boolean }>("figure.close", { id }),
    list: () => kernel.run<FigureInfo[]>("figure.list"),
    hitTest: (id: number, x: number, y: number) =>
      kernel.run<FigureHit>("figure.hit-test", { id, x, y }),
    /** The open figure as SVG bytes. */
    exportSvg: (id: number) => kernel.runBinary("figure.export-svg", { id }),
  };
}

export type FigureApi = ReturnType<typeof figureApi>;
