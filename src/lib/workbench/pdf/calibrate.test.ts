import { describe, expect, it } from "vitest";

import { corrections, scaledX, type WidthSample } from "./calibrate";

function samples(font: string, ratio: number, count: number): WidthSample[] {
  return Array.from({ length: count }, (_, i) => ({
    font,
    expected: 100 + i,
    actual: (100 + i) / ratio,
  }));
}

describe("text-layer width calibration", () => {
  it("leaves a layer alone when spans match the PDF", () => {
    const fix = corrections(samples("serif", 1.005, 40));
    expect(fix.global).toBeNull();
    expect(fix.byFont.size).toBe(0);
  });

  it("corrects each font by its median ratio", () => {
    const fix = corrections([
      ...samples("serif", 1.06, 30),
      ...samples("mono", 0.95, 10),
    ]);
    expect(fix.byFont.get("serif")).toBeCloseTo(1.06);
    expect(fix.byFont.get("mono")).toBeCloseTo(0.95);
  });

  it("needs enough samples, ignores absurd ones and clamps the correction", () => {
    expect(corrections(samples("serif", 1.1, 10)).byFont.size).toBe(0);
    const noisy = [
      ...samples("serif", 1.1, 30),
      { font: "serif", expected: 100, actual: 10 },
    ];
    expect(corrections(noisy).byFont.get("serif")).toBeCloseTo(1.1);
    expect(corrections(samples("serif", 1.4, 30)).byFont.get("serif")).toBe(1.25);
  });

  it("a font with few samples falls back to the overall correction", () => {
    const fix = corrections([
      ...samples("serif", 1.05, 30),
      ...samples("rare", 1.05, 3),
    ]);
    expect(fix.byFont.has("rare")).toBe(false);
    expect(fix.global).toBeCloseTo(1.05);
  });

  it("multiplies the existing scale", () => {
    expect(scaledX("0.9", 1.1)).toBe(String(0.9 * 1.1));
    expect(scaledX("", 1.1)).toBe("1.1");
  });
});
