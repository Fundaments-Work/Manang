import { describe, expect, it } from "vitest";

import { diffRange, scalarLength, scalarToUtf16, utf16ToScalar } from "./offsets";

const EMOJI = "😀"; // 2 UTF-16 units, 1 scalar
const COMBINED = "é"; // e + combining acute: 2 units, 2 scalars

describe("offset mapping", () => {
  it("counts scalars, not UTF-16 units", () => {
    expect(EMOJI.length).toBe(2);
    expect(scalarLength(EMOJI)).toBe(1);
    expect(scalarLength("a😀b")).toBe(3);
    expect(scalarLength("👨‍👩‍👧")).toBe(5); // ZWJ sequence: three emoji + two joiners
  });

  it("treats combining marks as their own scalar", () => {
    expect(scalarLength(COMBINED)).toBe(2);
    expect(utf16ToScalar(COMBINED, 1)).toBe(1);
  });

  it("maps UTF-16 offsets to scalar indices past astral characters", () => {
    const text = `${EMOJI}x${EMOJI}y`;
    expect(utf16ToScalar(text, 0)).toBe(0);
    expect(utf16ToScalar(text, 2)).toBe(1);
    expect(utf16ToScalar(text, 3)).toBe(2);
    expect(utf16ToScalar(text, 5)).toBe(3);
    expect(utf16ToScalar(text, 999)).toBe(4);
  });

  it("round-trips at every scalar boundary", () => {
    const text = `a${EMOJI}${COMBINED}𝒳z`;
    for (let s = 0; s <= scalarLength(text); s += 1) {
      expect(utf16ToScalar(text, scalarToUtf16(text, s))).toBe(s);
    }
    expect(scalarToUtf16(text, 99)).toBe(text.length);
  });

  it("accepts a CodeMirror-style sliceable", () => {
    const text = `${EMOJI}b`;
    const doc = {
      length: text.length,
      sliceString: (f: number, t?: number) => text.slice(f, t),
    };
    expect(utf16ToScalar(doc, 3)).toBe(2);
  });
});

describe("diffRange", () => {
  it("finds a minimal replacement", () => {
    expect(diffRange("hello world", "hello brave world")).toEqual({
      from: 6,
      to: 6,
      insert: "brave ",
    });
    expect(diffRange("abc", "abc")).toEqual({ from: 3, to: 3, insert: "" });
    expect(diffRange("abc", "")).toEqual({ from: 0, to: 3, insert: "" });
  });

  it("never splits a surrogate pair", () => {
    // 😀 and 😁 share the high surrogate.
    const { from, to, insert } = diffRange("a😀b", "a😁b");
    expect([from, to, insert]).toEqual([1, 3, "😁"]);
    const again = diffRange("😀", "😁");
    expect([again.from, again.to, again.insert]).toEqual([0, 2, "😁"]);
  });
});
