/**
 * Open figures: one `FigureState` per `.figure.json` tab, loaded and saved
 * through the project plugin.
 */

import { SvelteMap } from "svelte/reactivity";

import { errorMessage, kernel } from "$lib/kernel";

import { svgPathFor } from "$lib/figure/edit";

import { FigureState, parseScene, serializeScene } from "./figure.svelte";
import { notice } from "./notice.svelte";
import { workspace } from "./workspace.svelte";

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

  async open(path: string): Promise<FigureState | null> {
    const existing = this.#open.get(path);
    if (existing !== undefined) return existing;
    try {
      const figure = new FigureState(path, parseScene(await workspace.readFile(path)));
      this.#open.set(path, figure);
      this.version += 1;
      return figure;
    } catch (error) {
      notice.error(`Could not open ${path}: ${errorMessage(error)}`);
      return null;
    }
  }

  isDirty(path: string): boolean {
    return this.#open.get(path)?.dirty ?? false;
  }

  async save(path: string): Promise<boolean> {
    const figure = this.#open.get(path);
    if (figure === undefined) return true;
    const scene = figure.scene;
    try {
      await workspace.writeFile(path, serializeScene(scene));
      figure.markSaved(scene);
    } catch (error) {
      notice.error(`Could not save ${path}: ${errorMessage(error)}`);
      return false;
    }
    // The SVG next to the scene is what documents embed. The scene is already
    // safe on disk, so a rendering failure is reported but does not fail the save.
    try {
      const bytes = await kernel.runBinary("figure.render-svg", { scene });
      await workspace.writeFile(svgPathFor(path), new TextDecoder().decode(bytes));
    } catch (error) {
      notice.error(
        `Saved ${path}, but could not write its SVG: ${errorMessage(error)}`,
      );
    }
    return true;
  }

  renamed(from: string, to: string): void {
    for (const [key, figure] of [...this.#open]) {
      if (key !== from && !key.startsWith(`${from}/`)) continue;
      const next = to + key.slice(from.length);
      this.#open.delete(key);
      figure.path = next;
      this.#open.set(next, figure);
    }
    this.version += 1;
  }

  drop(path: string): void {
    if (this.#open.delete(path)) this.version += 1;
  }

  dropUnder(path: string): void {
    for (const key of [...this.#open.keys()]) {
      if (key === path || key.startsWith(`${path}/`)) this.drop(key);
    }
  }
}

export const figures = new FiguresStore();
