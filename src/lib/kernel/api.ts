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
    rename: (from: string, to: string) => kernel.run<null>("project.rename", { from, to }),
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

export type ProjectApi = ReturnType<typeof projectApi>;
export type TypstApi = ReturnType<typeof typstApi>;
