import { describe, expect, it } from "vitest";

import type { WireDiagnostic } from "$lib/kernel/types";

import {
  countLabel,
  countProblems,
  fromWire,
  groupByFile,
  problemsIn,
  projectPath,
  toGutter,
} from "./problems";

const wire = (over: Partial<WireDiagnostic>): WireDiagnostic => ({
  severity: "error",
  message: "boom",
  file: "/main.typ",
  line: 1,
  column: 1,
  range: null,
  hints: [],
  trace: [],
  ...over,
});

describe("problems model", () => {
  it("strips the virtual root and ignores package files", () => {
    expect(projectPath("/sections/a.typ")).toBe("sections/a.typ");
    expect(projectPath("@preview/touying:0.5.2/lib.typ")).toBe("");
    expect(projectPath(null)).toBe("");
  });

  it("sorts errors before warnings, then by file and line", () => {
    const problems = fromWire([
      wire({ severity: "warning", line: 1, message: "w" }),
      wire({ line: 9, message: "late" }),
      wire({ line: 2, message: "early" }),
      wire({ file: "/a.typ", line: 5, message: "other file" }),
    ]);
    expect(problems.map((p) => p.message)).toEqual(["other file", "early", "late", "w"]);
  });

  it("counts and phrases them in plain language", () => {
    const problems = fromWire([wire({}), wire({ severity: "warning" }), wire({ severity: "warning" })]);
    const counts = countProblems(problems);
    expect(counts).toEqual({ errors: 1, warnings: 2 });
    expect(countLabel(counts)).toBe("1 error, 2 warnings");
    expect(countLabel({ errors: 2, warnings: 0 })).toBe("2 errors");
    expect(countLabel({ errors: 0, warnings: 0 })).toBe("No problems");
  });

  it("groups by file and filters by file", () => {
    const problems = fromWire([wire({ file: "/b.typ" }), wire({ file: "/a.typ" }), wire({ file: "/a.typ", line: 4 })]);
    expect(groupByFile(problems).map((g) => [g.file, g.items.length])).toEqual([
      ["a.typ", 2],
      ["b.typ", 1],
    ]);
    expect(problemsIn(problems, "a.typ")).toHaveLength(2);
  });

  it("places problems in the editor by line and column, skipping unpositioned ones", () => {
    const text = "= Title\nsome $x + y\n";
    const gutter = toGutter(
      text,
      fromWire([wire({ line: 2, column: 6 }), wire({ line: null, column: null })]),
    );
    expect(gutter).toHaveLength(1);
    expect(text.slice(gutter[0]?.from, gutter[0]?.to)).toBe("$x");
    expect(gutter[0]?.severity).toBe("error");
  });
});
