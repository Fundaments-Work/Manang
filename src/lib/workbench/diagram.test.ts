import { describe, expect, it } from "vitest";

import {
  DIAGRAM_KINDS,
  diagramErrorText,
  diagramLanguageOf,
  diagramNameProblem,
  diagramPath,
  includeLine,
  parseDiagramError,
} from "./diagram";
import { openAsByName } from "./opener";
import { fromDiagram, toGutter } from "./problems";

describe("diagram files", () => {
  it("recognises the diagram extensions", () => {
    expect(diagramLanguageOf("figures/a.mmd")).toBe("mermaid");
    expect(diagramLanguageOf("a.MERMAID")).toBe("mermaid");
    expect(diagramLanguageOf("a.pikchr")).toBe("pikchr");
    expect(diagramLanguageOf("a.bob")).toBe("svgbob");
    expect(diagramLanguageOf("main.typ")).toBeNull();
  });

  it("opens every diagram file as text", () => {
    for (const path of ["a.mmd", "a.mermaid", "a.pikchr", "a.bob"]) {
      expect(openAsByName(path)).toBe("text");
    }
  });

  it("names new diagrams under figures/", () => {
    const [flow, , pik, ascii] = DIAGRAM_KINDS;
    if (flow === undefined || pik === undefined || ascii === undefined) {
      throw new Error("missing kinds");
    }
    expect(diagramPath(" setup ", flow)).toBe("figures/setup.mmd");
    expect(diagramPath("setup.mmd", flow)).toBe("figures/setup.mmd");
    expect(diagramPath("x", pik)).toBe("figures/x.pikchr");
    expect(diagramPath("x", ascii)).toBe("figures/x.bob");
    expect(diagramNameProblem("")).not.toBeNull();
    expect(diagramNameProblem("a/b")).not.toBeNull();
    expect(diagramNameProblem("flow")).toBeNull();
  });

  it("builds the include line per engine", () => {
    expect(includeLine("typst", "figures/x.mmd")).toBe('#image("figures/x.svg")');
    expect(includeLine("latex", "figures/x.mmd")).toBe(
      "\\includegraphics{figures/x.pdf}",
    );
    expect(includeLine("markdown", "figures/x.bob")).toBe("![x](figures/x.svg)");
  });

  it("reads the position of a render failure", () => {
    expect(parseDiagramError("[diagram_error] line 3:7: bad arrow")).toMatchObject({
      line: 3,
      column: 7,
      message: "bad arrow",
      severity: "error",
    });
    expect(parseDiagramError("[timeout] took too long")).toBeNull();
    expect(diagramErrorText("[diagram_error] line 3:7: bad arrow")).toBe(
      "Line 3: bad arrow",
    );
    expect(diagramErrorText("[timeout] took too long")).toBe("took too long");
  });

  it("maps diagnostics to problems and gutter markers like Typst ones", () => {
    const text = "flowchart TD\n  é -->>> B\n";
    const problems = fromDiagram("figures/x.mmd", [
      {
        severity: "error",
        message: "bad",
        line: 2,
        column: 5,
        endLine: 2,
        endColumn: 10,
        code: "parse",
      },
    ]);
    expect(problems).toMatchObject([
      { file: "figures/x.mmd", line: 2, column: 5, source: "diagram", code: "parse" },
    ]);
    const [marker] = toGutter(text, problems);
    expect(text.slice(marker?.from, marker?.to)).toBe("-->>>");
  });
});
