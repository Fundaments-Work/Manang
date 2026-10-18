import { describe, expect, it } from "vitest";

import { writingApi } from "./api";
import { createDevKernel } from "./dev";
import { createMockKernel } from "./mock";
import { maskProse, writingPlugin } from "./mockWriting";

const kernel = () => createMockKernel({ plugins: [writingPlugin()] });

describe("mock writing plugin", () => {
  it("lists exactly the manifest's fourteen commands, and the dev kernel includes them", async () => {
    const ids = (await kernel().commands())
      .filter((c) => c.plugin === "writing")
      .map((c) => c.id);
    expect(ids).toHaveLength(14);
    const dev = (await createDevKernel().commands()).map((c) => c.id);
    for (const id of ids) expect(dev).toContain(id);
  });

  it("spell.check answers {language, count, misspelled} in scalar offsets", async () => {
    const api = writingApi(kernel());
    const text = "😀 teh cat wich";
    const result = await api.spellCheck(text, "typst");
    expect(result.language).toBe("en_US");
    expect(result.count).toBe(2);
    expect(result.misspelled[0]).toMatchObject({
      word: "teh",
      range: { start: 2, end: 5 },
      line: 1,
      column: 3,
      suggestions: [],
    });
    expect(result.misspelled[1]?.range).toEqual({ start: 10, end: 14 });
  });

  it("does not check markup, math or code", async () => {
    const api = writingApi(kernel());
    const text = "$teh$ #wich `teh` // teh\nfine";
    expect((await api.spellCheck(text, "typst")).count).toBe(0);
  });

  it("suggests, and add-word silences a word", async () => {
    const k = kernel();
    const api = writingApi(k);
    expect((await api.suggest("teh", 3)).suggestions[0]).toBe("the");
    expect((await api.spellCheck("teh", "typst")).count).toBe(1);
    expect(await api.addWord("teh")).toEqual({ added: true, count: 1 });
    expect((await api.spellCheck("teh", "typst")).count).toBe(0);
    expect(await k.run("writing.spell.words")).toEqual({ words: ["teh"] });
  });

  it("lists and removes personal dictionary words; removed words are flagged again", async () => {
    const api = writingApi(kernel());
    await api.addWord("wich");
    await api.addWord("Teh");
    expect(await api.words()).toEqual({ words: ["teh", "wich"] });
    expect((await api.spellCheck("teh wich", "typst")).count).toBe(0);
    expect(await api.removeWord("teh")).toEqual({ removed: true, count: 1 });
    expect(await api.removeWord("teh")).toEqual({ removed: false, count: 1 });
    expect(await api.words()).toEqual({ words: ["wich"] });
    expect((await api.spellCheck("teh wich", "typst")).count).toBe(1);
  });

  it("lists the bundled languages like the Rust plugin", async () => {
    const api = writingApi(kernel());
    expect(await api.languages()).toEqual({
      default: "en_US",
      spellcheck: true,
      languages: [
        { code: "en_GB", source: "bundled" },
        { code: "en_US", source: "bundled" },
      ],
    });
  });

  it("lint returns the writing envelope with codes and an info severity", async () => {
    const api = writingApi(kernel());
    const out = await api.lint("This is is really good.", "typst");
    expect(out.source).toBe("writing");
    expect(out.diagnostics.map((d) => [d.code, d.severity])).toEqual([
      ["repeated-word", "warning"],
      ["weak-word", "info"],
    ]);
    expect(out.diagnostics[0]).toMatchObject({
      line: 1,
      column: 6,
      range: { start: 5, end: 10 },
      file: null,
    });
  });

  it("formats trailing whitespace and blank runs; LaTeX is unsupported", async () => {
    const api = writingApi(kernel());
    expect(await api.format("a  \n\n\n\nb", "markdown")).toEqual({
      formatted: "a\n\nb\n",
      changed: true,
    });
    await expect(api.format("\\a", "latex")).rejects.toMatchObject({
      message: expect.stringContaining("unsupported"),
    });
  });

  it("symbols come back in the requested syntax with a Unicode preview", async () => {
    const api = writingApi(kernel());
    const typst = (await api.symbols("alph", "typst")).symbols[0];
    expect(typst).toMatchObject({ insert: "alpha", preview: "α", unicode: "U+03B1" });
    const latex = (await api.symbols("alph", "latex")).symbols[0];
    expect(latex?.insert).toBe("\\alpha");
  });

  it("expands snippets with scalar tab-stop ranges and a final cursor", async () => {
    const api = writingApi(kernel());
    const exp = await api.expand("eq", "typst");
    expect(exp.trigger).toBe("equation");
    expect(exp.text).toBe("$ x = y $ <eq:label>\n");
    expect(exp.tab_stops.map((s) => s.index)).toEqual([1, 2, 3]);
    expect(exp.tab_stops[0]?.ranges).toEqual([{ start: 2, end: 3 }]);
    expect(exp.final_cursor).toBe(exp.text.length);
    const { snippets } = await api.snippets("latex");
    expect(snippets.map((s) => s.trigger)).toEqual(["theorem"]);
  });

  it("counts prose only", async () => {
    const api = writingApi(kernel());
    const stats = await api.stats(
      "Hello there world. $x + y$ Second one!\n\nNew para.",
      "typst",
    );
    expect(stats.words).toBe(7);
    expect(stats.sentences).toBe(3);
    expect(stats.paragraphs).toBe(2);
  });

  it("masking preserves scalar length", () => {
    const text = "😀 $x😀$ ok";
    expect([...maskProse(text, "typst")]).toHaveLength([...text].length);
  });
});
