import { describe, expect, it } from "vitest";

import { applyEdits } from "$lib/workbench/heal";

import { createDevKernel } from "./dev";
import { healApi, type HealDiagnosticIn } from "./healApi";
import { createMockKernel } from "./mock";
import { healPlugin } from "./mockHeal";

const kernel = () => createMockKernel({ plugins: [healPlugin()] });
const error = (
  message: string,
  extra: Partial<HealDiagnosticIn> = {},
): HealDiagnosticIn => ({
  severity: "error",
  message,
  ...extra,
});

const DOC =
  "\\documentclass{article}\n\\usepackage[utf8]{inputenc}\n\\begin{document}\n\\includegraphics{a}\n\\end{document}\n";

describe("mock heal plugin", () => {
  it("lists exactly the manifest's two commands, and the dev kernel includes them", async () => {
    const ids = (await kernel().commands())
      .filter((c) => c.plugin === "heal")
      .map((c) => c.id);
    expect(ids.sort()).toEqual(["heal.configure", "heal.suggest"]);
    const dev = (await createDevKernel().commands()).map((c) => c.id);
    for (const id of ids) expect(dev).toContain(id);
  });

  it("adds the missing package after the last \\usepackage, with UTF-16 edits", async () => {
    const api = healApi(kernel());
    const out = await api.suggest({
      engine: "latex",
      file: "main.tex",
      text: DOC,
      diagnostics: [
        error("Undefined control sequence: \\includegraphics", { line: 4 }),
      ],
    });
    expect(out.enabled).toBe(true);
    const fix = out.fixes[0];
    expect(fix).toMatchObject({
      id: "latex.package:graphicx",
      label: "Add \\usepackage{graphicx}",
      confidence: "certain",
      rule: "latex.missing-package",
      diagnostics: [0],
    });
    expect(applyEdits(DOC, fix?.edits ?? [])).toBe(
      DOC.replace("{inputenc}\n", "{inputenc}\n\\usepackage{graphicx}\n"),
    );
  });

  it("never adds a loaded package and reads the token from the line when absent", async () => {
    const api = healApi(kernel());
    const loaded = DOC.replace("{inputenc}", "{inputenc,graphicx}");
    expect(
      (
        await api.suggest({
          engine: "latex",
          file: "main.tex",
          text: loaded,
          diagnostics: [error("Undefined control sequence", { line: 4 })],
        })
      ).fixes,
    ).toEqual([]);
    const out = await api.suggest({
      engine: "latex",
      file: "main.tex",
      text: DOC,
      diagnostics: [error("Undefined control sequence", { line: 4 })],
    });
    expect(out.fixes.map((f) => f.id)).toEqual(["latex.package:graphicx"]);
  });

  it("makes one Fix all of several certain fixes, and two alternatives for \\url", async () => {
    const api = healApi(kernel());
    const text =
      "\\documentclass{article}\n\\begin{document}\n\\includegraphics{a}\n\\toprule\n\\url{x}\n\\end{document}\n";
    const out = await api.suggest({
      engine: "latex",
      file: "main.tex",
      text,
      diagnostics: [
        error("Undefined control sequence: \\includegraphics", { line: 3 }),
        error("Undefined control sequence: \\toprule", { line: 4 }),
        error("Undefined control sequence: \\url", { line: 5 }),
      ],
    });
    expect(out.fixes.map((f) => [f.id, f.confidence])).toEqual([
      ["latex.package:graphicx", "certain"],
      ["latex.package:booktabs", "certain"],
      ["latex.package:hyperref", "likely"],
      ["latex.package:url", "likely"],
    ]);
    expect(out.fix_all?.fixes).toEqual([
      "latex.package:graphicx",
      "latex.package:booktabs",
    ]);
    const fixed = applyEdits(text, out.fix_all?.edits ?? []);
    expect(fixed).toContain(
      "\\documentclass{article}\n\\usepackage{graphicx}\n\\usepackage{booktabs}\n\\begin{document}",
    );
  });

  it("Typst: imports a package name, fixes a keyword slip, closes a call", async () => {
    const api = healApi(kernel());
    let text = "#cetz.canvas({})\n";
    let out = await api.suggest({
      engine: "typst",
      file: "main.typ",
      text,
      diagnostics: [
        error("unknown variable: canvas", { range: { start: 6, end: 12 }, line: 1 }),
      ],
    });
    expect(applyEdits(text, out.fixes[0]?.edits ?? [])).toBe(
      '#import "@preview/cetz:0.5.2": canvas\n#cetz.canvas({})\n',
    );
    text = "#sett text(size: 11pt)\n";
    out = await api.suggest({
      engine: "typst",
      file: "main.typ",
      text,
      diagnostics: [
        error("unknown variable: sett", { range: { start: 1, end: 5 }, line: 1 }),
      ],
    });
    expect(applyEdits(text, out.fixes[0]?.edits ?? [])).toBe("#set text(size: 11pt)\n");
    expect(out.fixes[0]?.confidence).toBe("likely");
    text = "#lorem(10\n\ntext\n";
    out = await api.suggest({
      engine: "typst",
      file: "main.typ",
      text,
      diagnostics: [
        error("unclosed delimiter", { range: { start: 6, end: 7 }, line: 1 }),
      ],
    });
    expect(applyEdits(text, out.fixes[0]?.edits ?? [])).toBe("#lorem(10)\n\ntext\n");
  });

  it("Markdown: an unclosed fence is a finding with a certain fix", async () => {
    const api = healApi(kernel());
    const text = "# Notes\n\n```rust\nfn main() {}\n";
    const out = await api.suggest({
      engine: "markdown",
      file: "n.md",
      text,
      diagnostics: [],
    });
    expect(out.findings[0]).toMatchObject({ line: 3, from: 9, to: 16 });
    expect(out.fixes[0]?.finding).toBe(0);
    expect(applyEdits(text, out.fixes[0]?.edits ?? [])).toBe(`${text}\`\`\`\n`);
  });

  it("heal.enabled off silences it; bad arguments are named", async () => {
    const api = healApi(kernel());
    await api.configure({ "heal.enabled": false });
    const out = await api.suggest({
      engine: "latex",
      file: "main.tex",
      text: DOC,
      diagnostics: [
        error("Undefined control sequence: \\includegraphics", { line: 4 }),
      ],
    });
    expect(out).toEqual({ fixes: [], findings: [], enabled: false });
    await expect(api.configure({ "heal.enabled": "yes" })).rejects.toMatchObject({
      message: expect.stringContaining("boolean"),
    });
    await expect(
      kernel().run("heal.suggest", { engine: "word", text: "x" }),
    ).rejects.toMatchObject({ message: expect.stringContaining("unknown engine") });
  });
});
