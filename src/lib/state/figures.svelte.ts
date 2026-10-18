/**
 * Open figures: one `FigureState` per `.figure.json` tab. The figure plugin
 * (`figure.*`) holds each scene; this maps project paths to the plugin's ids
 * and turns failures into notices.
 */

import { SvelteMap } from "svelte/reactivity";

import { embedLine, svgPathFor } from "$lib/figure/edit";
import { errorMessage, kernel, reportError } from "$lib/kernel";
import { diagramApi, figureApi, projectApi } from "$lib/kernel/api";
import type { ProjectChange } from "$lib/kernel/types";
import { decideExternal, isNotFoundError } from "$lib/workbench/external";
import type { DiagramTheme } from "$lib/kernel/types";
import { basename, dirname } from "$lib/workbench/paths";

import { FigureState, type FigureBackend, type MermaidResult } from "./figure.svelte";
import { dialog } from "./dialog.svelte";
import { notice } from "./notice.svelte";
import { workspace } from "./workspace.svelte";

const api = figureApi(kernel);
const project = projectApi(kernel);

/** The plugin works on absolute paths; the workbench on project-relative ones. */
function absolute(path: string): string {
  const root = workspace.project?.root;
  if (root === undefined) throw new Error("No project is open.");
  return `${root.replace(/\/+$/, "")}/${path}`;
}

function backendFor(id: number): FigureBackend {
  return {
    apply: (op) => api.apply(id, op),
    get: () => api.get(id),
    save: (path) => api.save(id, absolute(path)),
    close: async () => {
      await api.close(id);
    },
    exportSvg: async () => new TextDecoder().decode(await api.exportSvg(id)),
    fromMermaid: (args) =>
      kernel.run<MermaidResult>("figure.from-mermaid", { figure: id, ...args }),
  };
}

export class FiguresStore {
  readonly #open = new SvelteMap<string, FigureState>();
  /** Bumped when a figure is added or removed, so dirty lists refresh. */
  version = $state(0);

  readonly #changeListeners: ((path: string) => void)[] = [];
  readonly #saveListeners: ((path: string) => void)[] = [];

  /** Called with the path of a figure after its scene reached the disk. */
  onSave(listener: (path: string) => void): void {
    this.#saveListeners.push(listener);
  }

  /** Called with the path of a figure each time its scene changes. */
  onChange(listener: (path: string) => void): void {
    this.#changeListeners.push(listener);
  }

  get(path: string): FigureState | undefined {
    return this.#open.get(path);
  }

  all(): FigureState[] {
    void this.version;
    return [...this.#open.values()];
  }

  async #adopt(path: string, id: number): Promise<FigureState> {
    const figure = new FigureState(path, await api.get(id), backendFor(id), id);
    this.#hook(figure);
    void this.#noteDisk(figure);
    this.#open.set(path, figure);
    this.version += 1;
    this.#listen();
    return figure;
  }

  #hook(figure: FigureState): void {
    figure.onEdit = () => {
      for (const listener of this.#changeListeners) listener(figure.path);
    };
    figure.onSaved = () => {
      void this.#noteDisk(figure);
      for (const listener of this.#saveListeners) listener(figure.path);
    };
  }

  #unlisten: Promise<() => void> | null = null;

  /**
   * Follow `figure.changed` (edits by agents and other plugins): the open
   * figure with that id resyncs when the revision is newer than its own.
   */
  #listen(): void {
    if (this.#unlisten !== null) return;
    this.#unlisten = kernel.onEvent((event) => {
      if (event.topic !== "figure.changed") return;
      const payload = event.payload as { id?: unknown; revision?: unknown } | null;
      if (typeof payload?.id !== "number" || typeof payload.revision !== "number")
        return;
      const revision = payload.revision;
      for (const figure of this.#open.values()) {
        if (figure.id === payload.id) {
          figure
            .externalChange(revision)
            .catch((e: unknown) => reportError("figure.changed", e));
        }
      }
    });
  }

  #stopListening(): void {
    if (this.#open.size > 0 || this.#unlisten === null) return;
    const pending = this.#unlisten;
    this.#unlisten = null;
    pending
      .then((stop) => stop())
      .catch((e: unknown) => reportError("figure.changed", e));
  }

  /** Remember the fingerprint of the scene file as it is now. */
  async #noteDisk(figure: FigureState): Promise<void> {
    try {
      figure.diskHash = (await project.readText(figure.path)).hash;
    } catch {
      figure.diskHash = null;
    }
  }

  /**
   * The watcher reported changes: an open figure whose file changed outside
   * reloads when it has no unsaved edits, and is marked (never overwritten)
   * when it has. A figure whose file went away keeps its tab.
   */
  async externalChanges(changes: readonly ProjectChange[]): Promise<void> {
    for (const change of changes) {
      const gone = change.kind === "removed" ? change.path : change.from;
      if (gone !== undefined) {
        const figure = this.#open.get(gone);
        if (figure !== undefined && figure.external !== "deleted") {
          figure.external = "deleted";
          notice.error(`${basename(gone)} was deleted or moved on disk.`);
        }
      }
      if (change.kind === "removed") continue;
      const figure = this.#open.get(change.path);
      if (figure === undefined) continue;
      // Our own save comes back through the watcher a moment later.
      if (Date.now() - figure.savedAt < 2500) continue;
      let disk: { content: string; hash: string } | null;
      try {
        disk = await project.readText(change.path);
      } catch (error) {
        if (!isNotFoundError(error)) {
          reportError("figures.externalChange", error);
          continue;
        }
        disk = null;
      }
      if (this.#open.get(change.path) !== figure) continue;
      const action = decideExternal(
        { dirty: figure.dirty, diskHash: figure.diskHash, savedText: "\0", text: "\0" },
        disk,
      );
      if (disk === null) continue;
      if (action.kind === "reload") await this.reload(change.path);
      else if (action.kind === "ignore") figure.diskHash = disk.hash;
      else if (action.kind === "conflict") {
        const first = figure.external === null;
        figure.external = "changed";
        if (first) {
          notice.error(
            `${basename(change.path)} changed on disk. Your unsaved drawing is kept: choose how to continue in the bar above the editor.`,
          );
        }
      }
    }
  }

  /**
   * Replace an open figure with what is on disk, keeping its tab, tool and
   * view. The old scene's unsaved edits are dropped; the caller asks first.
   */
  async reload(path: string): Promise<boolean> {
    const old = this.#open.get(path);
    if (old === undefined) return false;
    try {
      const { id } = await api.open(absolute(path));
      const fresh = new FigureState(path, await api.get(id), backendFor(id), id);
      fresh.view = old.view;
      fresh.tool = old.tool;
      fresh.panelOpen = old.panelOpen;
      this.#hook(fresh);
      this.#open.set(path, fresh);
      this.version += 1;
      await this.#noteDisk(fresh);
      old.close().catch((e: unknown) => reportError("figures.close", e));
      return true;
    } catch (error) {
      reportError("figures.reload", error);
      notice.error(`Could not reload ${path}: ${errorMessage(error)}`);
      return false;
    }
  }

  /** "Keep my version": acknowledge the disk file; the next save replaces it. */
  keepMine(path: string): void {
    const figure = this.#open.get(path);
    if (figure === undefined) return;
    figure.external = null;
    void this.#noteDisk(figure);
  }

  /** "Load disk version": drop the unsaved drawing and show the file. */
  async loadDisk(path: string): Promise<void> {
    await this.reload(path);
  }

  /** Opens in flight, so a tab and a click asking at once share one load. */
  readonly #loading = new SvelteMap<string, Promise<FigureState | null>>();

  open(path: string): Promise<FigureState | null> {
    const existing = this.#open.get(path);
    if (existing !== undefined) return Promise.resolve(existing);
    const pending = this.#loading.get(path);
    if (pending !== undefined) return pending;
    const load = this.#load(path).finally(() => this.#loading.delete(path));
    this.#loading.set(path, load);
    return load;
  }

  async #load(path: string): Promise<FigureState | null> {
    try {
      const { id } = await api.open(absolute(path));
      return await this.#adopt(path, id);
    } catch (error) {
      reportError("figures.open", error);
      notice.error(`Could not open ${path}: ${errorMessage(error)}`);
      return null;
    }
  }

  /**
   * Create `path` (a `.figure.json`) through the plugin, which writes the
   * scene and its SVG at once, and hold it open. Throws a readable error.
   */
  async create(path: string): Promise<FigureState> {
    const { id } = await api.createAt(absolute(path), 960, 540);
    return this.#adopt(path, id);
  }

  isDirty(path: string): boolean {
    return this.#open.get(path)?.dirty ?? false;
  }

  /** Save the scene and its SVG; false (with a notice) on failure. */
  async save(path: string): Promise<boolean> {
    const figure = this.#open.get(path);
    if (figure === undefined) return true;
    try {
      if (!(await this.#confirmOverwrite(figure))) return false;
      await figure.save();
      return true;
    } catch (error) {
      reportError("figures.save", error);
      notice.error(`Could not save ${path}: ${errorMessage(error)}`);
      return false;
    }
  }

  /**
   * Saving replaces the scene file. If another program changed it since this
   * figure loaded it, ask first (checked against the disk now, not only the
   * watcher's events).
   */
  async #confirmOverwrite(figure: FigureState): Promise<boolean> {
    const path = figure.path;
    let changed = figure.external === "changed";
    if (!changed && figure.diskHash !== null) {
      try {
        changed = (await project.readText(path)).hash !== figure.diskHash;
      } catch {
        changed = false; // Gone: saving simply writes it again.
      }
    }
    if (!changed) return true;
    const choice = await dialog.ask({
      title: `${basename(path)} changed on disk`,
      message:
        "Another program changed this figure after Manang loaded it. Saving now would replace its changes with yours.",
      actions: [
        { id: "overwrite", label: "Overwrite with my version" },
        { id: "load", label: "Load disk version" },
      ],
    });
    if (choice === "overwrite") {
      await project
        .backup(path)
        .catch((e: unknown) => reportError("figures.backup", e));
      figure.external = null;
      return true;
    }
    if (choice === "load") await this.reload(path);
    return false;
  }

  /**
   * The embed line for a figure, after making sure the SVG it names is on
   * disk and current: unsaved changes are saved (scene and SVG), and a saved
   * figure whose SVG was never written (or was deleted) gets just the SVG.
   */
  async embed(path: string): Promise<string | null> {
    const figure = this.#open.get(path);
    if (figure === undefined) return null;
    const svg = svgPathFor(path);
    try {
      await figure.idle();
      if (figure.dirty) {
        await figure.save();
      } else if (!(await this.#exists(svg))) {
        // Only the SVG is missing: write it alone, leaving the scene file as it is.
        await project.writeFile(svg, await figure.exportSvg());
      }
      // LaTeX includes the PDF next to the SVG; the plugin skips a current one
      // and writes none in projects without LaTeX.
      if (workspace.engine === "latex") await project.exportPdf(svg);
    } catch (error) {
      reportError("figures.embed", error);
      notice.error(`Could not write ${svg}: ${errorMessage(error)}`);
      return null;
    }
    return embedLine(svg, workspace.engine);
  }

  async #exists(path: string): Promise<boolean> {
    const dir = dirname(path);
    await workspace.loadDir(dir);
    if (workspace.namesIn(dir).includes(basename(path))) return true;
    // A folder listing can be paged; ask for the file itself to be sure.
    try {
      await project.readBytes(path);
      return true;
    } catch {
      return false;
    }
  }

  renamed(from: string, to: string): void {
    for (const [key, figure] of [...this.#open]) {
      if (key !== from && !key.startsWith(`${from}/`)) continue;
      const next = to + key.slice(from.length);
      this.#open.delete(key);
      // The next save passes the new path, so the plugin follows the rename.
      figure.path = next;
      this.#open.set(next, figure);
    }
    this.version += 1;
  }

  drop(path: string): void {
    const figure = this.#open.get(path);
    if (figure === undefined) return;
    this.#open.delete(path);
    this.version += 1;
    figure.close().catch((error: unknown) => reportError("figures.close", error));
    this.#stopListening();
  }

  dropUnder(path: string): void {
    for (const key of [...this.#open.keys()]) {
      if (key === path || key.startsWith(`${path}/`)) this.drop(key);
    }
  }
}

export const figures = new FiguresStore();

/** One Mermaid block's picture: a blob URL, or why it could not render. */
export interface MermaidRender {
  readonly url?: string;
  readonly error?: string;
}

const diagrams = diagramApi(kernel);

/**
 * Rendered Mermaid blocks for the canvas, by theme and source, as blob URLs
 * drawn with `<image>` (never SVG markup in the DOM). Least recently used
 * renders beyond the limit are dropped and their URLs revoked.
 */
export class MermaidRenders {
  readonly #renders = new SvelteMap<string, MermaidRender>();
  readonly #order: string[] = [];
  readonly #limit: number;

  constructor(limit = 48) {
    this.#limit = limit;
  }

  /** The render for `source` in `theme`; starts one when there is none yet. */
  get(source: string, theme: DiagramTheme): MermaidRender | undefined {
    const key = `${theme}\n${source}`;
    const found = this.#renders.get(key);
    if (found !== undefined || this.#order.includes(key)) return found;
    this.#order.push(key);
    void this.#render(key, source, theme);
    return undefined;
  }

  async #render(key: string, source: string, theme: DiagramTheme): Promise<void> {
    let render: MermaidRender;
    try {
      const bytes = await diagrams.render("mermaid", source, theme);
      const blob = new Blob([bytes.slice()], { type: "image/svg+xml" });
      render = { url: URL.createObjectURL(blob) };
    } catch (error) {
      reportError("figure.mermaid-block", error);
      render = { error: errorMessage(error) };
    }
    if (!this.#order.includes(key)) {
      if (render.url !== undefined) URL.revokeObjectURL(render.url);
      return;
    }
    this.#renders.set(key, render);
    while (this.#order.length > this.#limit) {
      const old = this.#order.shift();
      if (old === undefined) break;
      const gone = this.#renders.get(old);
      if (gone?.url !== undefined) URL.revokeObjectURL(gone.url);
      this.#renders.delete(old);
    }
  }
}

export const mermaidRenders = new MermaidRenders();
