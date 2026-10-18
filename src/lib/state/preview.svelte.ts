/**
 * Preview state: what the preview pane shows for the last good compile.
 *
 *  - Typst and LaTeX: the whole PDF (`typst.export-pdf`, `latex.export-pdf`),
 *    raw bytes handed to one PDF viewer (ADR 0006, amended 2026-10-18), which
 *    draws only the pages on screen and gives both engines text selection,
 *    find, links, the outline and zoom.
 *  - Markdown: sanitised HTML straight from the build.
 *
 * Bytes are never base64-encoded. The previous PDF stays on screen until the
 * next one has been drawn, so the pane never flashes empty while typing.
 */

import { kernel, errorMessage } from "$lib/kernel";
import { latexApi, typstApi } from "$lib/kernel/api";
import { sniffArtifact } from "$lib/kernel/artifact";
import { join } from "$lib/workbench/paths";
import { documentName } from "$lib/workbench/target";

import { build } from "./build.svelte";
import { notice } from "./notice.svelte";
import { PdfView, type PreviewMark } from "./pdfView.svelte";
import { workspace } from "./workspace.svelte";

export { MARK_MS, type PreviewMark } from "./pdfView.svelte";

const typst = typstApi(kernel);
const latex = latexApi(kernel);

export type PreviewKind = "pdf" | "html" | "none";

export class PreviewState {
  /** Zoom, page, find, outline and reading options of the preview's viewer. */
  readonly view = new PdfView();
  /** The latest PDF, raw. */
  pdf = $state.raw<Uint8Array | null>(null);
  /** The compiled file the shown PDF belongs to. */
  shownTarget = $state("");
  error = $state("");
  rendering = $state(false);
  exporting = $state(false);

  #generation = 0;

  readonly kind: PreviewKind = $derived(
    build.engine === "typst" || build.engine === "latex"
      ? "pdf"
      : build.engine === "markdown"
        ? "html"
        : "none",
  );

  /** 0-based page the reader is on. */
  get page(): number {
    return this.view.page;
  }

  get pageCount(): number {
    return this.view.pageCount > 0 ? this.view.pageCount : build.pages;
  }

  get mark(): PreviewMark | null {
    return this.view.mark;
  }

  /** Fetch the PDF of the latest successful compile. */
  async render(): Promise<void> {
    if (build.generation === 0 || build.status === "idle") return;
    const run = ++this.#generation;
    if (this.kind !== "pdf") {
      this.error = "";
      return;
    }
    this.rendering = true;
    try {
      const root = workspace.project?.root ?? "";
      const bytes =
        build.engine === "latex"
          ? await latex.exportPdf(join(root, build.target))
          : await typst.exportPdf();
      if (run !== this.#generation) return;
      if (sniffArtifact(bytes)?.kind !== "pdf") {
        this.error = "The compiler returned a file that is not a PDF.";
        return;
      }
      if (this.shownTarget !== build.target) this.view.reset();
      this.pdf = bytes;
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
    this.view.go(page);
  }

  /** Go to the mark's page and outline the box there for a moment. */
  showMark(box: Omit<PreviewMark, "id">): void {
    this.view.showMark(box);
  }

  /** Width in points of a 0-based page, when the viewer has read it. */
  pageWidth(page: number): number | null {
    return this.view.pageSizes[page]?.w ?? null;
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
    this.pdf = null;
    this.view.pageCount = 0;
    this.view.pageSizes = [];
    this.view.reset();
  }
}

export const preview = new PreviewState();
