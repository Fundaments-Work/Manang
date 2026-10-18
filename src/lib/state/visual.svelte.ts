/**
 * Visual editor mode per document.
 *
 * Visual is opt-in. A document is in Visual when its tab was switched to it, or
 * when the `editor.visual-default` setting lists its format and the person has
 * not chosen for that tab yet. Switching a tab also updates the setting for its
 * format, so the next `.md` or `.typ` file opens the way the last one was left.
 *
 * The CodeMirror extension and its parts are a separate chunk, imported the
 * first time a document is switched on, so Source mode never pays for them.
 */

import { SvelteMap } from "svelte/reactivity";

import type { VisualHost } from "$lib/editor/visual/host";
import { kernel, reportError } from "$lib/kernel";
import { figureApi, projectApi } from "$lib/kernel/api";
import { visualApi } from "$lib/editor/visual/client";
import { openFile } from "$lib/state/actions";
import { documents, type DocSession } from "$lib/state/documents.svelte";
import { settings } from "$lib/state/settings.svelte";
import { workspace } from "$lib/state/workspace.svelte";
import {
  VISUAL_DEFAULT_SETTING,
  figureSceneFor,
  opensVisual,
  resolveAsset,
  visualFormat,
  withVisualDefault,
} from "$lib/workbench/visual";

const project = projectApi(kernel);
const figure = figureApi(kernel);
const api = visualApi(kernel);

/** The colour formulas are drawn in: the editor's text colour, read at draw time. */
function textColour(): string {
  const value = getComputedStyle(document.documentElement)
    .getPropertyValue("--mng-text")
    .trim();
  return /^#[0-9a-f]{6}$/i.test(value) ? value : "#e8e6e1";
}

function hostFor(session: DocSession): VisualHost {
  return {
    get path() {
      return session.path;
    },
    api,
    readBytes: (path) => project.readBytes(path),
    renderMath: async (language, source, size) => {
      const color = textColour();
      const bytes =
        language === "typst"
          ? await kernel.runBinary("typst.math-to-svg", { source, size, color })
          : await figure.renderMath("latex", source, size, color);
      return new TextDecoder().decode(bytes);
    },
    resolveAsset: (src) => resolveAsset(session.path, src),
    figureFor: (svgPath) =>
      figureSceneFor(svgPath, new Set(workspace.figures.map((f) => f.path))),
    openFigure: (scene) => void openFile(scene),
  };
}

/** The Visual editor is its own chunk: Source mode never loads it. */
const loadExtension = () => import("$lib/editor/visual/extension");

export class VisualMode {
  /** Per path: the person's choice for this tab. Absent: follow the setting. */
  readonly #choice = new SvelteMap<string, boolean>();
  #chunk: ReturnType<typeof loadExtension> | undefined;
  /** Paths whose extension is currently installed, to avoid redundant work. */
  readonly #installed = new SvelteMap<string, boolean>();

  /** Whether the file has a Visual mode at all. */
  available(path: string): boolean {
    return visualFormat(path) !== null;
  }

  /** Whether the tab for `path` is in Visual mode. */
  isOn(path: string): boolean {
    const format = visualFormat(path);
    if (format === null) return false;
    const choice = this.#choice.get(path);
    if (choice !== undefined) return choice;
    return opensVisual(settings.value(VISUAL_DEFAULT_SETTING), format);
  }

  /** Switch a tab, and remember the choice for its file type. */
  async set(path: string, on: boolean): Promise<void> {
    const format = visualFormat(path);
    if (format === null) return;
    this.#choice.set(path, on);
    const next = withVisualDefault(settings.value(VISUAL_DEFAULT_SETTING), format, on);
    void settings.set(VISUAL_DEFAULT_SETTING, next, "user").then((problem) => {
      if (problem !== "") reportError("visual default", new Error(problem));
    });
    await this.apply(path);
  }

  toggle(path: string): Promise<void> {
    return this.set(path, !this.isOn(path));
  }

  /**
   * Make the document's editor match its mode. Called when a document is
   * shown and when its mode changes; a no-op when it already matches.
   */
  async apply(path: string): Promise<void> {
    const session = documents.get(path);
    if (session === undefined) return;
    const on = this.isOn(path);
    if (this.#installed.get(path) === on) return;
    if (!on) {
      this.#installed.set(path, false);
      session.setVisual([]);
      return;
    }
    try {
      this.#chunk ??= loadExtension();
      const { visualEditor } = await this.#chunk;
      // The mode may have been switched back while the chunk loaded.
      if (!this.isOn(path) || documents.get(path) !== session) return;
      this.#installed.set(path, true);
      session.setVisual(visualEditor(hostFor(session)));
    } catch (error) {
      this.#chunk = undefined;
      reportError("the visual editor could not be loaded", error);
    }
  }

  /** A document was closed: forget what was installed for it. */
  forget(path: string): void {
    this.#installed.delete(path);
    this.#choice.delete(path);
  }
}

export const visualMode = new VisualMode();

documents.onDrop((path) => visualMode.forget(path));
