import { describe, expect, it } from "vitest";

import { diagramApi } from "./api";
import { sniffArtifact } from "./artifact";
import { createDevKernel } from "./dev";
import { mockCheck, parseFlowchart } from "./mockDiagram";

const FLOW = "flowchart LR\n  %% comment\n  A[Start] --> B[End]\n  B --> C\n";

describe("mock diagram plugin", () => {
  const api = diagramApi(createDevKernel());

  it("lists the three languages of the contract", async () => {
    const { languages } = await api.languages();
    expect(languages.map((l) => l.id)).toEqual(["mermaid", "pikchr", "svgbob"]);
    expect(languages[2]?.fences).toEqual(["bob", "svgbob"]);
  });

  it("parses the flowchart subset", () => {
    const chart = parseFlowchart(FLOW);
    expect(chart?.direction).toBe("LR");
    expect(chart?.labels.get("A")).toBe("Start");
    expect(chart?.labels.get("C")).toBe("C");
    expect(chart?.edges).toEqual([
      ["A", "B"],
      ["B", "C"],
    ]);
    expect(parseFlowchart("sequenceDiagram\n")).toBeNull();
  });

  it("renders SVG bytes with the labels, escaped", async () => {
    const bytes = await api.render("mermaid", "flowchart TD\nA[a < b] --> B\n", "dark");
    expect(sniffArtifact(bytes)?.kind).toBe("svg");
    const svg = new TextDecoder().decode(bytes);
    expect(svg).toContain("a &lt; b");
    expect(svg).toContain("#141416");
  });

  it("follows the theme", async () => {
    const light = new TextDecoder().decode(await api.render("mermaid", FLOW, "light"));
    expect(light).toContain("#faf9f7");
  });

  it("draws a placeholder for other inputs", async () => {
    const svg = new TextDecoder().decode(await api.render("pikchr", "box", "dark"));
    expect(svg).toContain("Pikchr");
  });

  it("reports -->>> with a 1-based scalar line and column", async () => {
    const source = "flowchart TD\n  é -->>> B\n";
    expect(mockCheck(source)).toMatchObject([
      { severity: "error", line: 2, column: 5, endColumn: 10 },
    ]);
    const { diagnostics } = await api.check("mermaid", source);
    expect(diagnostics).toHaveLength(1);
    await expect(api.render("mermaid", source, "dark")).rejects.toThrow(
      /^\[diagram_error\] line 2:5: /,
    );
  });

  it("gives equal keys for equal inputs only", async () => {
    const a = await api.key("mermaid", FLOW, "dark");
    expect(await api.key("mermaid", FLOW, "dark")).toEqual(a);
    expect(await api.key("mermaid", FLOW, "light")).not.toEqual(a);
  });

  it("rejects an unknown language", async () => {
    await expect(api.check("nope" as "mermaid", "")).rejects.toThrow(
      /unknown_language/,
    );
  });
});
