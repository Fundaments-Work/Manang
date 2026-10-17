import { describe, expect, it } from "vitest";

import { scalarLength } from "$lib/kernel/offsets";

import { ScalarIndex } from "./offsets";

describe("ScalarIndex", () => {
  it("is the identity for text without astral characters", () => {
    const index = new ScalarIndex("héllo wörld");
    expect([0, 1, 5, 11].map((n) => index.toUtf16(n))).toEqual([0, 1, 5, 11]);
  });

  it("counts an emoji as one scalar but two UTF-16 units", () => {
    const text = "a😀b teh";
    const index = new ScalarIndex(text);
    expect(scalarLength(text)).toBe(7);
    // scalar 2 is "b"; in UTF-16 it is offset 3
    expect(index.toUtf16(2)).toBe(3);
    expect(index.toUtf16(4)).toBe(5);
    expect(text.slice(index.toUtf16(4), index.toUtf16(7))).toBe("teh");
  });

  it("treats combining marks as separate BMP scalars on both sides", () => {
    const text = "café teh"; // e + combining acute
    const index = new ScalarIndex(text);
    expect(scalarLength(text)).toBe(9);
    expect(text.slice(index.toUtf16(6), index.toUtf16(9))).toBe("teh");
  });

  it("handles runs of emoji and ZWJ sequences", () => {
    const text = "👨‍👩‍👧 x 𝔸𝔹 teh";
    const index = new ScalarIndex(text);
    const start = scalarLength("👨‍👩‍👧 x 𝔸𝔹 ");
    expect(text.slice(index.toUtf16(start), index.toUtf16(start + 3))).toBe("teh");
  });

  it("clamps indices past the end", () => {
    const text = "😀😀";
    const index = new ScalarIndex(text);
    expect(index.toUtf16(99)).toBe(text.length);
    expect(index.toUtf16(-1)).toBe(0);
    expect(new ScalarIndex("ab").toUtf16(99)).toBe(2);
  });

  it("keeps a lone surrogate as one scalar", () => {
    const text = "a\ud83db";
    const index = new ScalarIndex(text);
    expect(index.toUtf16(2)).toBe(2);
    expect(index.toUtf16(3)).toBe(3);
  });
});
