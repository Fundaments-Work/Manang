import { describe, expect, it } from "vitest";

import {
  CSS_PER_POINT,
  MAX_ZOOM,
  MIN_ZOOM,
  ZOOM_MENU,
  ZOOM_PRESETS,
  clampZoom,
  isCurrentMode,
  scaleFor,
  stepZoom,
  wheelZoom,
  zoomKey,
  zoomLabel,
  zoomOf,
  zoomPercent,
} from "./zoom";

const A4 = { w: 595.28, h: 841.89 };
const PANE = { width: 700, height: 600 };

describe("zoom presets", () => {
  it("offers Overleaf's presets: fit width, fit page, 50 to 400%", () => {
    expect(ZOOM_MENU.map((e) => e.label)).toEqual([
      "Fit width",
      "Fit page",
      "50%",
      "75%",
      "100%",
      "125%",
      "150%",
      "200%",
      "300%",
      "400%",
    ]);
    expect(ZOOM_PRESETS[0]).toBe(0.5);
    expect(ZOOM_PRESETS.at(-1)).toBe(MAX_ZOOM);
  });

  it("100% is the printed size on a 96 dpi screen", () => {
    expect(scaleFor(1, PANE, A4)).toBeCloseTo(4 / 3);
    expect(Math.round(A4.w * scaleFor(1, PANE, A4))).toBe(794);
    expect(zoomPercent(CSS_PER_POINT * 1.5)).toBe("150%");
    expect(zoomOf(CSS_PER_POINT * 2)).toBeCloseTo(2);
  });

  it("fit width fills the pane less the side margins", () => {
    const scale = scaleFor("page-width", PANE, A4);
    expect(A4.w * scale).toBeCloseTo(700 - 48);
  });

  it("fit page shows a whole page, limited by the height of a wide pane", () => {
    const scale = scaleFor("page-fit", PANE, A4);
    expect(A4.h * scale).toBeCloseTo(600 - 32);
    expect(A4.w * scale).toBeLessThan(700 - 48);
    // A tall narrow pane: the width decides.
    const narrow = scaleFor("page-fit", { width: 300, height: 2000 }, A4);
    expect(A4.w * narrow).toBeCloseTo(300 - 48);
  });

  it("numbers are clamped to the range", () => {
    expect(clampZoom(10)).toBe(MAX_ZOOM);
    expect(clampZoom(0.01)).toBe(MIN_ZOOM);
    expect(clampZoom(Number.NaN)).toBe(1);
    expect(scaleFor(9, PANE, A4)).toBeCloseTo(MAX_ZOOM * CSS_PER_POINT);
  });

  it("labels say the mode and the percentage", () => {
    const scale = scaleFor("page-width", PANE, A4);
    expect(zoomLabel("page-width", scale)).toBe(`Fit width, ${zoomPercent(scale)}`);
    expect(zoomLabel("page-fit", CSS_PER_POINT)).toBe("Fit page, 100%");
    expect(zoomLabel(1.25, CSS_PER_POINT * 1.25)).toBe("125%");
  });

  it("marks the current entry of the menu", () => {
    expect(isCurrentMode("page-width", "page-width")).toBe(true);
    expect(isCurrentMode(1.25, 1.2501)).toBe(true);
    expect(isCurrentMode(1.25, 1.5)).toBe(false);
    expect(isCurrentMode(1, "page-fit")).toBe(false);
  });
});

describe("zoom steps", () => {
  it("steps to the next preset from any scale, including a fit", () => {
    expect(stepZoom(CSS_PER_POINT, 1)).toBe(1.25);
    expect(stepZoom(CSS_PER_POINT, -1)).toBe(0.75);
    // 87% (a fit width) steps to 100% and to 75%.
    expect(stepZoom(CSS_PER_POINT * 0.87, 1)).toBe(1);
    expect(stepZoom(CSS_PER_POINT * 0.87, -1)).toBe(0.75);
  });

  it("stays inside the range at both ends", () => {
    expect(stepZoom(CSS_PER_POINT * 4, 1)).toBe(MAX_ZOOM);
    expect(stepZoom(CSS_PER_POINT * 0.5, -1)).toBe(MIN_ZOOM);
    expect(stepZoom(CSS_PER_POINT * 0.25, -1)).toBe(MIN_ZOOM);
  });

  it("a wheel notch zooms about 20%, a pinch a little, and both are clamped", () => {
    const up = wheelZoom(CSS_PER_POINT, -100);
    expect(up).toBeGreaterThan(1.15);
    expect(up).toBeLessThan(1.25);
    expect(wheelZoom(CSS_PER_POINT, 100)).toBeLessThan(0.9);
    const pinch = wheelZoom(CSS_PER_POINT, -4);
    expect(pinch).toBeGreaterThan(1);
    expect(pinch).toBeLessThan(1.02);
    // Line mode counts each line as 16 px.
    expect(wheelZoom(CSS_PER_POINT, -3, 1)).toBeCloseTo(wheelZoom(CSS_PER_POINT, -48));
    expect(wheelZoom(CSS_PER_POINT * 4, -1000)).toBe(MAX_ZOOM);
    expect(wheelZoom(CSS_PER_POINT * 0.25, 1000)).toBe(MIN_ZOOM);
  });

  it("zoom-in and back out at the same spot returns to the start", () => {
    const zoomed = wheelZoom(CSS_PER_POINT, -100) * CSS_PER_POINT;
    expect(wheelZoom(zoomed, 100)).toBeCloseTo(1);
  });
});

describe("zoom keys", () => {
  const key = (
    k: string,
    mods: Partial<Record<"ctrlKey" | "metaKey" | "altKey", boolean>> = {},
  ) => zoomKey({ key: k, ctrlKey: false, metaKey: false, altKey: false, ...mods });

  it("Ctrl or Cmd with = + - 0", () => {
    expect(key("=", { ctrlKey: true })).toBe("in");
    expect(key("+", { metaKey: true })).toBe("in");
    expect(key("-", { ctrlKey: true })).toBe("out");
    expect(key("0", { ctrlKey: true })).toBe("fit");
  });

  it("ignores plain keys and Alt combinations", () => {
    expect(key("=")).toBeNull();
    expect(key("0", { ctrlKey: true, altKey: true })).toBeNull();
    expect(key("f", { ctrlKey: true })).toBeNull();
  });
});
