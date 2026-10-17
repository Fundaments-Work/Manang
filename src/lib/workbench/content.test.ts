import { describe, expect, it } from "vitest";

import { fileKind, engineOf, isOpenable } from "./paths";
import { authorSummary, citationFor, filterReferences, parseBibtex } from "./references";
import { MAX_MATCHES_PER_FILE, searchNames, searchText } from "./search";
import { clampSlide, parseSlides } from "./slides";

const BIB = `
@comment{ignore me}
@article{landauer1961,
  author  = {Landauer, Rolf},
  title   = {Irreversibility and {heat} generation},
  journal = {IBM J. Res. Dev.},
  year    = {1961}
}
@book{nielsen2010, author = "Nielsen, Michael A. and Chuang, Isaac L.", title = "Quantum Computation",
  publisher = {CUP}, year = 2010}
@article{berut2012,
  author = {B{\\'e}rut, Antoine and Lutz, Eric and Ciliberto, Sergio},
  title = {Experimental verification},
  year = {2012}
}
`;

describe("references", () => {
  const entries = parseBibtex(BIB, "references.bib");

  it("reads entries, skipping comments", () => {
    expect(entries.map((e) => e.key)).toEqual(["landauer1961", "nielsen2010", "berut2012"]);
    expect(entries[0]).toMatchObject({
      type: "article",
      title: "Irreversibility and heat generation",
      year: "1961",
      venue: "IBM J. Res. Dev.",
      file: "references.bib",
    });
  });

  it("handles quoted and bare values, name order and accents", () => {
    expect(entries[1]).toMatchObject({ year: "2010", authors: ["Michael A. Nielsen", "Isaac L. Chuang"] });
    expect(entries[2]?.authors[0]).toBe("Antoine Bérut");
  });

  it("filters by key, author, title and year", () => {
    expect(filterReferences(entries, "landauer").map((e) => e.key)).toEqual(["landauer1961"]);
    expect(filterReferences(entries, "2012").map((e) => e.key)).toContain("berut2012");
    expect(filterReferences(entries, "")).toHaveLength(3);
  });

  it("summarises authors and cites per engine", () => {
    expect(authorSummary(["Rolf Landauer"])).toBe("Landauer");
    expect(authorSummary(["A B", "C D"])).toBe("B and D");
    expect(authorSummary(["A B", "C D", "E F"])).toBe("B et al.");
    expect(authorSummary([])).toBe("Unknown author");
    expect(citationFor("k", "typst")).toBe("@k");
    expect(citationFor("k", "latex")).toBe("\\cite{k}");
    expect(citationFor("k", "markdown")).toBe("[@k]");
    expect(citationFor("k", null)).toBe("@k");
  });

  it("returns nothing for text that is not BibTeX", () => {
    expect(parseBibtex("just some prose")).toEqual([]);
  });
});

describe("slides", () => {
  const deck = parseSlides(`#set page(paper: "presentation-16-9")

= Talk title

== First idea

Some words
that wrap.

$ E = m c^2 $

== Second idea

Only prose.
`);

  it("starts a slide at every heading, with level, paragraphs and the first equation", () => {
    expect(deck.map((s) => [s.title, s.level])).toEqual([
      ["Talk title", 1],
      ["First idea", 2],
      ["Second idea", 2],
    ]);
    expect(deck[1]?.paragraphs).toEqual(["Some words that wrap."]);
    expect(deck[1]?.math).toBe("E = m c^2");
    expect(deck[2]?.math).toBeNull();
    expect(deck[0]?.id).toBe("slide-0");
  });

  it("yields no slides for a document without headings", () => {
    expect(parseSlides("plain text")).toEqual([]);
  });

  it("clamps the current slide", () => {
    expect(clampSlide(9, 3)).toBe(2);
    expect(clampSlide(-1, 3)).toBe(0);
    expect(clampSlide(1, 0)).toBe(0);
  });
});

describe("search", () => {
  it("finds every occurrence per line, case-insensitively, with 1-based positions", () => {
    const matches = searchText("Entropy rises\nthe entropy of entropy", "entropy");
    expect(matches.map((m) => [m.line, m.column])).toEqual([
      [1, 1],
      [2, 5],
      [2, 16],
    ]);
    expect(matches[0]).toMatchObject({ hit: "Entropy", before: "", after: " rises" });
  });

  it("caps matches and ignores an empty query", () => {
    expect(searchText("a".repeat(500), "a")).toHaveLength(MAX_MATCHES_PER_FILE);
    expect(searchText("abc", "")).toEqual([]);
  });

  it("ranks file names fuzzily", () => {
    expect(searchNames(["main.typ", "sections/results.typ", "notes.md"], "rslt")[0]).toBe("sections/results.typ");
    expect(searchNames(["a"], " ")).toEqual([]);
  });
});

describe("file classification", () => {
  it("tells manuscripts, figures and binaries apart", () => {
    expect(fileKind("a/b.typ")).toBe("typst");
    expect(fileKind("fig.figure.json")).toBe("figure");
    expect(fileKind("x.pdf")).toBe("binary");
    expect(fileKind("manang.toml")).toBe("config");
    expect(fileKind("README")).toBe("text");
    expect(engineOf("x.tex")).toBe("latex");
    expect(engineOf("x.bib")).toBeNull();
    expect(isOpenable("x.png")).toBe(false);
    expect(isOpenable("x.svg")).toBe(false);
    expect(isOpenable("x.bib")).toBe(true);
  });
});
