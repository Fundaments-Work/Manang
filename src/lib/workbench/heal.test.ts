import { describe, expect, it } from "vitest";

import type { HealFix, HealSuggestion } from "$lib/kernel/healApi";

import {
  applyEdits,
  choiceOf,
  diagnosticsFor,
  doneText,
  findingProblem,
  fixesOf,
  noProgressText,
  verdict,
} from "./heal";
import { toGutter, type Problem } from "./problems";

const problem = (over: Partial<Problem> = {}): Problem => ({
  id: "0:main.tex:4:Undefined control sequence: \\includegraphics",
  severity: "error",
  message: "Undefined control sequence: \\includegraphics",
  file: "main.tex",
  line: 4,
  column: null,
  hints: ["add it"],
  ...over,
});

const fix = (over: Partial<HealFix> = {}): HealFix => ({
  id: "latex.package:graphicx",
  label: "Add \\usepackage{graphicx}",
  why: "`\\includegraphics` is defined by the `graphicx` package.",
  edits: [{ from: 5, to: 5, insert: "\n\\usepackage{graphicx}" }],
  confidence: "certain",
  rule: "latex.missing-package",
  diagnostics: [0],
  ...over,
});

describe("applyEdits", () => {
  it("applies UTF-16 edits, keeping the order of insertions at one offset", () => {
    expect(
      applyEdits("a😀b", [
        { from: 3, to: 3, insert: "X" },
        { from: 3, to: 3, insert: "Y" },
        { from: 0, to: 1, insert: "A" },
      ]),
    ).toBe("A😀XYb");
  });

  it("skips edits outside the text or overlapping an earlier one", () => {
    expect(
      applyEdits("abcdef", [
        { from: 1, to: 4, insert: "-" },
        { from: 2, to: 3, insert: "!" },
        { from: 5, to: 99, insert: "?" },
        { from: 4, to: 2, insert: "?" },
      ]),
    ).toBe("a-ef");
  });

  it("does nothing without edits", () => {
    expect(applyEdits("same", [])).toBe("same");
  });
});

describe("requests and answers", () => {
  it("sends problems as diagnostics, with the range when the compiler gave one", () => {
    expect(
      diagnosticsFor([
        problem(),
        problem({ severity: "info", range: { start: 1, end: 3 }, column: 2 }),
      ]),
    ).toEqual([
      {
        severity: "error",
        message: "Undefined control sequence: \\includegraphics",
        line: 4,
        column: null,
        hints: ["add it"],
      },
      {
        severity: "warning",
        message: "Undefined control sequence: \\includegraphics",
        line: 4,
        column: 2,
        range: { start: 1, end: 3 },
        hints: ["add it"],
      },
    ]);
  });

  it("makes a scanned finding a Problem the panel and gutter can show", () => {
    const p = findingProblem(
      "n.md",
      {
        message: "Code fence is never closed",
        severity: "warning",
        from: 9,
        to: 16,
        line: 3,
        rule: "markdown.unclosed-fence",
      },
      1,
    );
    expect(p).toMatchObject({
      id: "heal:n.md:markdown.unclosed-fence:9",
      file: "n.md",
      line: 3,
      severity: "warning",
      source: "heal",
    });
  });

  it("picks the fixes of a diagnostic or a finding", () => {
    const suggestion: HealSuggestion = {
      enabled: true,
      findings: [],
      fixes: [
        fix(),
        fix({ id: "b", diagnostics: [1] }),
        fix({ id: "c", diagnostics: [], finding: 0 }),
      ],
    };
    expect(fixesOf(suggestion, 0, null).map((f) => f.id)).toEqual([
      "latex.package:graphicx",
    ]);
    expect(fixesOf(suggestion, 1, null).map((f) => f.id)).toEqual(["b"]);
    expect(fixesOf(suggestion, null, 0).map((f) => f.id)).toEqual(["c"]);
    expect(fixesOf(suggestion, null, null)).toEqual([]);
    expect(choiceOf(fix())).toEqual({
      id: "latex.package:graphicx",
      label: "Add \\usepackage{graphicx}",
      why: "`\\includegraphics` is defined by the `graphicx` package.",
      confidence: "certain",
    });
  });

  it("attaches the fixes to the gutter diagnostic of the same problem", () => {
    const text = "\\documentclass{article}\n\\begin{document}\n\\includegraphics{a}\n";
    const p = problem({ line: 3, column: 1 });
    const [d] = toGutter(text, [p], () => [choiceOf(fix())]);
    expect(d?.problemId).toBe(p.id);
    expect(d?.fixes?.[0]?.id).toBe("latex.package:graphicx");
    const [plain] = toGutter(text, [p]);
    expect(plain?.fixes).toBeUndefined();
  });
});

describe("notices", () => {
  it("says what was done in the past tense", () => {
    expect(doneText("Add \\usepackage{graphicx}")).toBe(
      "Added \\usepackage{graphicx}.",
    );
    expect(doneText("Change sett to set")).toBe("Changed sett to set.");
    expect(doneText("Import canvas from @preview/cetz:0.5.2")).toBe(
      "Imported canvas from @preview/cetz:0.5.2.",
    );
    expect(doneText("Close the code fence with ```")).toBe(
      "Closed the code fence with ```.",
    );
    expect(doneText("Rewrite it")).toBe("Applied: Rewrite it.");
    expect(noProgressText("Add \\usepackage{x}")).toBe(
      "Add \\usepackage{x} did not clear the error.",
    );
  });
});

describe("the verification verdict", () => {
  const msg = "Undefined control sequence: \\includegraphics";
  it("worked when the targeted error is gone, even if another appeared", () => {
    expect(verdict([msg], 1, [])).toBe("worked");
    expect(verdict([msg], 1, [problem({ message: "Other error" })])).toBe("worked");
  });

  it("worked when there are fewer errors", () => {
    expect(verdict([msg], 3, [problem(), problem({ id: "x", message: "b" })])).toBe(
      "worked",
    );
  });

  it("no progress when the same error stays and the count does not drop", () => {
    expect(verdict([msg], 1, [problem()])).toBe("no-progress");
    expect(
      verdict([msg], 2, [
        problem(),
        problem({ id: "y", message: "c" }),
        problem({ id: "z", message: "d" }),
      ]),
    ).toBe("no-progress");
  });

  it("warnings never count as the error", () => {
    expect(verdict([msg], 1, [problem({ severity: "warning" })])).toBe("worked");
  });
});
