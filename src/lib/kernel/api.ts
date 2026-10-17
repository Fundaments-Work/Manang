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
  GitStatus,
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
      kernel.run<{ cancelled: number }>("search.cancel", id !== undefined ? { id } : {}),
    replacePreview: (req: SearchPreviewRequest) =>
      kernel.run<SearchPreviewResponse>("search.replace-preview", { ...req }),
    replaceApply: (req: SearchApplyRequest) =>
      kernel.run<SearchApplyResponse>("search.replace-apply", { ...req }),
    inText: (req: SearchInTextRequest) =>
      kernel.run<SearchTextResponse>("search.in-text", { ...req }),
  };
}

export type ProjectApi = ReturnType<typeof projectApi>;
export type TypstApi = ReturnType<typeof typstApi>;
export type SearchApi = ReturnType<typeof searchApi>;

