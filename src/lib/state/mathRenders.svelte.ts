/**
 * Formula renderings for the canvas, cached by `(language, source, size,
 * colour)` so editing anything else (moving a shape, changing a fill) never
 * renders a formula again. A rendering is the plugin's vector SVG
 * (`figure.render-math`: Typst for Typst math, RaTeX for LaTeX math) drawn
 * through a blob URL with `<image>`, never as markup in the DOM. A formula
 * that does not compile caches its positioned error instead, so the canvas
 * can keep the old rendering on screen, dimmed, and the properties panel can
 * say what is wrong.
 */

import { untrack } from "svelte";
import { SvelteMap } from "svelte/reactivity";

import {
  mathKey,
  parseMathError,
  svgSize,
  type MathRender,
} from "$lib/figure/mathRender";
import type { MathLanguage } from "$lib/figure/scene";
import { errorMessage, kernel } from "$lib/kernel";
import { figureApi } from "$lib/kernel/api";

const api = figureApi(kernel);

export class MathRenders {
  readonly #renders = new SvelteMap<string, MathRender>();
  /** Renders in flight. A plain record, not reactive: `get` runs inside deriveds. */
  readonly #pending: Record<string, Promise<MathRender> | undefined> = {};
  readonly #order: string[] = [];
  readonly #limit: number;

  constructor(limit = 160) {
    this.#limit = limit;
  }

  /**
   * The rendering for these inputs, or undefined while it is being made.
   * Reactive: a caller inside an effect or `$derived` re-reads when it lands.
   */
  get(
    language: MathLanguage,
    source: string,
    size: number,
    colour: string,
  ): MathRender | undefined {
    const key = mathKey(language, source, size, colour);
    const found = this.#renders.get(key);
    if (found === undefined) void this.load(language, source, size, colour);
    return found;
  }

  /** Renders (or finds) the formula; never rejects. */
  load(
    language: MathLanguage,
    source: string,
    size: number,
    colour: string,
  ): Promise<MathRender> {
    const key = mathKey(language, source, size, colour);
    const found = untrack(() => this.#renders.get(key));
    if (found !== undefined) return Promise.resolve(found);
    const running = this.#pending[key];
    if (running !== undefined) return running;
    const task = this.#render(key, language, source, size, colour).finally(() => {
      this.#pending[key] = undefined;
    });
    this.#pending[key] = task;
    return task;
  }

  async #render(
    key: string,
    language: MathLanguage,
    source: string,
    size: number,
    colour: string,
  ): Promise<MathRender> {
    // Never write reactive state synchronously: `get` is called from deriveds.
    await Promise.resolve();
    let render: MathRender;
    if (source.trim() === "") {
      render = { error: { message: "the formula is empty" } };
    } else {
      try {
        const bytes = await api.renderMath(language, source, size, colour);
        const svg = new TextDecoder().decode(bytes);
        const box = svgSize(svg);
        const blob = new Blob([bytes.slice()], { type: "image/svg+xml" });
        render = {
          url: URL.createObjectURL(blob),
          ...(box === undefined ? {} : box),
        };
      } catch (error) {
        // A broken formula is the user's input, not a fault: no report.
        render = { error: parseMathError(errorMessage(error)) };
      }
    }
    this.#renders.set(key, render);
    this.#order.push(key);
    while (this.#order.length > this.#limit) {
      const old = this.#order.shift();
      if (old === undefined) break;
      const gone = this.#renders.get(old);
      if (gone?.url !== undefined) URL.revokeObjectURL(gone.url);
      this.#renders.delete(old);
    }
    return render;
  }
}

export const mathRenders = new MathRenders();
