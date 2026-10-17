/**
 * The shared shape of a background check (spelling, prose lint): wait for idle,
 * look at the visible paragraphs, ask only about the ones not cached, and apply
 * the answer only if the document is still the one that was asked about.
 */

import { language } from "@codemirror/language";
import { StateEffect, type Extension, type StateEffectType } from "@codemirror/state";
import { ViewPlugin, type EditorView, type ViewUpdate } from "@codemirror/view";

import type { ScalarRange, WritingSyntax } from "$lib/kernel/api";

import { collectChunks, Lru, resolveChunks, type Chunk, type RelItem } from "./chunks";
import { IdleRunner } from "./idle";
import { writingSyntax } from "./syntax";

export interface CheckerConfig<R, T extends RelItem> {
  readonly request: (text: string, syntax: WritingSyntax) => Promise<readonly R[]>;
  readonly range: (raw: R) => ScalarRange | null;
  readonly make: (raw: R, start: number, end: number) => T;
  /** Called with the findings of every visible chunk, for the current document. */
  readonly apply: (
    view: EditorView,
    chunks: readonly Chunk[],
    found: (readonly T[])[],
  ) => void;
  /** Paragraphs remembered. The cache is the only thing that grows; this caps it. */
  readonly cacheLimit?: number;
  readonly idleMs?: number;
  readonly minGapMs?: number;
}

/** Dispatch one of these to forget cached answers (e.g. the dictionary changed). */
export type Invalidation = StateEffectType<null>;

export function newInvalidation(): Invalidation {
  return StateEffect.define<null>();
}

export function checker<R, T extends RelItem>(
  config: CheckerConfig<R, T>,
  invalidate: Invalidation,
): Extension {
  return ViewPlugin.fromClass(
    class {
      readonly #cache = new Lru<readonly T[]>(config.cacheLimit ?? 400);
      readonly #runner: IdleRunner;
      #dead = false;

      constructor(readonly view: EditorView) {
        this.#runner = new IdleRunner(
          () => this.#run(),
          config.idleMs,
          config.minGapMs,
        );
        this.#runner.poke();
      }

      update(update: ViewUpdate): void {
        const flushed = update.transactions.some((tr) =>
          tr.effects.some((effect) => effect.is(invalidate)),
        );
        const relanguaged =
          update.startState.facet(language) !== update.state.facet(language);
        if (flushed || relanguaged) this.#cache.clear();
        if (update.docChanged || update.viewportChanged || flushed || relanguaged) {
          this.#runner.poke();
        }
      }

      async #run(): Promise<void> {
        const view = this.view;
        const doc = view.state.doc;
        const syntax = writingSyntax(view.state);
        const chunks = collectChunks(doc, view.visibleRanges, syntax);
        const found = await resolveChunks({
          chunks,
          cache: this.#cache,
          request: (text) => config.request(text, syntax),
          range: config.range,
          make: config.make,
        });
        // The answer describes `doc`; if the author typed meanwhile it is stale.
        // It is already cached by text, so the next pass reuses it.
        if (this.#dead || view.state.doc !== doc) return;
        config.apply(view, chunks, found);
      }

      destroy(): void {
        this.#dead = true;
        this.#runner.destroy();
      }
    },
  );
}
