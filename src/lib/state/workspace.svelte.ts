/**
 * The open project: its file tree, git status, references and recent list.
 *
 * Everything comes from the kernel's `project.*` commands. The tree is loaded
 * lazily, one directory per expansion, and refreshed when the kernel publishes
 * `project.changed`. Actions that must also touch tabs or documents live in
 * `actions.ts`; this store only knows about the project itself.
 */

import { SvelteMap, SvelteSet } from "svelte/reactivity";

import { kernel, errorMessage } from "$lib/kernel";
import { projectApi } from "$lib/kernel/api";
import type { TextRead, WriteResult } from "$lib/kernel/api";
import type {
  Engine,
  GitStatus,
  ProjectConfig,
  ProjectSummary,
  RecentProject,
  TreeEntry,
} from "$lib/kernel/types";
import { dirname, engineOf, fileKind, join, moved } from "$lib/workbench/paths";
import { parseBibtex, type BibEntry } from "$lib/workbench/references";
import {
  KEEP_FILE,
  ancestors,
  branchLabel,
  filterRows,
  gitMarks,
  visibleRows,
} from "$lib/workbench/tree";

export interface ProjectInfo {
  readonly root: string;
  readonly name: string;
  readonly engine: Engine;
  readonly main: string;
  readonly config: ProjectConfig;
}

const api = projectApi(kernel);
/** The largest page the kernel returns for one listing. */
const PAGE = 5000;

export class WorkspaceState {
  project = $state.raw<ProjectInfo | null>(null);
  /** Directory path to its entries, filled in as folders are opened. */
  readonly listings = new SvelteMap<string, readonly TreeEntry[]>();
  readonly expanded = new SvelteSet<string>();
  git = $state.raw<GitStatus | null>(null);
  recent = $state.raw<readonly RecentProject[]>([]);
  /** Every entry in the project; loaded when something needs the whole list. */
  all = $state.raw<readonly TreeEntry[] | null>(null);
  references = $state.raw<readonly BibEntry[]>([]);
  referencesLoaded = $state(false);
  filter = $state("");
  opening = $state(false);
  error = $state("");

  readonly rows = $derived(
    this.filter.trim() === ""
      ? visibleRows(this.listings, this.expanded)
      : filterRows(this.all ?? [], this.filter),
  );
  readonly marks = $derived(gitMarks(this.git));
  readonly branch = $derived(branchLabel(this.git));
  readonly figures = $derived(
    (this.all ?? []).filter((e) => e.kind !== "dir" && fileKind(e.path) === "figure"),
  );
  readonly paths = $derived(
    (this.all ?? [])
      .filter((e) => e.kind !== "dir" && e.name !== KEEP_FILE)
      .map((e) => e.path),
  );

  #refreshTimer: ReturnType<typeof setTimeout> | undefined;

  get engine(): Engine {
    return this.project?.engine ?? "typst";
  }

  /* ----------------------------------------------------------------------- */
  /* Projects                                                                 */
  /* ----------------------------------------------------------------------- */

  async loadRecent(): Promise<void> {
    try {
      this.recent = await api.recent();
    } catch {
      this.recent = [];
    }
  }

  /** Open a folder. Resolves to the project, or null with `error` set. */
  async open(path: string): Promise<ProjectInfo | null> {
    this.opening = true;
    this.error = "";
    try {
      return this.#adopt(await api.open(path));
    } catch (error) {
      this.error = errorMessage(error);
      return null;
    } finally {
      this.opening = false;
    }
  }

  async create(
    path: string,
    name: string,
    engine: Engine,
    template?: string,
  ): Promise<ProjectInfo | null> {
    this.opening = true;
    this.error = "";
    try {
      return this.#adopt(await api.create(path, name, engine, template));
    } catch (error) {
      this.error = errorMessage(error);
      return null;
    } finally {
      this.opening = false;
    }
  }

  #adopt(summary: ProjectSummary): ProjectInfo {
    const info: ProjectInfo = {
      root: summary.root,
      name: summary.name,
      engine: summary.engine,
      main: summary.main,
      config: summary.config,
    };
    this.project = info;
    this.listings.clear();
    this.listings.set("", summary.tree.entries);
    this.expanded.clear();
    this.all = null;
    this.references = [];
    this.referencesLoaded = false;
    this.filter = "";
    this.git = null;
    void this.refreshGit();
    void this.loadRecent();
    return info;
  }

  async close(): Promise<void> {
    try {
      await api.close();
    } catch {
      /* Closing a project that is already gone is fine. */
    }
    this.project = null;
    this.listings.clear();
    this.expanded.clear();
    this.all = null;
    this.git = null;
    this.references = [];
    this.filter = "";
    void this.loadRecent();
  }

  /* ----------------------------------------------------------------------- */
  /* Tree                                                                     */
  /* ----------------------------------------------------------------------- */

  async loadDir(path: string): Promise<void> {
    try {
      const page = await api.tree({ path, limit: PAGE });
      this.listings.set(path, page.entries);
    } catch (error) {
      this.error = errorMessage(error);
    }
  }

  async toggle(path: string): Promise<void> {
    if (this.expanded.has(path)) {
      this.expanded.delete(path);
      return;
    }
    this.expanded.add(path);
    if (!this.listings.has(path)) await this.loadDir(path);
  }

  async expand(path: string): Promise<void> {
    if (!this.expanded.has(path)) await this.toggle(path);
  }

  collapse(path: string): void {
    this.expanded.delete(path);
  }

  collapseAll(): void {
    this.expanded.clear();
  }

  /** Expand every folder above `path` so its row exists. */
  async reveal(path: string): Promise<void> {
    for (const dir of ancestors(path)) await this.expand(dir);
  }

  /** The whole project as one flat list, fetched once and kept fresh. */
  async ensureAll(): Promise<void> {
    if (this.all !== null || this.project === null) return;
    try {
      const page = await api.tree({ recursive: true, limit: PAGE });
      this.all = page.entries;
    } catch (error) {
      this.error = errorMessage(error);
    }
  }

  async refreshGit(): Promise<void> {
    if (this.project === null) return;
    try {
      this.git = await api.gitStatus();
    } catch {
      this.git = null;
    }
  }

  /** Reload what is visible: expanded folders, git status, and caches. */
  async refresh(): Promise<void> {
    if (this.project === null) return;
    const dirs = ["", ...this.expanded];
    const fresh = await Promise.all(
      dirs.map(async (dir): Promise<[string, readonly TreeEntry[]] | null> => {
        try {
          return [dir, (await api.tree({ path: dir, limit: PAGE })).entries];
        } catch {
          // A folder that vanished simply drops out of the listing.
          return null;
        }
      }),
    );
    this.listings.clear();
    for (const entry of fresh)
      if (entry !== null) this.listings.set(entry[0], entry[1]);
    for (const dir of [...this.expanded])
      if (!this.listings.has(dir)) this.expanded.delete(dir);
    if (this.all !== null) {
      this.all = null;
      await this.ensureAll();
    }
    if (this.referencesLoaded) await this.loadReferences();
    await this.refreshGit();
  }

  /** Coalesce a burst of `project.changed` events into one refresh. */
  scheduleRefresh(): void {
    clearTimeout(this.#refreshTimer);
    this.#refreshTimer = setTimeout(() => void this.refresh(), 120);
  }

  /** Entry names in a folder, for duplicate checks while naming. */
  namesIn(dir: string): string[] {
    return (this.listings.get(dir) ?? []).map((e) => e.name);
  }

  /* ----------------------------------------------------------------------- */
  /* File operations                                                          */
  /* ----------------------------------------------------------------------- */

  readFile(path: string): Promise<string> {
    return api.readFile(path);
  }

  /** A text file with its fingerprint, for the save-time conflict check. */
  readText(path: string): Promise<TextRead> {
    return api.readText(path);
  }

  /** Write a file; `expected` is the fingerprint it must still have on disk. */
  writeFile(path: string, content: string, expected?: string): Promise<WriteResult> {
    return api.writeFile(path, content, expected);
  }

  /** Create a file with starting content. Throws a kernel error on failure. */
  async createFile(dir: string, name: string, content = ""): Promise<string> {
    const path = join(dir, name);
    await api.writeFile(path, content);
    await this.#touchDir(dir);
    return path;
  }

  /** A folder exists only through what it holds, so a placeholder file keeps it. */
  async createFolder(dir: string, name: string): Promise<string> {
    const path = join(dir, name);
    await api.writeFile(`${path}/.gitkeep`, "");
    await this.#touchDir(dir);
    await this.expand(path);
    return path;
  }

  async rename(from: string, to: string): Promise<void> {
    await api.rename(from, to);
    await this.#touchDir(dirname(from));
    if (dirname(to) !== dirname(from)) await this.#touchDir(dirname(to));
    const open = [...this.expanded];
    this.expanded.clear();
    for (const dir of open) this.expanded.add(moved(dir, from, to));
    this.scheduleRefresh();
  }

  async trash(path: string): Promise<void> {
    await api.trash(path);
    await this.#touchDir(dirname(path));
    this.scheduleRefresh();
  }

  async #touchDir(dir: string): Promise<void> {
    await this.loadDir(dir);
    this.all = null;
    void this.refreshGit();
  }

  /* ----------------------------------------------------------------------- */
  /* References                                                               */
  /* ----------------------------------------------------------------------- */

  /** Parse every bibliography file in the project. A citations plugin replaces this. */
  async loadReferences(): Promise<void> {
    await this.ensureAll();
    const files = this.paths.filter((p) => fileKind(p) === "bibliography");
    const parsed = await Promise.all(
      files.map(async (file) => {
        try {
          return parseBibtex(await api.readFile(file), file);
        } catch {
          return [];
        }
      }),
    );
    this.references = parsed.flat();
    this.referencesLoaded = true;
  }

  /** The engine the file belongs to, falling back to the project's. */
  engineFor(path: string): Engine {
    return engineOf(path) ?? this.engine;
  }
}

export const workspace = new WorkspaceState();
