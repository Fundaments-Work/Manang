import { describe, expect, it } from "vitest";

import {
  fingerprint,
  includes,
  inputKey,
  resolveTarget,
  setMainInToml,
} from "./target";

const typst = { engine: "typst", main: "main.typ" } as const;
const latex = { engine: "latex", main: "main.tex" } as const;

describe("resolveTarget", () => {
  it("compiles the active file itself when it is the main file or standalone", () => {
    expect(resolveTarget({ active: "main.typ", project: typst })).toEqual({
      engine: "typst",
      main: "main.typ",
      viaMain: false,
    });
    expect(
      resolveTarget({ active: "talk.typ", project: typst, mainText: "= Hi" }),
    ).toMatchObject({ main: "talk.typ", viaMain: false });
  });

  it("compiles a Typst chapter through the main file that includes it", () => {
    const mainText = '#include "sections/results.typ"\n';
    expect(
      resolveTarget({ active: "sections/results.typ", project: typst, mainText }),
    ).toEqual({ engine: "typst", main: "main.typ", viaMain: true });
  });

  it("treats a LaTeX file without \\documentclass as a chapter of the main file", () => {
    expect(
      resolveTarget({
        active: "chapters/methods.tex",
        project: latex,
        activeText: "\\section{Methods}",
      }),
    ).toEqual({ engine: "latex", main: "main.tex", viaMain: true });
    expect(
      resolveTarget({
        active: "poster.tex",
        project: latex,
        activeText: "\\documentclass{article}",
      }),
    ).toMatchObject({ main: "poster.tex", viaMain: false });
  });

  it("falls back to the project's main for non-document tabs and to the file's own engine", () => {
    expect(resolveTarget({ active: "fig.figure.json", project: latex })).toMatchObject({
      engine: "latex",
      main: "main.tex",
    });
    expect(resolveTarget({ active: "notes.md", project: typst })).toMatchObject({
      engine: "markdown",
      main: "notes.md",
    });
    expect(resolveTarget({ active: null, project: null })).toBeNull();
  });
});

describe("includes", () => {
  it("matches the spellings a main file may use", () => {
    expect(includes("latex", "\\input{chapters/a}", "chapters/a.tex", "main.tex")).toBe(
      true,
    );
    expect(includes("latex", "\\include{a.tex}", "src/a.tex", "src/main.tex")).toBe(
      true,
    );
    expect(
      includes("typst", '#import "/sections/a.typ": x', "sections/a.typ", "main.typ"),
    ).toBe(true);
    expect(includes("typst", '#include "b.typ"', "a.typ", "main.typ")).toBe(false);
  });
});

describe("setMainInToml", () => {
  it("replaces an existing main and keeps the rest and its comment", () => {
    const toml =
      '[project]\nengine = "typst"\nmain = "main.typ" # root\n\n[bibliography]\npaths = []\n';
    expect(setMainInToml(toml, "paper.typ")).toBe(
      '[project]\nengine = "typst"\nmain = "paper.typ" # root\n\n[bibliography]\npaths = []\n',
    );
  });

  it("adds main inside [project] when absent, or the table when missing", () => {
    expect(setMainInToml('[project]\nengine = "latex"\n\n[x]\na = 1\n', "t.tex")).toBe(
      '[project]\nengine = "latex"\nmain = "t.tex"\n\n[x]\na = 1\n',
    );
    expect(setMainInToml("", "a.md")).toBe('[project]\nmain = "a.md"\n');
    expect(setMainInToml('[other]\nmain = "x"\n', "a.md")).toBe(
      '[other]\nmain = "x"\n\n[project]\nmain = "a.md"\n',
    );
  });

  it("keeps CRLF line endings", () => {
    expect(setMainInToml('[project]\r\nmain = "a.typ"\r\n', "b.typ")).toBe(
      '[project]\r\nmain = "b.typ"\r\n',
    );
  });
});

describe("inputKey", () => {
  it("changes with the text, the target and (for LaTeX) disk changes only", () => {
    const a = inputKey("typst", "main.typ", { "main.typ": "x" }, 0);
    expect(inputKey("typst", "main.typ", { "main.typ": "x" }, 5)).toBe(a);
    expect(inputKey("typst", "main.typ", { "main.typ": "y" }, 0)).not.toBe(a);
    expect(inputKey("typst", "other.typ", { "main.typ": "x" }, 0)).not.toBe(a);
    expect(inputKey("latex", "main.tex", {}, 1)).not.toBe(
      inputKey("latex", "main.tex", {}, 2),
    );
    expect(fingerprint("abc")).toBe(fingerprint("abc"));
  });
});
