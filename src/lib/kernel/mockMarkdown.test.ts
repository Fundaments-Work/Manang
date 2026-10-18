import { describe, expect, it } from "vitest";

import { markdownApi } from "./api";
import { createDevKernel } from "./dev";
import { createMockKernel } from "./mock";
import { markdownPlugin } from "./mockMarkdown";

const api = () => markdownApi(createMockKernel({ plugins: [markdownPlugin()] }));

describe("mock markdown plugin", () => {
  it("lists the manifest's commands, and the dev kernel includes them", async () => {
    const ids = (await createDevKernel().commands())
      .filter((c) => c.plugin === "markdown")
      .map((c) => c.id);
    expect(ids).toEqual([
      "markdown.diagrams",
      "markdown.outline",
      "markdown.render-html",
      "markdown.to-typst",
    ]);
  });

  it("converts headings, emphasis, lists, links and quotes to Typst", async () => {
    const source = [
      "# Notes",
      "",
      "Some **bold** and *soft* text with `code` and [a link](https://x.org).",
      "",
      "- one",
      "1. first",
      "> quoted",
      "",
      "```",
      "# not a heading",
      "```",
      "Costs $5 at #3 @home",
    ].join("\n");
    const { typst, diagnostics } = await api().toTypst(source);
    expect(typst).toBe(
      [
        "= Notes",
        "",
        'Some *bold* and _soft_ text with `code` and #link("https://x.org")[a link].',
        "",
        "- one",
        "+ first",
        "#quote(block: true)[quoted]",
        "",
        "```",
        "# not a heading",
        "```",
        "Costs \\$5 at \\#3 \\@home",
        "",
      ].join("\n"),
    );
    expect(diagnostics).toEqual([]);
  });

  it("drops frontmatter with an info diagnostic", async () => {
    const { typst, diagnostics } = await api().toTypst("---\ntitle: X\n---\n# Body\n");
    expect(typst).toBe("= Body\n");
    expect(diagnostics).toEqual([
      {
        severity: "info",
        message: "YAML frontmatter was extracted and omitted from Typst body",
        line: 1,
      },
    ]);
  });

  it("rejects a missing source", async () => {
    const k = createMockKernel({ plugins: [markdownPlugin()] });
    await expect(k.run("markdown.to-typst", {})).rejects.toMatchObject({
      message: "`source` must be a string",
    });
  });
});
