/**
 * Writing tools state: prose-lint findings per file (merged into Problems next
 * to compiler diagnostics) and the last word count per file (status bar).
 *
 * The editor extensions publish on a tiny bus (`editor/writing/bus.ts`); this
 * store is the only subscriber and attributes each result to the file the view
 * is showing. Findings cover the visible paragraphs only, and each new result
 * replaces the previous one for that file, so nothing accumulates.
 */

import { subscribeWriting, type FormatReport } from "$lib/editor/writing/bus";
import { setSyntaxResolver } from "$lib/editor/writing/syntax";
import type { TextStats } from "$lib/kernel/api";
import { fromWriting, type Problem } from "$lib/workbench/problems";

import { documents } from "./documents.svelte";
import { notice } from "./notice.svelte";

export class WritingState {
  /** Findings keyed by project-relative path. */
  lint = $state.raw<Readonly<Record<string, readonly Problem[]>>>({});
  stats = $state.raw<Readonly<Record<string, TextStats>>>({});

  /** Every finding of every file, for the Problems panel. */
  readonly problems: readonly Problem[] = $derived(Object.values(this.lint).flat());

  constructor() {
    // The shown view's language comes from its file's extension.
    setSyntaxResolver((state) =>
      documents.view?.state === state ? (documents.shown?.language ?? null) : null,
    );
    subscribeWriting({
      lint: (view, findings) => {
        const path = this.#pathOf(view);
        if (path === null) return;
        this.lint = {
          ...this.lint,
          [path]: fromWriting(path, view.state.doc, findings),
        };
      },
      stats: (view, stats) => {
        const path = this.#pathOf(view);
        if (path === null) return;
        this.stats = { ...this.stats, [path]: stats };
      },
      format: (_view, report) => this.#reportFormat(report),
    });
  }

  #pathOf(view: unknown): string | null {
    return documents.view === view && documents.shown !== undefined
      ? documents.shown.path
      : null;
  }

  #reportFormat(report: FormatReport): void {
    if (report.kind === "failed" || report.kind === "unsupported") {
      notice.error(report.message);
    } else {
      notice.show(report.message);
    }
  }
}

export const writing = new WritingState();
