import { describe, expect, it } from "vitest";

import { MAX_CANVAS_PIXELS, outputRatio } from "./pdfBook";

describe("canvas resolution", () => {
  it("uses the screen's pixel ratio for ordinary pages", () => {
    expect(outputRatio(794, 1123, 2)).toBe(2);
    expect(outputRatio(794, 1123, 1)).toBe(1);
    expect(outputRatio(794, 1123, 1.25)).toBe(1.25);
  });

  it("caps the pixel count of very large pages", () => {
    // An A4 page at 400% on a 2x screen.
    const w = 3175;
    const h = 4490;
    const ratio = outputRatio(w, h, 2);
    expect(ratio).toBeLessThan(2);
    expect(w * ratio * (h * ratio)).toBeLessThanOrEqual(MAX_CANVAS_PIXELS + 1);
  });

  it("treats a missing ratio as 1", () => {
    expect(outputRatio(100, 100, Number.NaN)).toBe(1);
    expect(outputRatio(100, 100, 0)).toBe(1);
  });
});
