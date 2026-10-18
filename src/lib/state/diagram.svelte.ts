/**
 * The diagram preview's reactive state: follows the active tab, its text and
 * the app theme, and mirrors `workbench/diagramPreview.ts` (where the rules
 * live) into runes for the preview and Problems.
 */

import { untrack } from "svelte";

import { kernel, reportError } from "$lib/kernel";
import { diagramApi } from "$lib/kernel/api";
import { ArtifactUrl } from "$lib/kernel/artifact";
import { diagramLanguageOf, diagramTheme, DIAGRAM_NAMES } from "$lib/workbench/diagram";
import {
  DiagramPreview,
  EMPTY_DIAGRAM,
  type DiagramSnapshot,
} from "$lib/workbench/diagramPreview";
import type { Problem } from "$lib/workbench/problems";

import { documents } from "./documents.svelte";
import { layout } from "./layout.svelte";
import { tabs } from "./tabs.svelte";

export class DiagramState {
  snapshot = $state.raw<DiagramSnapshot>(EMPTY_DIAGRAM);

  readonly #core = new DiagramPreview({
    api: diagramApi(kernel),
    urls: new ArtifactUrl(),
    onChange: (next) => (this.snapshot = next),
    onError: (error) => reportError("diagram preview", error),
  });
  #started = false;

  /** True while the active tab is a diagram file. */
  get active(): boolean {
    return this.snapshot.path !== null;
  }

  get problems(): readonly Problem[] {
    return this.snapshot.problems;
  }

  /** "Mermaid", "Pikchr", ... for the preview header. */
  get name(): string {
    const language = this.snapshot.language;
    return language === null ? "" : DIAGRAM_NAMES[language];
  }

  /** Follow the active tab, its edits and the theme. Call once at boot. */
  start(): void {
    if (this.#started) return;
    this.#started = true;
    documents.onEdit(() => this.#sync());
    $effect.root(() => {
      $effect(() => {
        void tabs.active?.id;
        void tabs.active?.view;
        void documents.version;
        void layout.theme;
        untrack(() => this.#sync());
      });
    });
  }

  refresh(): void {
    this.#core.refresh();
  }

  #sync(): void {
    const tab = tabs.active;
    const path = tab?.kind === "document" && tab.view === "text" ? tab.path : null;
    const language = path === null ? null : diagramLanguageOf(path);
    if (path === null || language === null) {
      this.#core.show(null);
      return;
    }
    // The session appears once the file is read; `documents.version` re-runs this.
    const session = documents.get(path);
    if (session === undefined) return;
    this.#core.show({
      path,
      language,
      source: session.text(),
      theme: diagramTheme(layout.theme),
    });
  }
}

export const diagram = new DiagramState();
