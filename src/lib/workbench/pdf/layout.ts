/**
 * Where every page of the PDF viewer sits, in CSS px inside the scroll area:
 * a centred column of pages, `gap` apart, each as large as its own size times
 * the scale. Pure, so zoom anchoring, page tracking and scroll restoring are
 * testable without a browser.
 */

import { MARGINS, type Margins, type PageSize } from "./zoom";

export interface PageLayout {
  /** CSS px per point. */
  readonly scale: number;
  readonly tops: readonly number[];
  readonly lefts: readonly number[];
  readonly widths: readonly number[];
  readonly heights: readonly number[];
  readonly contentWidth: number;
  readonly contentHeight: number;
}

export interface Point {
  readonly x: number;
  readonly y: number;
}

/** Slot size of a page: whole pixels, rounded down like pdf.js' layers. */
export function slotSize(page: PageSize, scale: number): { w: number; h: number } {
  return { w: Math.floor(page.w * scale), h: Math.floor(page.h * scale) };
}

export function layoutPages(
  sizes: readonly PageSize[],
  scale: number,
  paneWidth: number,
  margins: Margins = MARGINS,
): PageLayout {
  const widths: number[] = [];
  const heights: number[] = [];
  for (const size of sizes) {
    const slot = slotSize(size, scale);
    widths.push(slot.w);
    heights.push(slot.h);
  }
  const column = Math.max(0, ...widths) + margins.side * 2;
  const contentWidth = Math.max(paneWidth, column);
  const tops: number[] = [];
  let y = margins.gap;
  for (const h of heights) {
    tops.push(y);
    y += h + margins.gap;
  }
  return {
    scale,
    tops,
    lefts: widths.map((w) => (contentWidth - w) / 2),
    widths,
    heights,
    contentWidth,
    contentHeight: sizes.length === 0 ? 0 : y,
  };
}

/** The page at content height `y`: half of each gap belongs to either side. */
export function pageAt(layout: PageLayout, y: number): number {
  const { tops, heights } = layout;
  if (tops.length === 0) return 0;
  let lo = 0;
  let hi = tops.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    const prevBottom = (tops[mid - 1] ?? 0) + (heights[mid - 1] ?? 0);
    const boundary = (prevBottom + (tops[mid] ?? 0)) / 2;
    if (y >= boundary) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

/** The reader's page: the one under the upper third of the pane. */
export function currentPage(
  layout: PageLayout,
  scrollTop: number,
  clientHeight: number,
): number {
  return pageAt(layout, scrollTop + clientHeight / 3);
}

/** A point on a page, in points from its top-left corner. */
export interface PagePoint {
  readonly page: number;
  readonly x: number;
  readonly y: number;
}

/** The page point under content coordinates `at` (may lie outside the page box). */
export function pointAt(layout: PageLayout, at: Point): PagePoint {
  const page = pageAt(layout, at.y);
  const scale = layout.scale || 1;
  return {
    page,
    x: (at.x - (layout.lefts[page] ?? 0)) / scale,
    y: (at.y - (layout.tops[page] ?? 0)) / scale,
  };
}

/** Content coordinates of a page point. */
export function contentAt(layout: PageLayout, point: PagePoint): Point {
  return {
    x: (layout.lefts[point.page] ?? 0) + point.x * layout.scale,
    y: (layout.tops[point.page] ?? 0) + point.y * layout.scale,
  };
}

/**
 * Scroll offsets that keep the document point under `pointer` (relative to
 * the scroll area's top-left corner) in place when the layout changes from
 * `before` to `after`: zooming at the pointer.
 */
export function anchorScroll(
  before: PageLayout,
  after: PageLayout,
  pointer: Point,
  scroll: { readonly left: number; readonly top: number },
): { left: number; top: number } {
  const point = pointAt(before, {
    x: scroll.left + pointer.x,
    y: scroll.top + pointer.y,
  });
  const next = contentAt(after, point);
  return {
    left: Math.max(0, next.x - pointer.x),
    top: Math.max(0, next.y - pointer.y),
  };
}

/** Where the reader is: the page point at the top edge of the pane. */
export function readingPosition(
  layout: PageLayout,
  scrollTop: number,
): { page: number; y: number } {
  const point = pointAt(layout, { x: 0, y: scrollTop });
  return { page: point.page, y: point.y };
}

/** The scrollTop that puts a reading position back at the top edge. */
export function scrollForPosition(
  layout: PageLayout,
  position: { readonly page: number; readonly y: number },
): number {
  const page = Math.max(0, Math.min(position.page, layout.tops.length - 1));
  return Math.max(0, contentAt(layout, { page, x: 0, y: position.y }).y);
}

/**
 * The scrollTop that shows `y` points down `page` a third of the way down the
 * pane (the top of the page with a small margin when `y` is null).
 */
export function scrollToPoint(
  layout: PageLayout,
  page: number,
  y: number | null,
  clientHeight: number,
  margin: number = MARGINS.gap,
): number {
  const at = Math.max(0, Math.min(page, layout.tops.length - 1));
  const top = layout.tops[at] ?? 0;
  if (y === null) return Math.max(0, top - margin);
  return Math.max(0, top + y * layout.scale - clientHeight / 3);
}
