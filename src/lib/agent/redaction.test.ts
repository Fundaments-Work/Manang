import { describe, expect, it } from "vitest";

import { byteToIndex, redactionSegments } from "./redaction";

describe("redaction diff", () => {
  it("maps UTF-8 byte offsets to string indexes", () => {
    expect(byteToIndex("aé😀b", 0)).toBe(0);
    expect(byteToIndex("aé😀b", 3)).toBe(2);
    expect(byteToIndex("aé😀b", 7)).toBe(4);
    expect(byteToIndex("abc", 99)).toBe(3);
  });

  it("marks removed spans and their replacements", () => {
    const original = "key = é sk-abc end";
    const start = new TextEncoder().encode("key = é ").length;
    const segments = redactionSegments(original, [
      { kind: "secret", start, end: start + 6, replacement: "[REDACTED:secret]" },
    ]);
    expect(segments).toEqual([
      { kind: "text", text: "key = é " },
      { kind: "removed", text: "sk-abc" },
      { kind: "replacement", text: "[REDACTED:secret]" },
      { kind: "text", text: " end" },
    ]);
  });
});
