import { describe, expect, it } from "vitest";

import {
  countLabel,
  findMatches,
  firstFrom,
  matchTop,
  pageText,
  queryPattern,
  runRanges,
  splitRun,
  stepMatch,
} from "./find";

const pages = [
  pageText([
    { str: "Entropy production", top: 40 },
    { str: " is bounded.", hasEOL: true, top: 40 },
    { str: "The entropy", top: 60 },
    { str: "", hasEOL: true },
    { str: "production rate.", top: 75 },
  ]),
  pageText([{ str: "No match here.", top: 10 }]),
  pageText([
    { str: "ENTROPY again", top: 20 },
    { str: "en", top: 30 },
    { str: "tropy split across runs", top: 30 },
  ]),
];

describe("page text", () => {
  it("joins runs and puts a space after a line end", () => {
    expect(pages[0]?.text).toBe(
      "Entropy production is bounded. The entropy production rate.",
    );
    expect(pages[0]?.starts).toEqual([0, 18, 31, 42, 43]);
  });
});

describe("find", () => {
  it("is case-insensitive by default and finds every page in order", () => {
    const matches = findMatches(pages, "entropy", { caseSensitive: false });
    expect(matches.map((m) => m.page)).toEqual([0, 0, 2, 2]);
    expect(matches[0]).toEqual({ page: 0, start: 0, end: 7 });
  });

  it("can match case", () => {
    const matches = findMatches(pages, "entropy", { caseSensitive: true });
    expect(matches.map((m) => m.page)).toEqual([0, 2]);
  });

  it("a space matches a line break between runs", () => {
    const matches = findMatches(pages, "entropy   production", {
      caseSensitive: false,
    });
    expect(matches).toHaveLength(2);
    expect(matches[1]).toMatchObject({ page: 0, start: 35 });
  });

  it("treats the query as text, not a pattern", () => {
    expect(findMatches(pages, "bounded.", { caseSensitive: false })).toHaveLength(1);
    expect(findMatches(pages, "b.unded", { caseSensitive: false })).toHaveLength(0);
    expect(queryPattern("(a+", { caseSensitive: false })?.test("x(a+y")).toBe(true);
    expect(queryPattern("   ", { caseSensitive: false })).toBeNull();
    expect(findMatches(pages, "", { caseSensitive: false })).toEqual([]);
  });

  it("maps a match back to the runs it covers, across run boundaries", () => {
    const page = pages[2];
    if (page === undefined) throw new Error("fixture");
    const matches = findMatches([page], "entropy split", { caseSensitive: false });
    expect(matches).toHaveLength(1);
    const match = matches[0];
    if (match === undefined) throw new Error("no match");
    expect(runRanges(page, match)).toEqual([
      { run: 1, start: 0, end: 2 },
      { run: 2, start: 0, end: 11 },
    ]);
    expect(matchTop(page, match)).toBe(30);
  });

  it("a match that runs into a line-end space stays within the runs' own text", () => {
    const page = pages[0];
    if (page === undefined) throw new Error("fixture");
    const match = findMatches([page], "bounded. the", { caseSensitive: false })[0];
    if (match === undefined) throw new Error("no match");
    expect(runRanges(page, match)).toEqual([
      { run: 1, start: 4, end: 12 },
      { run: 2, start: 0, end: 3 },
    ]);
  });
});

describe("moving between matches", () => {
  it("wraps around the document both ways", () => {
    expect(stepMatch(0, 4, 1)).toBe(1);
    expect(stepMatch(3, 4, 1)).toBe(0);
    expect(stepMatch(0, 4, -1)).toBe(3);
    expect(stepMatch(-1, 4, 1)).toBe(0);
    expect(stepMatch(-1, 4, -1)).toBe(3);
    expect(stepMatch(0, 0, 1)).toBe(-1);
  });

  it("a new search starts at the reader's page", () => {
    const matches = findMatches(pages, "entropy", { caseSensitive: false });
    expect(firstFrom(matches, 0)).toBe(0);
    expect(firstFrom(matches, 1)).toBe(2);
    expect(firstFrom(matches, 5)).toBe(0);
    expect(firstFrom([], 0)).toBe(-1);
  });

  it("counts like Overleaf", () => {
    expect(countLabel(2, 17, "x")).toBe("3 of 17");
    expect(countLabel(-1, 0, "x")).toBe("No matches");
    expect(countLabel(-1, 0, " ")).toBe("");
  });
});

describe("highlight pieces", () => {
  it("splits a run into plain, match and current pieces", () => {
    expect(
      splitRun("the entropy of entropy", [
        { start: 4, end: 11, current: false },
        { start: 15, end: 22, current: true },
      ]),
    ).toEqual([
      { text: "the ", hit: "none" },
      { text: "entropy", hit: "match" },
      { text: " of ", hit: "none" },
      { text: "entropy", hit: "current" },
    ]);
    expect(splitRun("abc", [])).toEqual([{ text: "abc", hit: "none" }]);
    expect(splitRun("abc", [{ start: 0, end: 3, current: true }])).toEqual([
      { text: "abc", hit: "current" },
    ]);
  });
});
