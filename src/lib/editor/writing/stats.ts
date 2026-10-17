/**
 * Word count and statistics for the status bar.
 *
 * Counted by the writing plugin (it skips markup, math, comments and code), at
 * most once a second and only after typing settles. A document is sent whole
 * (the count is global), so very large ones are counted less often, and an
 * unchanged document is never sent twice.
 */

import type { Extension } from "@codemirror/state";
import { language as languageFacet } from "@codemirror/language";
import { ViewPlugin, type EditorView, type ViewUpdate } from "@codemirror/view";

import type { WritingApi } from "$lib/kernel/api";

import { emitStats } from "./bus";
import { IdleRunner } from "./idle";
import { writingSyntax } from "./syntax";

export const STATS_MIN_GAP_MS = 1000;
const LARGE_DOC_CHARS = 400_000;
const LARGE_DOC_GAP_MS = 5000;
const MAX_DOC_CHARS = 4_000_000;

export function stats(api: WritingApi): Extension {
  return ViewPlugin.fromClass(
    class {
      readonly #runner: IdleRunner;
      #counted: unknown = null;
      #dead = false;

      constructor(readonly view: EditorView) {
        this.#runner = new IdleRunner(
          () => this.#run(),
          400,
          () =>
            this.view.state.doc.length > LARGE_DOC_CHARS
              ? LARGE_DOC_GAP_MS
              : STATS_MIN_GAP_MS,
        );
        this.#runner.poke();
      }

      update(update: ViewUpdate): void {
        const relanguaged =
          update.startState.facet(languageFacet) !== update.state.facet(languageFacet);
        if (update.docChanged || relanguaged) this.#runner.poke();
      }

      async #run(): Promise<void> {
        const doc = this.view.state.doc;
        if (doc === this.#counted || doc.length > MAX_DOC_CHARS) return;
        const result = await api.stats(doc.toString(), writingSyntax(this.view.state));
        if (this.#dead) return;
        // Only a count of the current text is shown; a stale one waits for the next pass.
        if (this.view.state.doc !== doc) return;
        this.#counted = doc;
        emitStats(this.view, result);
      }

      destroy(): void {
        this.#dead = true;
        this.#runner.destroy();
      }
    },
  );
}
