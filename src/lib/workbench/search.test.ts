import { describe, expect, it } from "vitest";
import {
  formatMatchCount,
  parsePatternList,
  searchNames,
  searchText,
  splitLineByRanges,
} from "./search";

describe("workbench search utilities", () => {
  it("splitLineByRanges splits lines accurately around matches", () => {
    const line = "The irreversible entropy production rate";
    // "irreversible" is at index 4..16
    const segments = splitLineByRanges(line, [{ start: 4, end: 16 }]);
    expect(segments).toEqual([
      { text: "The ", isMatch: false },
      { text: "irreversible", isMatch: true },
      { text: " entropy production rate", isMatch: false },
    ]);
  });

  it("splitLineByRanges handles multiple matches on one line", () => {
    const line = "foo bar foo";
    const segments = splitLineByRanges(line, [
      { start: 0, end: 3 },
      { start: 8, end: 11 },
    ]);
    expect(segments).toEqual([
      { text: "foo", isMatch: true },
      { text: " bar ", isMatch: false },
      { text: "foo", isMatch: true },
    ]);
  });

  it("parsePatternList parses comma/newline-delimited globs", () => {
    expect(parsePatternList("*.typ, src/**/*.rs; !dist")).toEqual([
      "*.typ",
      "src/**/*.rs",
      "!dist",
    ]);
    expect(parsePatternList("")).toEqual([]);
  });

  it("formatMatchCount formats singular and plural cases", () => {
    expect(formatMatchCount(1, 1)).toBe("1 match in 1 file");
    expect(formatMatchCount(3, 2)).toBe("3 matches in 2 files");
  });

  it("searchNames returns ranked matches", () => {
    const paths = ["main.typ", "figures/state.svg", "sections/results.typ"];
    expect(searchNames(paths, "results")).toEqual(["sections/results.typ"]);
  });

  it("searchText finds text matches in string", () => {
    const text = "Hello world\nAnother world";
    const matches = searchText(text, "world");
    expect(matches.length).toBe(2);
    expect(matches[0]?.line).toBe(1);
    expect(matches[1]?.line).toBe(2);
  });
});
