import { describe, expect, it } from "vitest";

import { fuzzyMatch, fuzzyRank } from "./fuzzy";

describe("fuzzyMatch", () => {
  it("matches a subsequence regardless of order-of-typing speed", () => {
    expect(fuzzyMatch("Export Publication PDF", "expdf")).not.toBeNull();
  });

  it("returns null when a character is absent", () => {
    expect(fuzzyMatch("Export Publication PDF", "exportq")).toBeNull();
  });

  it("is case and diacritic insensitive", () => {
    expect(fuzzyMatch("Vérifier les citations", "verifier")).not.toBeNull();
  });

  it("reports the matched indices for highlighting", () => {
    const match = fuzzyMatch("Toggle Split Screen", "split");
    expect(match).not.toBeNull();
    // "split" starts at the first character of the second word.
    expect(match?.indices[0]).toBe(7);
  });

  it("scores consecutive runs above scattered matches", () => {
    const tight = fuzzyMatch("Toggle Split", "split");
    const loose = fuzzyMatch("Save Preferences", "split");
    expect(tight?.score ?? 0).toBeGreaterThan(loose?.score ?? 0);
  });

  it("treats an empty query as a match with no highlight", () => {
    const match = fuzzyMatch("anything", "   ");
    expect(match).toEqual({ score: 0, indices: [] });
  });
});

describe("fuzzyRank", () => {
  const items = ["Verify citations", "Open Drawing Canvas", "Toggle Split Screen"];

  it("returns everything unfiltered for an empty query", () => {
    expect(fuzzyRank(items, "", (item) => item)).toHaveLength(items.length);
  });

  it("drops non-matches entirely", () => {
    const ranked = fuzzyRank(items, "canvas", (item) => item);
    expect(ranked).toHaveLength(1);
    expect(ranked[0]?.item).toBe("Open Drawing Canvas");
  });

  it("orders by descending score", () => {
    const ranked = fuzzyRank(items, "o", (item) => item);
    const scores = ranked.map((entry) => entry.score);
    expect([...scores].sort((a, b) => b - a)).toEqual(scores);
  });

  it("prefers the shorter haystack when both match equally", () => {
    const ranked = fuzzyRank(
      ["Zen", "Zen Mode Distraction Free"],
      "zen",
      (item) => item,
    );
    expect(ranked[0]?.item).toBe("Zen");
  });
});
