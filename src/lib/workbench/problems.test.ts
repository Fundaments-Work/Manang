import { Text } from "@codemirror/state";
import { describe, expect, it } from "vitest";

import type { LintFinding } from "$lib/editor/writing/bus";

import type { WireDiagnostic } from "$lib/kernel/types";

import {
  countLabel,
  countProblems,
  fromWire,
  fromWriting,
  sortProblems,
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
    expect(problems.map((p) => p.message)).toEqual([
      "other file",
      "early",
      "late",
      "w",
    ]);
  });

  it("counts and phrases them in plain language", () => {
    const problems = fromWire([
      wire({}),
      wire({ severity: "warning" }),
      wire({ severity: "warning" }),
    ]);
    const counts = countProblems(problems);
    expect(counts).toEqual({ errors: 1, warnings: 2 });
    expect(countLabel(counts)).toBe("1 error, 2 warnings");
    expect(countLabel({ errors: 2, warnings: 0 })).toBe("2 errors");
    expect(countLabel({ errors: 0, warnings: 0 })).toBe("No problems");
  });

  it("groups by file and filters by file", () => {
    const problems = fromWire([
      wire({ file: "/b.typ" }),
      wire({ file: "/a.typ" }),
      wire({ file: "/a.typ", line: 4 }),
    ]);
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

describe("writing findings in Problems", () => {
  const doc = Text.of(["= Title", "😀 this is is odd", "fine"]);
  const finding = (over: Partial<LintFinding>): LintFinding => ({
    from: 0,
    to: 1,
    severity: "warning",
    message: "Repeated word",
    code: "repeated-word",
    hints: [],
    ...over,
  });

  it("places a finding by line and scalar column, tagged with its source", () => {
    const from = doc.line(2).from + 8; // "is is": after "😀 this " (UTF-16 offset)
    const [problem] = fromWriting("a.typ", doc, [finding({ from, to: from + 5 })]);
    expect(problem).toMatchObject({
      file: "a.typ",
      line: 2,
      column: 8, // the emoji is one scalar, so one less than its UTF-16 offset
      source: "writing",
      code: "repeated-word",
      severity: "warning",
    });
  });

  it("sits next to compiler diagnostics, ranked below them, none dropped", () => {
    const compiler = fromWire([
      wire({ severity: "warning", line: 1 }),
      wire({ line: 5 }),
    ]);
    const prose = fromWriting("main.typ", doc, [
      finding({ severity: "info", from: 0 }),
      finding({ severity: "warning", from: 2 }),
    ]);
    const merged = sortProblems([...prose, ...compiler]);
    expect(merged).toHaveLength(4);
    expect(merged.map((p) => p.severity)).toEqual([
      "error",
      "warning",
      "warning",
      "info",
    ]);
    expect(merged[0]?.source).toBeUndefined();
  });

  it("does not count advice as errors or warnings, and keeps it out of the gutter", () => {
    const prose = fromWriting("main.typ", doc, [
      finding({ severity: "info" }),
      finding({ severity: "warning" }),
    ]);
    expect(countProblems(prose)).toEqual({ errors: 0, warnings: 1 });
    expect(toGutter("= Title\n", prose)).toEqual([]);
  });
});
