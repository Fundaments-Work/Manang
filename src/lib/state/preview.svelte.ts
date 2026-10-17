/**
 * Preview state: the rendered page the preview pane shows.
 *
 * Pages come from `typst.render` as raw SVG bytes and become a Blob URL; they
 * are never base64-encoded. A render follows each successful compile, and the
 * previous page stays visible until the next one is ready, so the pane never
 * flashes empty while typing.
 */

import { kernel, errorMessage } from "$lib/kernel";
import { typstApi } from "$lib/kernel/api";
import { ArtifactUrl, sniffArtifact } from "$lib/kernel/artifact";

import { build } from "./build.svelte";
import { notice } from "./notice.svelte";

export const ZOOM_STEPS: readonly number[] = [0.5, 0.75, 1, 1.25, 1.5, 2];
const api = typstApi(kernel);

export class PreviewState {
  /** 0-based. */
  page = $state(0);
  /** `null` fits the pane's width. */
  zoom = $state<number | null>(null);
  url = $state<string | null>(null);
  error = $state("");
  rendering = $state(false);
  exporting = $state(false);

  readonly #holder = new ArtifactUrl();
  #generation = 0;

  get pageCount(): number {
    return build.pages;
  }

  /** Render the current page of the latest successful compile. */
  async render(): Promise<void> {
    if (build.generation === 0 || build.pages === 0) return;
    const run = ++this.#generation;
    this.page = Math.min(this.page, Math.max(0, build.pages - 1));
    this.rendering = true;
    try {
      const bytes = await api.render(this.page);
      if (run !== this.#generation) return;
      const artifact = sniffArtifact(bytes);
      if (artifact === null) {
        this.error = "The compiler returned a page the preview cannot read.";
        return;
      }
      this.url = this.#holder.set(bytes, artifact.mime);
      this.error = "";
    } catch (error) {
      if (run !== this.#generation) return;
      this.error = errorMessage(error);
    } finally {
      if (run === this.#generation) this.rendering = false;
    }
  }

  go(page: number): void {
    const last = Math.max(0, build.pages - 1);
    const next = Math.max(0, Math.min(page, last));
    if (next === this.page) return;
    this.page = next;
    void this.render();
  }

  zoomIn(): void {
    const current = this.zoom ?? 1;
    this.zoom =
      ZOOM_STEPS.find((s) => s > current + 0.001) ??
      ZOOM_STEPS[ZOOM_STEPS.length - 1] ??
      2;
  }

  zoomOut(): void {
    const current = this.zoom ?? 1;
    this.zoom =
      [...ZOOM_STEPS].reverse().find((s) => s < current - 0.001) ??
      ZOOM_STEPS[0] ??
      0.5;
  }

  fit(): void {
    this.zoom = null;
  }

  /** Ask the compiler for a PDF and hand it to the browser as a download. */
  async exportPdf(name: string): Promise<void> {
    this.exporting = true;
    try {
      await build.compile();
      if (build.status === "failed") {
        notice.error("Fix the errors in the Problems panel before exporting a PDF.");
        return;
      }
      const bytes = await api.exportPdf();
      const url = URL.createObjectURL(
        new Blob([new Uint8Array(bytes)], { type: "application/pdf" }),
      );
      const link = document.createElement("a");
      link.href = url;
      link.download = name.replace(/\.[^.]+$/, "") + ".pdf";
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 10_000);
      notice.show(`Exported ${link.download}.`);
    } catch (error) {
      notice.error(`Could not export a PDF: ${errorMessage(error)}`);
    } finally {
      this.exporting = false;
    }
  }

  release(): void {
    this.#generation += 1;
    this.#holder.release();
    this.url = null;
  }
}

export const preview = new PreviewState();
