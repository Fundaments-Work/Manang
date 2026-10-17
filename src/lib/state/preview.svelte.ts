/**
 * Preview state: the latest rendered artefact for the active document.
 *
 * `command` is configurable. Left unset, the first of `typst.render` /
 * `latex.export-pdf` that the kernel offers is used, so the pane lights up as
 * soon as such a plugin is installed and stays on its empty state until then.
 */

import { kernel } from "$lib/kernel";
import { ArtifactUrl, type ArtifactKind } from "$lib/kernel/artifact";
import {
  DEFAULT_PREVIEW_COMMANDS,
  pickPreviewCommand,
  renderPreview,
} from "$lib/kernel/preview";
import { session } from "$lib/state/session.svelte";

export type PreviewStatus = "empty" | "loading" | "ready" | "error";

const RENDER_DEBOUNCE_MS = 400;

export class PreviewState {
  /** Explicit command id; `null` means auto-detect from the defaults. */
  command = $state<string | null>(null);

  /** Whether any producing command exists. Drives whether the pane is offered. */
  available = $state(false);

  status = $state<PreviewStatus>("empty");
  kind = $state<ArtifactKind | null>(null);
  url = $state<string | null>(null);
  message = $state("");

  readonly #holder = new ArtifactUrl();
  #generation = 0;
  #timer: ReturnType<typeof setTimeout> | undefined;

  /** Probe the kernel for a producing command. */
  async detect(): Promise<void> {
    const candidates =
      this.command === null ? DEFAULT_PREVIEW_COMMANDS : [this.command];
    this.available = (await pickPreviewCommand(kernel, candidates)) !== null;
  }

  /** Debounced render, for use while typing. */
  schedule(): void {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => void this.render(), RENDER_DEBOUNCE_MS);
  }

  async render(): Promise<void> {
    const generation = ++this.#generation;
    const candidates =
      this.command === null ? DEFAULT_PREVIEW_COMMANDS : [this.command];
    const command = await pickPreviewCommand(kernel, candidates);
    this.available = command !== null;
    if (command === null) {
      this.#show({ status: "empty" });
      return;
    }

    this.status = "loading";
    // Let queued edits reach the kernel so the render sees what is on screen.
    await session.sync.idle();
    const result = await renderPreview(kernel, command, {
      id: session.sync.docId,
    });
    // A newer render superseded this one while it was in flight.
    if (generation !== this.#generation) return;

    if (result.state === "ready") {
      const url = this.#holder.set(result.bytes, result.artifact.mime);
      this.#show({ status: "ready", kind: result.artifact.kind, url });
    } else if (result.state === "error") {
      this.#show({ status: "error", message: result.message });
    } else {
      this.#show({ status: "empty" });
    }
  }

  /** Free the Blob URL, e.g. when the pane is closed. */
  release(): void {
    clearTimeout(this.#timer);
    this.#generation += 1;
    this.#holder.release();
    this.url = null;
  }

  #show(next: {
    status: PreviewStatus;
    kind?: ArtifactKind;
    url?: string;
    message?: string;
  }): void {
    if (next.status !== "ready") {
      this.#holder.release();
    }
    this.status = next.status;
    this.kind = next.kind ?? null;
    this.url = next.url ?? null;
    this.message = next.message ?? "";
  }
}

export const preview = new PreviewState();
