/**
 * What one PDF viewer shows and what its controls ask of it: zoom, the
 * reader's page, jump requests, find, the outline and the reading options.
 *
 * The preview owns one (`preview.view`) and a .pdf file tab owns another; the
 * header and the viewer both talk to it, so the controls and the pages stay in
 * step without the viewer knowing about the header. The rules (zoom steps,
 * page numbers, matching) are in `workbench/pdf/`.
 */

import type { OutlineEntry } from "$lib/workbench/pdf/links";
import type { PageSize, ZoomMode } from "$lib/workbench/pdf/zoom";
import { CSS_PER_POINT, clampZoom, stepZoom } from "$lib/workbench/pdf/zoom";

/** A spot on the pages to outline briefly: 0-based page, a box in points. */
export interface PreviewMark {
  readonly page: number;
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  /** Distinguishes two marks at the same place, so each one shows. */
  readonly id: number;
}

/** How long a mark stays on the page. */
export const MARK_MS = 1600;

/** Where the reader was in a document: a page and points down it. */
export interface ReadingSpot {
  readonly page: number;
  readonly y: number;
}

export class PdfView {
  /** 0-based; follows the scroll position. */
  page = $state(0);
  pageCount = $state(0);
  /** Size of every page in points, as the viewer read them. */
  pageSizes = $state.raw<readonly PageSize[]>([]);
  zoom = $state<ZoomMode>("page-width");
  /** CSS px per point on screen now (the zoom mode resolved for the pane). */
  scale = $state(CSS_PER_POINT);
  /** Bumped by `go`, so the viewer scrolls only on requests, not on scroll. */
  jump = $state(0);
  /** Points down the page `go` asked for, or null for the top of the page. */
  jumpY = $state<number | null>(null);
  mark = $state.raw<PreviewMark | null>(null);

  /** Find bar. */
  findOpen = $state(false);
  findQuery = $state("");
  findCase = $state(false);
  /** Bumped to move the focus into the find field. */
  findFocus = $state(0);
  /** Bumped with the direction to move between matches. */
  findStep = $state<{ readonly n: number; readonly direction: 1 | -1 }>({
    n: 0,
    direction: 1,
  });
  /** Filled by the viewer: index of the current match (-1 none) and the count. */
  findCurrent = $state(-1);
  findTotal = $state(0);

  /** The document's outline (bookmarks); empty when it has none. */
  outline = $state.raw<readonly OutlineEntry[]>([]);
  outlineOpen = $state(false);

  /** Pages inverted for reading in the dark (CSS filter on the canvas only). */
  darkPages = $state(false);
  /** Double-click goes to the source (Alt+double-click selects a word). */
  doubleClickSource = $state(true);

  #markId = 0;
  #markTimer: ReturnType<typeof setTimeout> | undefined;
  /** Plain on purpose: nothing renders from it. */
  readonly #spots: Record<string, ReadingSpot> = {};

  /** Scroll to a 0-based page, optionally to `y` points down it. */
  go(page: number, y: number | null = null): void {
    const last = Math.max(0, this.pageCount - 1);
    this.page = Math.max(0, Math.min(page, last));
    this.jumpY = y;
    this.jump += 1;
  }

  /** Go to the mark's page and outline the box there for a moment. */
  showMark(box: Omit<PreviewMark, "id">): void {
    this.#markId += 1;
    this.mark = { ...box, id: this.#markId };
    this.go(box.page);
    clearTimeout(this.#markTimer);
    const id = this.#markId;
    this.#markTimer = setTimeout(() => {
      if (this.mark?.id === id) this.mark = null;
    }, MARK_MS);
  }

  setZoom(mode: ZoomMode): void {
    this.zoom = typeof mode === "number" ? clampZoom(mode) : mode;
  }

  zoomIn(): void {
    this.zoom = stepZoom(this.scale, 1);
  }

  zoomOut(): void {
    this.zoom = stepZoom(this.scale, -1);
  }

  fit(): void {
    this.zoom = "page-width";
  }

  openFind(): void {
    this.findOpen = true;
    this.findFocus += 1;
  }

  closeFind(): void {
    this.findOpen = false;
  }

  stepFind(direction: 1 | -1): void {
    this.findStep = { n: this.findStep.n + 1, direction };
  }

  toggleOutline(): void {
    this.outlineOpen = !this.outlineOpen;
  }

  /** Remember where the reader is in `key` (a document), to come back to it. */
  remember(key: string, spot: ReadingSpot): void {
    if (key !== "") this.#spots[key] = spot;
  }

  recall(key: string): ReadingSpot | null {
    return this.#spots[key] ?? null;
  }

  /** A new document: forget what belonged to the old one. */
  reset(): void {
    this.mark = null;
    this.outline = [];
    this.findCurrent = -1;
    this.findTotal = 0;
  }
}
