import { describe, expect, it } from "vitest";

import { renderTemplateFiles } from "./mockTemplates";

describe("renderTemplateFiles", () => {
  it("escapes LaTeX titles the same way the Rust scaffolder does", () => {
    const files = renderTemplateFiles("blank-latex", "R&D 50% #1 \\ ~ ^");
    expect(files?.["main.tex"]).toContain(
      "\\title{R\\&D 50\\% \\#1 \\textbackslash{} \\textasciitilde{} \\textasciicircum{}}",
    );
  });
});
