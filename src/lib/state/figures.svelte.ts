/**
 * Open figures: one `FigureState` per `.figure.json` tab. The figure plugin
 * (`figure.*`) holds each scene; this maps project paths to the plugin's ids
 * and turns failures into notices.
 */

import { SvelteMap } from "svelte/reactivity";

import { embedLine, svgPathFor } from "$lib/figure/edit";
import { errorMessage, kernel, reportError } from "$lib/kernel";
import { diagramApi, figureApi, projectApi } from "$lib/kernel/api";
import type { DiagramTheme } from "$lib/kernel/types";
import { basename, dirname } from "$lib/workbench/paths";

import { FigureState, type FigureBackend, type MermaidResult } from "./figure.svelte";
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

  get(path: string): FigureState | undefined {
    return this.#open.get(path);
  }

  all(): FigureState[] {
    void this.version;
    return [...this.#open.values()];
  }

  async #adopt(path: string, id: number): Promise<FigureState> {
    const figure = new FigureState(path, await api.get(id), backendFor(id), id);
    this.#open.set(path, figure);
    this.version += 1;
    this.#listen();
    return figure;
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
      await figure.save();
      return true;
    } catch (error) {
      reportError("figures.save", error);
      notice.error(`Could not save ${path}: ${errorMessage(error)}`);
      return false;
    }
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
