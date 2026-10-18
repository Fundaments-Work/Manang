import { describe, expect, it } from "vitest";

import { mockDiffText, mockHash } from "./mockDiff";

describe("mockDiffText", () => {
  it("shows a changed line as a remove and an add with context", () => {
    const d = mockDiffText("a.typ", "one\ntwo\nthree\n", "one\nTWO\nthree\n");
    expect([d.additions, d.deletions]).toEqual([1, 1]);
    expect(d.hunks).toHaveLength(1);
    expect(d.hunks[0]?.lines.map((l) => l.kind)).toEqual([
      "context",
      "remove",
      "add",
      "context",
    ]);
    expect(d.hunks[0]?.header).toBe("@@ -1,3 +1,3 @@");
  });

  it("has no hunks for identical text and calls a missing file added", () => {
    expect(mockDiffText("a", "same\n", "same\n").hunks).toEqual([]);
    const added = mockDiffText("a", null, "x\ny\n");
    expect(added.status).toBe("added");
    expect(added.additions).toBe(2);
  });

  it("splits distant changes into separate hunks", () => {
    const old = Array.from({ length: 30 }, (_, i) => `line ${i}`).join("\n");
    const next = old.replace("line 2", "LINE 2").replace("line 27", "LINE 27");
    expect(mockDiffText("a", old, next).hunks).toHaveLength(2);
  });

  it("refuses huge inputs", () => {
    const big = Array.from({ length: 2000 }, (_, i) => `l${i}`).join("\n");
    expect(mockDiffText("a", big, "x").too_large).toBe(true);
  });
});

describe("mockHash", () => {
  it("is stable and sensitive to content", () => {
    expect(mockHash("abc")).toBe(mockHash("abc"));
    expect(mockHash("abc")).not.toBe(mockHash("abd"));
    expect(mockHash("")).toMatch(/^b3:/);
  });
});
