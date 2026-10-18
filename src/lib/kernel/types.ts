/**
 * Wire shapes of the first-party plugins, copied from the Rust structs they are
 * serialised from. Keep in step with:
 *
 *  - `crates/manang-project` (`tree.rs`, `git.rs`, `config.rs`, `watch.rs`)
 *  - `crates/manang-typst`   (`compiler.rs` CompileReport, `diagnostics.rs`)
 */

export type Engine = "typst" | "latex" | "markdown";

export type EntryKind = "file" | "dir" | "symlink";

export interface TreeEntry {
  /** Project-relative, `/`-separated. */
  readonly path: string;
  readonly name: string;
  readonly kind: EntryKind;
}

export interface TreePage {
  readonly entries: readonly TreeEntry[];
  readonly total: number;
  readonly offset: number;
  readonly limit: number;
  readonly has_more: boolean;
  readonly truncated: boolean;
}

export interface TreeQuery {
  /** Directory to list; empty is the project root. */
  readonly path?: string;
  readonly recursive?: boolean;
  readonly offset?: number;
  readonly limit?: number;
}

export interface ProjectConfig {
  readonly project: {
    readonly engine: Engine;
    readonly name?: string;
    readonly main?: string;
  };
  readonly template?: { readonly id: string; readonly version?: string };
  readonly bibliography?: { readonly paths: readonly string[] };
}

export interface TemplateSummary {
  readonly id: string;
  readonly name: string;
  readonly engine: Engine;
  readonly description: string;
  readonly tags: readonly string[];
  readonly main: string;
}

/** What `project.open` and `project.create` answer with. */
export interface ProjectSummary {
  readonly root: string;
  readonly name: string;
  readonly engine: Engine;
  readonly main: string;
  readonly config: ProjectConfig;
  readonly warnings: readonly string[];
  readonly tree: TreePage;
}

export interface RecentProject {
  readonly path: string;
  readonly name: string;
  /** Seconds since the Unix epoch. */
  readonly opened_at: number;
  /** False when the folder or its `manang.toml` has gone. */
  readonly exists: boolean;
}

/** One project in the registry (`project.list`, `project.register`, `project.update`). */
export interface ProjectEntry {
  readonly path: string;
  readonly name: string;
  readonly engine: Engine;
  /** Seconds since the Unix epoch. */
  readonly created: number;
  readonly last_opened: number;
  readonly pinned: boolean;
  readonly archived: boolean;
  readonly tags: readonly string[];
  /** False when the folder or its `manang.toml` has gone. */
  readonly exists: boolean;
  readonly has_thumbnail: boolean;
  readonly trashed_at?: string | null;
}

export type ProjectSort = "modified" | "name" | "created";

export interface ProjectListQuery {
  readonly query?: string;
  readonly sort?: ProjectSort;
  readonly include_archived?: boolean;
  readonly include_trashed?: boolean;
}

export interface ProjectPatch {
  readonly name?: string;
  readonly pinned?: boolean;
  readonly archived?: boolean;
  readonly tags?: readonly string[];
}

export type ChangeState =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "copied"
  | "typechange"
  | "conflicted"
  | "intent-to-add"
  | "untracked";

export interface GitChange {
  readonly path: string;
  readonly staged: ChangeState | null;
  readonly unstaged: ChangeState | null;
}

export interface GitStatus {
  readonly is_repo: boolean;
  readonly branch: string | null;
  readonly detached: boolean;
  readonly head: string | null;
  readonly changes: readonly GitChange[];
  readonly truncated: boolean;
}

export interface ProjectChange {
  readonly path: string;
  readonly kind: "created" | "modified" | "removed" | "renamed";
  readonly from?: string;
}

/** Payload of the `project.changed` event. */
export interface ProjectChanged {
  readonly root: string;
  readonly changes: readonly ProjectChange[];
}

export interface WireDiagnostic {
  readonly severity: "error" | "warning";
  readonly message: string;
  /** Virtual path, e.g. `/main.typ`. */
  readonly file: string | null;
  /** 1-based. */
  readonly line: number | null;
  /** 1-based, counted in Unicode scalars. */
  readonly column: number | null;
  /** Scalar offsets into the file's text. */
  readonly range: { readonly start: number; readonly end: number } | null;
  readonly hints: readonly string[];
  readonly trace: readonly string[];
}

export interface CompileReport {
  readonly ok: boolean;
  readonly pages: number;
  /** `[width, height]` per page, in points. */
  readonly page_sizes: readonly (readonly [number, number])[];
  readonly diagnostics: readonly WireDiagnostic[];
  readonly duration_ms: number;
  readonly generation: number;
}

/** `crates/manang-latex` `Diagnostic`: file is relative to the main file's directory. */
export interface LatexDiagnostic {
  readonly severity: "error" | "warning" | "info";
  readonly message: string;
  readonly file: string | null;
  /** 1-based. */
  readonly line: number | null;
  readonly hint: string | null;
}

/** What `latex.compile` answers with. `pages` is null when the log did not say. */
export interface LatexReport {
  readonly ok: boolean;
  readonly pages: number | null;
  readonly diagnostics: readonly LatexDiagnostic[];
  readonly duration_ms: number;
  readonly synctex: boolean;
  /** Which TeX engine ran (`pdflatex`, `xelatex`, `lualatex`, `tectonic`). Absent on older plugins. */
  readonly engine?: string;
  /** `system` (TeX Live / MiKTeX on PATH) or `tectonic` (optional built-in). */
  readonly backend?: "system" | "tectonic";
  readonly tex_version?: string;
}

/** `crates/manang-markdown` `Diagnostic`; severity is a free string in Rust. */
export interface MarkdownDiagnostic {
  readonly severity: string;
  readonly message: string;
  /** 1-based; omitted when unknown. */
  readonly line?: number;
}

export interface MarkdownHeading {
  readonly title: string;
  readonly level: number;
  readonly id?: string;
  readonly line: number;
}

/**
 * A diagram fence of a Markdown document (`crates/manang-markdown`
 * `DiagramFence`). `line` is the opening fence's line: a diagnostic at line `n`
 * of `source` is at line `line + n` of the document.
 */
export interface MarkdownFence {
  readonly language: DiagramLanguageId;
  readonly source: string;
  readonly line: number;
  readonly endLine: number;
  /** Names the fence's placeholder in the HTML (`data-diagram`). */
  readonly key: string;
}

/**
 * What `markdown.render-html` answers with. `html` is already sanitised in
 * Rust; each diagram fence is an empty
 * `<figure class="mng-diagram" data-diagram="<key>">` the preview fills.
 */
export interface MarkdownRender {
  readonly html: string;
  readonly frontmatter: unknown;
  readonly headings: readonly MarkdownHeading[];
  readonly diagnostics: readonly MarkdownDiagnostic[];
  readonly diagrams: readonly MarkdownFence[];
}

/** What `project.export-diagram` / `project.cache-diagram` / `project.export-pdf` did. */
export interface DiagramExport {
  /** False when the source has an error; the previous files are untouched. */
  readonly ok: boolean;
  /** Project-relative path of the SVG, when there is one. */
  readonly svg: string | null;
  /** Project-relative path of the PDF, in projects that include PDFs (LaTeX). */
  readonly pdf: string | null;
  /** The files this call wrote; empty when they were current. */
  readonly written: readonly string[];
  /** The diagram plugin's message (`[diagram_error] line L:C: ...`) when not ok. */
  readonly error: string | null;
}

/** `project.stat`: an entry's size, date and kind, without reading it. */
export interface FileStat {
  readonly size: number;
  /** Milliseconds since the Unix epoch, or null when unknown. */
  readonly modified: number | null;
  readonly kind: "file" | "dir" | "other";
}

/* -------------------------------------------------------------------------- */
/* Diagrams (`crates/manang-diagram`)                                          */
/* -------------------------------------------------------------------------- */

export type DiagramLanguageId = "mermaid" | "pikchr" | "svgbob";

/** Colours the plugin renders with: the app's two themes, or ink on white. */
export type DiagramTheme = "dark" | "light" | "print";

/** One entry of `diagram.languages`. */
export interface DiagramLanguage {
  readonly id: DiagramLanguageId;
  readonly name: string;
  /** With the leading dot, e.g. `.mmd`. */
  readonly extensions: readonly string[];
  /** Code-fence info strings, e.g. `mermaid`. */
  readonly fences: readonly string[];
  /** The renderer and its version, e.g. `merman 0.9.0`. */
  readonly engine: string;
}

/**
 * A `diagram.check` diagnostic. Lines and columns are 1-based, columns counted
 * in Unicode scalars, as in Typst diagnostics.
 */
export interface DiagramDiagnostic {
  readonly severity: "error" | "warning";
  readonly message: string;
  readonly line: number;
  readonly column: number;
  readonly endLine: number;
  readonly endColumn: number;
  readonly code?: string;
}
