import { describe, expect, it } from "vitest";

import { createMockKernel } from "./mock";
import { fetchLint, parseLint, toEditorDiagnostics } from "./lint";

describe("parseLint", () => {
  it("keeps well-formed items and drops malformed ones", () => {
    const items = parseLint([
      { file: "a.typ", line: 2, column: 3, severity: "error", message: "bad" },
      { line: 1, column: 1, severity: "note", message: "odd severity" },
      { line: 0, column: 1, message: "zero line" },
      { line: 1, message: "no column" },
      null,
      "x",
    ]);

    expect(items).toEqual([
      { file: "a.typ", line: 2, column: 3, severity: "error", message: "bad" },
      { file: "", line: 1, column: 1, severity: "warning", message: "odd severity" },
    ]);
    expect(parseLint({ not: "an array" })).toEqual([]);
  });
});

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

describe("fetchLint", () => {
  it("degrades silently when the command is absent", async () => {
    await expect(fetchLint(createMockKernel())).resolves.toEqual([]);
  });

  it("returns parsed items when a plugin provides the command", async () => {
    const k = createMockKernel({
      handlers: {
        "lint.diagnostics": () => [
          { file: "a", line: 1, column: 1, severity: "error", message: "x" },
        ],
      },
    });
    await expect(fetchLint(k, "a")).resolves.toHaveLength(1);
  });
});
