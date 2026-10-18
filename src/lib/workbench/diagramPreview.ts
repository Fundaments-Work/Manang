/**
 * The live diagram preview, without the reactivity: which file is shown, a
 * debounced render after edits, the last good picture kept (marked stale)
 * while the source has an error, and the diagnostics for Problems.
 *
 * Plain TypeScript with injected kernel calls and URL holder, so the rules are
 * unit-tested; `state/diagram.svelte.ts` mirrors the snapshot into runes.
 */

import { errorMessage } from "$lib/kernel/client";
import type { DiagramApi } from "$lib/kernel/api";
import { sniffArtifact } from "$lib/kernel/artifact";
import type { DiagramLanguageId, DiagramTheme } from "$lib/kernel/types";

import { diagramErrorText, parseDiagramError } from "./diagram";
import { fromDiagram, type Problem } from "./problems";
import { createScheduler } from "./scheduler";

/** Edits settle this long before a render. */
export const DIAGRAM_DEBOUNCE_MS = 150;

export interface DiagramSnapshot {
  /** Project-relative path of the diagram shown, or null when none is. */
  readonly path: string | null;
  readonly language: DiagramLanguageId | null;
  /** Blob URL of the last good SVG. */
  readonly url: string | null;
  /** The theme `url` was rendered with. */
  readonly theme: DiagramTheme | null;
  /** True when `url` no longer matches the source (the source has an error). */
  readonly stale: boolean;
  /** One sentence about the current error, or "". */
  readonly error: string;
  readonly problems: readonly Problem[];
  readonly rendering: boolean;
}

export const EMPTY_DIAGRAM: DiagramSnapshot = {
  path: null,
  language: null,
  url: null,
  theme: null,
  stale: false,
  error: "",
  problems: [],
  rendering: false,
};

export interface UrlHolder {
  set(bytes: Uint8Array, mime: string): string;
  release(): void;
}

export interface DiagramPreviewDeps {
  readonly api: Pick<DiagramApi, "check" | "render">;
  readonly urls: UrlHolder;
  readonly onChange: (snapshot: DiagramSnapshot) => void;
  /** Called with failures that are not the diagram's fault (kernel trouble). */
  readonly onError?: (error: unknown) => void;
  readonly delayMs?: number;
}

interface Input {
  readonly path: string;
  readonly language: DiagramLanguageId;
  readonly source: string;
  readonly theme: DiagramTheme;
}

const sameInput = (a: Input | null, b: Input | null): boolean =>
  a !== null &&
  b !== null &&
  a.path === b.path &&
  a.language === b.language &&
  a.source === b.source &&
  a.theme === b.theme;

export class DiagramPreview {
  readonly #deps: DiagramPreviewDeps;
  readonly #scheduler;
  #snapshot: DiagramSnapshot = EMPTY_DIAGRAM;
  /** What should be on screen. */
  #wanted: Input | null = null;
  /** What the last finished run rendered (or failed on). */
  #done: Input | null = null;

  constructor(deps: DiagramPreviewDeps) {
    this.#deps = deps;
    this.#scheduler = createScheduler(
      () => this.#run(),
      deps.delayMs ?? DIAGRAM_DEBOUNCE_MS,
    );
  }

  get snapshot(): DiagramSnapshot {
    return this.#snapshot;
  }

  #update(patch: Partial<DiagramSnapshot>): void {
    this.#snapshot = { ...this.#snapshot, ...patch };
    this.#deps.onChange(this.#snapshot);
  }

  /**
   * Show `path` (or nothing). A different file, language or theme renders at
   * once; the same file with new text waits for the debounce.
   */
  show(input: Input | null): void {
    const before = this.#wanted;
    this.#wanted = input;
    if (input === null) {
      if (before !== null) this.clear();
      return;
    }
    if (sameInput(input, this.#done) && before !== null) return;
    if (before === null || before.path !== input.path) {
      // A new file: drop the old picture so it is never shown under this name.
      this.#scheduler.cancel();
      this.#deps.urls.release();
      this.#done = null;
      this.#update({
        ...EMPTY_DIAGRAM,
        path: input.path,
        language: input.language,
        rendering: true,
      });
      this.#scheduler.flush();
      return;
    }
    if (before.theme !== input.theme || before.language !== input.language) {
      this.#scheduler.flush();
      return;
    }
    if (before.source !== input.source) this.#scheduler.request();
  }

  /** Render now, even when nothing changed (the preview's Refresh). */
  refresh(): void {
    this.#done = null;
    this.#scheduler.flush();
  }

  clear(): void {
    this.#scheduler.cancel();
    this.#wanted = null;
    this.#done = null;
    this.#deps.urls.release();
    this.#update(EMPTY_DIAGRAM);
  }

  async #run(): Promise<void> {
    const input = this.#wanted;
    if (input === null || sameInput(input, this.#done)) return;
    this.#update({ rendering: true });
    const [checked, rendered] = await Promise.allSettled([
      this.#deps.api.check(input.language, input.source),
      this.#deps.api.render(input.language, input.source, input.theme),
    ]);
    // A newer request replaced this one: its own run will report.
    if (this.#wanted?.path !== input.path) return;
    this.#done = input;

    const diagnostics =
      checked.status === "fulfilled" ? [...checked.value.diagnostics] : [];
    if (checked.status === "rejected") this.#deps.onError?.(checked.reason);

    if (rendered.status === "fulfilled") {
      const bytes = rendered.value;
      if (sniffArtifact(bytes)?.kind !== "svg") {
        this.#update({
          rendering: false,
          stale: this.#snapshot.url !== null,
          error: "The diagram plugin returned something that is not an SVG.",
          problems: fromDiagram(input.path, diagnostics),
        });
        return;
      }
      this.#update({
        url: this.#deps.urls.set(bytes, "image/svg+xml"),
        theme: input.theme,
        stale: false,
        error: "",
        rendering: false,
        problems: fromDiagram(input.path, diagnostics),
      });
      return;
    }

    const message = errorMessage(rendered.reason);
    const position = parseDiagramError(message);
    if (position === null) this.#deps.onError?.(rendered.reason);
    // `check` is the full list; the render error stands in when it found nothing.
    if (position !== null && !diagnostics.some((d) => d.severity === "error")) {
      diagnostics.push(position);
    }
    this.#update({
      stale: this.#snapshot.url !== null,
      error: diagramErrorText(message),
      rendering: false,
      problems: fromDiagram(input.path, diagnostics),
    });
  }
}
