import { describe, expect, it } from "vitest";

import { createDevKernel } from "$lib/kernel/dev";
import { latexApi, markdownApi } from "$lib/kernel/api";
import { sniffArtifact } from "$lib/kernel/artifact";

import { explainLatexFailure, fromLatexReport, fromMarkdown } from "./problems";

describe("fromLatexReport", () => {
  it("makes paths project-relative, drops info, and sorts errors first", () => {
    const problems = fromLatexReport(
      {
        diagnostics: [
          {
            severity: "warning",
            message: "Overfull",
            file: "a.tex",
            line: 9,
            hint: null,
          },
          { severity: "info", message: "chatter", file: null, line: null, hint: null },
          {
            severity: "error",
            message: "Oops",
            file: "b.tex",
            line: 2,
            hint: "Fix it",
          },
        ],
      },
      "thesis/main.tex",
    );
    expect(problems.map((p) => [p.severity, p.file, p.line, p.hints])).toEqual([
      ["error", "thesis/b.tex", 2, ["Fix it"]],
      ["warning", "thesis/a.tex", 9, []],
    ]);
  });

  it("attributes a diagnostic with no file to the main file", () => {
    const [p] = fromLatexReport(
      {
        diagnostics: [
          { severity: "error", message: "x", file: null, line: null, hint: null },
        ],
      },
      "main.tex",
    );
    expect(p?.file).toBe("main.tex");
    expect(p?.line).toBeNull();
  });
});

describe("fromMarkdown", () => {
  it("keeps warnings and errors with their line and ignores other severities", () => {
    const problems = fromMarkdown(
      [
        { severity: "warning", message: "w", line: 3 },
        { severity: "info", message: "i" },
        { severity: "error", message: "e" },
      ],
      "notes.md",
    );
    expect(problems.map((p) => [p.severity, p.file, p.line])).toEqual([
      ["error", "notes.md", null],
      ["warning", "notes.md", 3],
    ]);
  });
});

describe("explainLatexFailure", () => {
  it("turns plugin error codes into a title and a fix", () => {
    const none = explainLatexFailure("[no_tex] No TeX installation found on PATH");
    expect(none).toMatchObject({ code: "no_tex", title: "No TeX installation found" });
    expect(none.fix).toContain("TeX Live");
    expect(explainLatexFailure("latexmk: command not found").code).toBe("no_tex");
    const worker = explainLatexFailure("[worker_missing] manang-latex-worker");
    expect(worker.code).toBe("no_tex");
    expect(worker.fix).toContain("cargo build -p manang-latex --features engine");
    expect(explainLatexFailure("[bundle_unavailable] offline").code).toBe(
      "bundle_unavailable",
    );
    expect(explainLatexFailure("plain failure")).toEqual({
      code: null,
      title: "plain failure",
      fix: null,
    });
  });
});

describe("dev kernel latex and markdown", () => {
  it("compiles the demo thesis through the main file and serves a PDF", async () => {
    const kernel = createDevKernel();
    await kernel.run("project.open", { path: "/home/author/thesis-draft" });
    const api = latexApi(kernel);
    const report = await api.compile({ main: "/home/author/thesis-draft/main.tex" });
    expect(report).toMatchObject({
      ok: true,
      synctex: true,
      engine: "pdflatex",
      backend: "system",
    });
    expect(report.pages).toBeGreaterThan(0);
    const pdf = await api.exportPdf("/home/author/thesis-draft/main.tex");
    expect(sniffArtifact(pdf)?.kind).toBe("pdf");
  });

  it("reports a missing \\input with a project-relative file and line", async () => {
    const kernel = createDevKernel();
    await kernel.run("project.open", { path: "/home/author/thesis-draft" });
    await kernel.run("project.write-file", {
      path: "main.tex",
      content:
        "\\documentclass{article}\n\\begin{document}\n\\input{nope}\n\\end{document}\n",
    });
    const report = await latexApi(kernel).compile({
      main: "/home/author/thesis-draft/main.tex",
    });
    expect(report.ok).toBe(false);
    expect(report.pages).toBeNull();
    expect(report.diagnostics[0]).toMatchObject({
      severity: "error",
      file: "main.tex",
      line: 3,
    });
  });

  it("renders Markdown with the exact Rust shape and escapes raw HTML", async () => {
    const kernel = createDevKernel();
    const out = await markdownApi(kernel).renderHtml(
      "---\ntitle: T\n---\n# Hello\n\nSome *text* <b>x</b>\n\n- a\n- b\n",
    );
    expect(Object.keys(out).sort()).toEqual([
      "diagnostics",
      "diagrams",
      "frontmatter",
      "headings",
      "html",
    ]);
    expect(out.headings).toEqual([{ title: "Hello", level: 1, line: 4 }]);
    expect(out.html).toContain("<h1");
    expect(out.html).toContain("&lt;b&gt;");
    expect(out.html).not.toContain("<b>");
  });
});
