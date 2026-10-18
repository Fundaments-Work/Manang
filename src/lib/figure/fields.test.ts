import { describe, expect, it } from "vitest";

import {
  formatField,
  keepAspect,
  mergeValues,
  normaliseAngle,
  parseField,
  scrubValue,
  stepValue,
  tidy,
} from "./fields";

describe("parseField", () => {
  it("reads numbers with or without a unit", () => {
    expect(parseField("24")).toBe(24);
    expect(parseField(" 24px ")).toBe(24);
    expect(parseField("45°")).toBe(45);
    expect(parseField("80 %")).toBe(80);
    expect(parseField("1,5")).toBe(1.5);
    expect(parseField("-3.25")).toBe(-3.25);
  });

  it("evaluates short sums with the usual precedence", () => {
    expect(parseField("120+16")).toBe(136);
    expect(parseField("100/4")).toBe(25);
    expect(parseField("2+3*4")).toBe(14);
    expect(parseField("10-3-2")).toBe(5);
    expect(parseField("-5+2")).toBe(-3);
  });

  it("refuses what is not a number", () => {
    for (const bad of ["", "abc", "1/0", "3+", "*2", "1 2", "NaN", "Infinity"]) {
      expect(parseField(bad), bad).toBeUndefined();
    }
  });

  it("clamps to the bounds", () => {
    expect(parseField("500", { min: 0, max: 100 })).toBe(100);
    expect(parseField("-4", { min: 0 })).toBe(0);
  });
});

describe("formatField", () => {
  it("drops trailing zeros and keeps two decimals", () => {
    expect(formatField(12)).toBe("12");
    expect(formatField(12.5)).toBe("12.5");
    expect(formatField(1 / 3)).toBe("0.33");
    expect(formatField(Number.NaN)).toBe("");
  });
});

describe("stepValue", () => {
  it("steps by one, or by ten with Shift", () => {
    expect(stepValue(10, 1, 1, false)).toBe(11);
    expect(stepValue(10, -1, 1, true)).toBe(0);
    expect(stepValue(1.5, 1, 0.5, false)).toBe(2);
    expect(stepValue(0.1, 1, 0.1, false)).toBe(0.2);
  });

  it("stays inside the bounds", () => {
    expect(stepValue(100, 1, 1, true, { max: 100 })).toBe(100);
    expect(stepValue(2, -1, 1, true, { min: 1 })).toBe(1);
  });
});

describe("scrubValue", () => {
  it("moves one step per four pixels, rightwards raising it", () => {
    expect(scrubValue(10, 0, 1, false)).toBe(10);
    expect(scrubValue(10, 3, 1, false)).toBe(10);
    expect(scrubValue(10, 8, 1, false)).toBe(12);
    expect(scrubValue(10, -8, 1, false)).toBe(8);
  });

  it("multiplies by ten with Shift and respects the step and bounds", () => {
    expect(scrubValue(10, 8, 1, true)).toBe(30);
    expect(scrubValue(1, 8, 0.5, false)).toBe(2);
    expect(scrubValue(50, 400, 1, false, { max: 100 })).toBe(100);
    expect(scrubValue(5, -400, 1, false, { min: 0 })).toBe(0);
  });
});

describe("mergeValues", () => {
  it("is one value when all agree", () => {
    expect(mergeValues([2, 2, 2])).toEqual({ value: 2, mixed: false });
    expect(mergeValues(["ink", "ink"])).toEqual({ value: "ink", mixed: false });
    expect(mergeValues([undefined, undefined])).toEqual({
      value: undefined,
      mixed: false,
    });
  });

  it("is mixed when they differ, an unset one included", () => {
    expect(mergeValues([1, 2]).mixed).toBe(true);
    expect(mergeValues(["ink", undefined]).mixed).toBe(true);
    expect(mergeValues(["a", "b", "a"]).mixed).toBe(true);
  });

  it("ignores float noise in numbers", () => {
    expect(mergeValues([0.1 + 0.2, 0.3]).mixed).toBe(false);
  });

  it("an empty selection holds nothing", () => {
    expect(mergeValues([])).toEqual({ value: undefined, mixed: false });
  });
});

describe("keepAspect", () => {
  it("scales the other side by the box's ratio", () => {
    expect(keepAspect("width", 200, 100, 50)).toEqual({ width: 200, height: 100 });
    expect(keepAspect("height", 25, 100, 50)).toEqual({ width: 50, height: 25 });
  });

  it("leaves a degenerate box alone", () => {
    expect(keepAspect("width", 10, 0, 50)).toEqual({ width: 10, height: 50 });
  });
});

describe("angles and noise", () => {
  it("normalises to 0 up to 360", () => {
    expect(normaliseAngle(-90)).toBe(270);
    expect(normaliseAngle(450)).toBe(90);
    expect(normaliseAngle(0)).toBe(0);
  });

  it("tidy removes float noise and negative zero", () => {
    expect(tidy(0.1 + 0.2)).toBe(0.3);
    expect(Object.is(tidy(-0.00001), 0)).toBe(true);
  });
});
