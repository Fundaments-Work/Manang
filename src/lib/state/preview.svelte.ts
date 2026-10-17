/**
 * Preview state: what the preview pane shows for the last good compile.
 *
 *  - Typst: one page at a time from `typst.render`, as raw SVG bytes in a Blob URL.
 *  - LaTeX: the whole PDF from `latex.export-pdf`, raw bytes handed to the PDF
 *    viewer (which renders only the pages on screen).
 *  - Markdown: sanitised HTML straight from the build.
 *
 * Bytes are never base64-encoded. The previous page stays visible until the next
 * one is ready, so the pane never flashes empty while typing.
 */

import { kernel, errorMessage } from "$lib/kernel";
import { latexApi, typstApi } from "$lib/kernel/api";
import { ArtifactUrl, sniffArtifact } from "$lib/kernel/artifact";
import { join } from "$lib/workbench/paths";
import { documentName } from "$lib/workbench/target";

import { build } from "./build.svelte";
import { notice } from "./notice.svelte";
import { workspace } from "./workspace.svelte";

export const ZOOM_STEPS: readonly number[] = [0.5, 0.75, 1, 1.25, 1.5, 2];
const typst = typstApi(kernel);
const latex = latexApi(kernel);

export type PreviewKind = "svg" | "pdf" | "html" | "none";

export class PreviewState {
  /** 0-based. For a PDF it follows the scroll position. */
  page = $state(0);
  /** `null` fits the pane's width. */
  zoom = $state<number | null>(null);
  url = $state<string | null>(null);
  /** The latest PDF, raw. */
  pdf = $state.raw<Uint8Array | null>(null);
  /** Page count the PDF viewer reports (the compile report may not know it). */
  pdfPages = $state(0);
  /** Bumped by `go`, so the PDF viewer scrolls only on requests, not on scroll. */
  jump = $state(0);
  /** The compiled file the shown page or PDF belongs to. */
  shownTarget = $state("");
  error = $state("");
  rendering = $state(false);
  exporting = $state(false);

  readonly #holder = new ArtifactUrl();
  #generation = 0;

  readonly kind: PreviewKind = $derived(
    build.engine === "typst"
      ? "svg"
      : build.engine === "latex"
        ? "pdf"
        : build.engine === "markdown"
          ? "html"
          : "none",
  );

  get pageCount(): number {
    return this.kind === "pdf" && this.pdfPages > 0 ? this.pdfPages : build.pages;
  }

  /** Fetch the artefact of the latest successful compile. */
  async render(): Promise<void> {
    if (build.generation === 0 || build.status === "idle") return;
    const run = ++this.#generation;
    if (this.kind === "html" || this.kind === "none") {
      this.error = "";
      return;
    }
    this.rendering = true;
    try {
      if (this.kind === "pdf") {
        const root = workspace.project?.root ?? "";
        const bytes = await latex.exportPdf(join(root, build.target));
        if (run !== this.#generation) return;
        if (sniffArtifact(bytes)?.kind !== "pdf") {
          this.error = "The compiler returned a file that is not a PDF.";
          return;
        }
        this.pdf = bytes;
        this.shownTarget = build.target;
        this.error = "";
        return;
      }
      if (build.pages === 0) return;
      this.page = Math.min(this.page, Math.max(0, build.pages - 1));
      const bytes = await typst.render(this.page);
      if (run !== this.#generation) return;
      const artifact = sniffArtifact(bytes);
      if (artifact === null) {
        this.error = "The compiler returned a page the preview cannot read.";
        return;
      }
      this.url = this.#holder.set(bytes, artifact.mime);
      this.shownTarget = build.target;
      this.error = "";
    } catch (error) {
      if (run !== this.#generation) return;
      this.error = errorMessage(error);
    } finally {
      if (run === this.#generation) this.rendering = false;
    }
  }

  go(page: number): void {
    const last = Math.max(0, this.pageCount - 1);
    const next = Math.max(0, Math.min(page, last));
    if (this.kind === "pdf") {
      this.page = next;
      this.jump += 1;
      return;
    }
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
      const root = workspace.project?.root ?? "";
      const bytes =
        build.engine === "latex"
          ? await latex.exportPdf(join(root, build.target))
          : await typst.exportPdf();
      const url = URL.createObjectURL(
        new Blob([new Uint8Array(bytes)], { type: "application/pdf" }),
      );
      const link = document.createElement("a");
      link.href = url;
      link.download = `${documentName(name)}.pdf`;
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
    this.pdf = null;
    this.pdfPages = 0;
  }
}

export const preview = new PreviewState();
