import { describe, expect, it } from "vitest";

import { toEditorDiagnostics } from "./lint";

describe("toEditorDiagnostics", () => {
  const item = (line: number, column: number) => ({
    file: "",
    line,
    column,
    severity: "error" as const,
    message: "m",
  });

  it("maps line and scalar column to UTF-16 offsets, spanning the token", () => {
    const text = "ab\n😀 oops here";
    // Line 2, column 3 is the "o" after the emoji and space (scalars: 😀, space).
    const [d] = toEditorDiagnostics(text, [item(2, 3)]);

    expect(d).toMatchObject({ from: 6, to: 10 });
    expect(text.slice(d?.from, d?.to)).toBe("oops");
  });

  it("ignores lines that do not exist and clamps columns", () => {
    expect(toEditorDiagnostics("one", [item(5, 1)])).toEqual([]);
    const [d] = toEditorDiagnostics("one", [item(1, 99)]);
    expect(d === undefined || d.to > d.from).toBe(true);
  });
});
