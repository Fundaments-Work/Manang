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
  readonly bibliography?: { readonly paths: readonly string[] };
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

/** What `markdown.render-html` answers with. `html` is already sanitised in Rust. */
export interface MarkdownRender {
  readonly html: string;
  readonly frontmatter: unknown;
  readonly headings: readonly MarkdownHeading[];
  readonly diagnostics: readonly MarkdownDiagnostic[];
}
