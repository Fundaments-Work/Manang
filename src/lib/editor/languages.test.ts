import { describe, expect, it } from "vitest";
import { StringStream } from "@codemirror/language";
import { tags } from "@lezer/highlight";

import {
  bibtexParser,
  languageFor,
  loadLanguageExtension,
} from "./languages";

describe("editor language selection", () => {
  it("selects language by file extension", () => {
    // LaTeX extensions
    expect(languageFor("document.tex")).toBe("latex");
    expect(languageFor("packages/mypackage.sty")).toBe("latex");
    expect(languageFor("classes/article.cls")).toBe("latex");
    expect(languageFor("macro.dtx")).toBe("latex");
    expect(languageFor("macro.ltx")).toBe("latex");

    // BibTeX extensions
    expect(languageFor("references.bib")).toBe("bibtex");
    expect(languageFor("sources.biblatex")).toBe("bibtex");

    // Typst extension
    expect(languageFor("main.typ")).toBe("typst");
    expect(languageFor("sections/results.typ")).toBe("typst");

    // Markdown extensions
    expect(languageFor("README.md")).toBe("markdown");
    expect(languageFor("notes.markdown")).toBe("markdown");

    // Plain text / others
    expect(languageFor("notes.txt")).toBe("plain");
    expect(languageFor("data.csv")).toBe("plain");
    expect(languageFor("Makefile")).toBe("plain");
  });
});

describe("bibtex stream parser", () => {
  function tokenizeLine(
    line: string,
    state = bibtexParser.startState!(2),
  ): { token: string | null; text: string }[] {
    const stream = new StringStream(line, 2, 2);
    const tokens: { token: string | null; text: string }[] = [];

    while (!stream.eol()) {
      const pos = stream.pos;
      const token = bibtexParser.token(stream, state);
      if (stream.pos === pos) {
        stream.next();
      }
      tokens.push({ token, text: line.slice(pos, stream.pos) });
    }

    return tokens;
  }

  it("tokenizes an @article entry with key, fields, braces, strings and numbers", () => {
    const state = bibtexParser.startState!(2);

    // Line 1: @article{landauer1961,
    const line1 = tokenizeLine("@article{landauer1961,", state);
    expect(line1.find((t) => t.text === "@article")?.token).toBe("entryType");
    expect(line1.find((t) => t.text === "{")?.token).toBe("bracket");
    expect(line1.find((t) => t.text === "landauer1961")?.token).toBe("citationKey");
    expect(line1.find((t) => t.text === ",")?.token).toBe("punctuation");

    // Line 2:   author = {Landauer, Rolf},
    const line2 = tokenizeLine("  author = {Landauer, Rolf},", state);
    expect(line2.find((t) => t.text === "author")?.token).toBe("fieldName");
    expect(line2.find((t) => t.text === "=")?.token).toBe("operator");
    expect(line2.find((t) => t.text.includes("Landauer"))?.token).toBe("string");
    expect(line2.find((t) => t.text === ",")?.token).toBe("punctuation");

    // Line 3:   year = 1961,
    const line3 = tokenizeLine("  year = 1961,", state);
    expect(line3.find((t) => t.text === "year")?.token).toBe("fieldName");
    expect(line3.find((t) => t.text === "=")?.token).toBe("operator");
    expect(line3.find((t) => t.text === "1961")?.token).toBe("number");

    // Line 4:   journal = "IBM JRD",
    const line4 = tokenizeLine('  journal = "IBM JRD",', state);
    expect(line4.find((t) => t.text === "journal")?.token).toBe("fieldName");
    expect(line4.find((t) => t.text === "=")?.token).toBe("operator");
    expect(line4.find((t) => t.text.includes("IBM"))?.token).toBe("string");

    // Line 5: }
    const line5 = tokenizeLine("}", state);
    expect(line5.find((t) => t.text === "}")?.token).toBe("bracket");
    expect(state.stage).toBe("top");
  });

  it("tokenizes comments (% and outside prose)", () => {
    const state = bibtexParser.startState!(2);

    const commentLine = tokenizeLine("% this is a line comment", state);
    expect(commentLine[0]?.token).toBe("comment");

    const proseLine = tokenizeLine("This is free text before any entry", state);
    expect(proseLine.some((t) => t.token === "comment")).toBe(true);
  });

  it("maps tokens to Lezer tags via tokenTable", () => {
    const table = bibtexParser.tokenTable;
    expect(table).toBeDefined();
    expect(table?.entryType).toBe(tags.keyword);
    expect(table?.citationKey).toBe(tags.labelName);
    expect(table?.fieldName).toBe(tags.propertyName);
    expect(table?.string).toBe(tags.string);
    expect(table?.number).toBe(tags.number);
    expect(table?.comment).toBe(tags.comment);
  });
});

describe("loadLanguageExtension", () => {
  it("loads latex language extension", async () => {
    const ext = await loadLanguageExtension("latex");
    expect(ext).toBeDefined();
    expect(Array.isArray(ext)).toBe(true);
  });

  it("loads bibtex language extension", async () => {
    const ext = await loadLanguageExtension("bibtex");
    expect(ext).toBeDefined();
    expect(Array.isArray(ext)).toBe(true);
  });

  it("loads typst language extension", async () => {
    const ext = await loadLanguageExtension("typst");
    expect(ext).toBeDefined();
  });

  it("loads markdown language extension", async () => {
    const ext = await loadLanguageExtension("markdown");
    expect(ext).toBeDefined();
  });

  it("returns empty extension for plain text", async () => {
    const ext = await loadLanguageExtension("plain");
    expect(ext).toEqual([]);
  });
});
