import { describe, expect, it } from "vitest";

import { projectApi } from "$lib/kernel/api";
import { sniffArtifact } from "$lib/kernel/artifact";
import { createDevKernel } from "$lib/kernel/dev";

import {
  DIAGRAM_KINDS,
  chosenKind,
  newDiagramRequest,
  diagramErrorText,
  diagramLanguageOf,
  diagramNameProblem,
  diagramPath,
  includeLine,
  parseDiagramError,
} from "./diagram";
import { stepChoice } from "./dialogRequest";
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
    expect(includeLine("typst", "figures/x.mmd")).toBe(
      '#figure(image("figures/x.svg"))',
    );
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

describe("the New diagram dialog", () => {
  it("has a name field, the kind as one single-select, and one Create action", () => {
    const request = newDiagramRequest("flow", "");
    expect(request.input).toEqual({ label: "Name", value: "flow" });
    expect(request.actions).toEqual([{ id: "create", label: "Create" }]);
    expect(request.choice?.options.map((o) => o.label)).toEqual([
      "Flowchart",
      "Sequence",
      "Pikchr",
      "ASCII",
    ]);
    expect(request.choice?.value).toBe("mermaid-flowchart");
    // Asking again after a bad name keeps the kind that was picked.
    expect(newDiagramRequest("x", "pikchr").choice?.value).toBe("pikchr");
  });

  it("reads the answer: Create with a kind, or nothing", () => {
    expect(chosenKind({ id: "create", value: "a", choice: "ascii" })?.extension).toBe(
      "bob",
    );
    expect(chosenKind({ id: "cancel", value: "a", choice: "ascii" })).toBeNull();
    expect(chosenKind({ id: "create", value: "a", choice: "nope" })).toBeNull();
  });

  it("moves the selection with the arrow keys, wrapping", () => {
    const options = newDiagramRequest("x", "").choice?.options ?? [];
    expect(stepChoice(options, "mermaid-flowchart", "ArrowRight")).toBe(
      "mermaid-sequence",
    );
    expect(stepChoice(options, "mermaid-flowchart", "ArrowLeft")).toBe("ascii");
    expect(stepChoice(options, "ascii", "ArrowDown")).toBe("mermaid-flowchart");
    expect(stepChoice(options, "pikchr", "Home")).toBe("mermaid-flowchart");
    expect(stepChoice(options, "pikchr", "End")).toBe("ascii");
    expect(stepChoice(options, "pikchr", "Enter")).toBeNull();
    expect(stepChoice([], "x", "ArrowLeft")).toBeNull();
  });
});

describe("rendered diagram files in the browser kernel", () => {
  const FLOW = "flowchart TD\n  A[Start] --> B[Done]\n";

  async function opened(engine: "typst" | "latex") {
    const kernel = createDevKernel();
    const project = projectApi(kernel);
    await project.create(`/home/author/${engine}-diagrams`, "D", engine);
    await project.writeFile("figures/flow.mmd", FLOW);
    return project;
  }

  it("writes a print SVG with its key next to the source, once", async () => {
    const project = await opened("typst");
    const first = await project.exportDiagram("figures/flow.mmd");
    expect(first).toMatchObject({
      ok: true,
      svg: "figures/flow.svg",
      pdf: null,
      written: ["figures/flow.svg"],
    });
    const svg = await project.readFile("figures/flow.svg");
    expect(svg).toMatch(/^<!-- manang-diagram mermaid key=[0-9a-f]+ -->\n<svg/);
    expect(sniffArtifact(new TextEncoder().encode(svg))?.kind).toBe("svg");
    expect((await project.exportDiagram("figures/flow.mmd")).written).toEqual([]);
    const stat = await project.stat("figures/flow.svg");
    expect(stat).toMatchObject({
      kind: "file",
      size: new TextEncoder().encode(svg).length,
    });
  });

  it("keeps the old SVG when the source has an error", async () => {
    const project = await opened("typst");
    await project.exportDiagram("figures/flow.mmd");
    const good = await project.readFile("figures/flow.svg");
    await project.writeFile("figures/flow.mmd", `${FLOW}  B -->>> C\n`);
    const broken = await project.exportDiagram("figures/flow.mmd");
    expect(broken).toMatchObject({ ok: false, svg: "figures/flow.svg", written: [] });
    expect(broken.error).toMatch(/^\[diagram_error\] line 3:/);
    expect(await project.readFile("figures/flow.svg")).toBe(good);
  });

  it("adds the PDF in LaTeX projects", async () => {
    const project = await opened("latex");
    const out = await project.exportDiagram("figures/flow.mmd");
    expect(out.pdf).toBe("figures/flow.pdf");
    expect(out.written).toEqual(["figures/flow.svg", "figures/flow.pdf"]);
  });
});
