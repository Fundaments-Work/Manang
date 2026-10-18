import { describe, expect, it } from "vitest";

import {
  anchorScroll,
  contentAt,
  currentPage,
  layoutPages,
  pageAt,
  pointAt,
  readingPosition,
  scrollForPosition,
  scrollToPoint,
} from "./layout";

const A4 = { w: 595, h: 842 };
const LANDSCAPE = { w: 842, h: 595 };

describe("page layout", () => {
  it("stacks pages with gaps and centres the column", () => {
    const layout = layoutPages([A4, A4, A4], 1, 1000);
    expect(layout.tops).toEqual([16, 16 + 842 + 16, 16 + 2 * (842 + 16)]);
    expect(layout.lefts[0]).toBe((1000 - 595) / 2);
    expect(layout.contentWidth).toBe(1000);
    expect(layout.contentHeight).toBe(16 + 3 * (842 + 16));
  });

  it("is wider than the pane when the pages are, with the side margins", () => {
    const layout = layoutPages([A4], 2, 600);
    expect(layout.widths[0]).toBe(1190);
    expect(layout.contentWidth).toBe(1190 + 48);
    expect(layout.lefts[0]).toBe(24);
  });

  it("keeps each page's own size and centres it", () => {
    const layout = layoutPages([A4, LANDSCAPE], 1, 900);
    expect(layout.heights).toEqual([842, 595]);
    expect(layout.lefts[1]).toBe((900 - 842) / 2);
    expect(layout.lefts[0]).toBe((900 - 595) / 2);
  });

  it("rounds slots down to whole pixels", () => {
    const layout = layoutPages([{ w: 595.28, h: 841.89 }], 1.5, 800);
    expect(layout.widths[0]).toBe(892);
    expect(layout.heights[0]).toBe(1262);
  });

  it("an empty document has no height", () => {
    expect(layoutPages([], 1, 500).contentHeight).toBe(0);
    expect(pageAt(layoutPages([], 1, 500), 100)).toBe(0);
  });
});

describe("which page", () => {
  const layout = layoutPages([A4, A4, A4, A4], 1, 800);

  it("splits each gap between the pages around it", () => {
    expect(pageAt(layout, 0)).toBe(0);
    expect(pageAt(layout, 16 + 842 + 7)).toBe(0);
    expect(pageAt(layout, 16 + 842 + 9)).toBe(1);
    expect(pageAt(layout, 1e9)).toBe(3);
    expect(pageAt(layout, -50)).toBe(0);
  });

  it("the current page is under the upper third of the pane", () => {
    expect(currentPage(layout, 0, 900)).toBe(0);
    expect(currentPage(layout, 858 - 300 + 20, 900)).toBe(1);
  });
});

describe("points and positions", () => {
  const layout = layoutPages([A4, A4], 1.5, 1200);

  it("maps a page point to content and back", () => {
    const at = contentAt(layout, { page: 1, x: 100, y: 200 });
    expect(pointAt(layout, at)).toEqual({ page: 1, x: 100, y: 200 });
  });

  it("restores the reading position at another zoom", () => {
    const position = readingPosition(layout, 1500);
    const zoomed = layoutPages([A4, A4], 3, 1200);
    const top = scrollForPosition(zoomed, position);
    expect(readingPosition(zoomed, top).page).toBe(position.page);
    expect(readingPosition(zoomed, top).y).toBeCloseTo(position.y);
  });

  it("clamps a stored page beyond the document", () => {
    expect(scrollForPosition(layout, { page: 9, y: 0 })).toBe(layout.tops[1]);
  });

  it("scrolls a point to the upper third, or a page to its top", () => {
    expect(scrollToPoint(layout, 1, null, 600)).toBe((layout.tops[1] ?? 0) - 16);
    expect(scrollToPoint(layout, 1, 100, 600)).toBe((layout.tops[1] ?? 0) + 150 - 200);
    expect(scrollToPoint(layout, 0, 0, 600)).toBe(0);
  });
});

describe("zoom at the pointer", () => {
  it("keeps the document point under the pointer where it was", () => {
    const sizes = [A4, A4, A4];
    const before = layoutPages(sizes, 1, 900);
    const after = layoutPages(sizes, 2, 900);
    const pointer = { x: 420, y: 300 };
    const scroll = { left: 0, top: 1000 };
    const under = pointAt(before, {
      x: scroll.left + pointer.x,
      y: scroll.top + pointer.y,
    });
    const next = anchorScroll(before, after, pointer, scroll);
    const now = pointAt(after, { x: next.left + pointer.x, y: next.top + pointer.y });
    expect(now.page).toBe(under.page);
    expect(now.x).toBeCloseTo(under.x, 6);
    expect(now.y).toBeCloseTo(under.y, 6);
  });

  it("works when zooming out, and never scrolls before the start", () => {
    const before = layoutPages([A4], 2, 900);
    const after = layoutPages([A4], 0.5, 900);
    const next = anchorScroll(before, after, { x: 10, y: 10 }, { left: 0, top: 0 });
    expect(next.left).toBeGreaterThanOrEqual(0);
    expect(next.top).toBeGreaterThanOrEqual(0);
  });

  it("round trips: zooming in then out at the same pointer returns to the start", () => {
    const sizes = [A4, LANDSCAPE, A4];
    const a = layoutPages(sizes, 1, 1000);
    const b = layoutPages(sizes, 1.7, 1000);
    const pointer = { x: 500, y: 250 };
    const start = { left: 0, top: 700 };
    const zoomed = anchorScroll(a, b, pointer, start);
    const back = anchorScroll(b, a, pointer, zoomed);
    expect(back.left).toBeCloseTo(start.left, 6);
    expect(back.top).toBeCloseTo(start.top, 6);
  });
});
