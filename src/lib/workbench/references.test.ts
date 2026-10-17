import { describe, expect, it } from "vitest";

import {
  authorSummary,
  citationFor,
  filterReferences,
  formatDifference,
  formatVenue,
  formatYear,
  verificationBadge,
} from "./references";
import type { CitationEntry } from "$lib/kernel/api";

describe("references workbench helpers", () => {
  it("formats author summaries with strings and CitationAuthor objects", () => {
    expect(authorSummary([])).toBe("Unknown author");
    expect(authorSummary(undefined)).toBe("Unknown author");

    // String inputs
    expect(authorSummary(["Rolf Landauer"])).toBe("Landauer");
    expect(authorSummary(["Rolf Landauer", "Antoine Bérut"])).toBe(
      "Landauer and Bérut",
    );
    expect(
      authorSummary(["Rolf Landauer", "Antoine Bérut", "Eric Lutz"]),
    ).toBe("Landauer et al.");

    // CitationAuthor objects
    expect(
      authorSummary([{ name: "Rolf Landauer", family: "Landauer", given: "Rolf" }]),
    ).toBe("Landauer");
    expect(
      authorSummary([
        { name: "Rolf Landauer", family: "Landauer" },
        { name: "Antoine Bérut", family: "Bérut" },
      ]),
    ).toBe("Landauer and Bérut");
    expect(
      authorSummary([
        { name: "Rolf Landauer", family: "Landauer" },
        { name: "Antoine Bérut", family: "Bérut" },
        { name: "Eric Lutz", family: "Lutz" },
      ]),
    ).toBe("Landauer et al.");
  });

  it("produces correct citation syntax per engine", () => {
    expect(citationFor("key1", "typst")).toBe("@key1");
    expect(citationFor("key1", "latex")).toBe("\\cite{key1}");
    expect(citationFor("key1", "markdown")).toBe("[@key1]");
    expect(citationFor("key1", null)).toBe("@key1");
  });

  it("formats years and venues cleanly", () => {
    expect(formatYear(1961)).toBe("1961");
    expect(formatYear("2024")).toBe("2024");
    expect(formatYear(undefined)).toBe("");
    expect(formatYear(null)).toBe("");

    expect(
      formatVenue({
        journal_or_publisher: "Nature",
        volume: "483",
        number: "7388",
        pages: "187–189",
      }),
    ).toBe("Nature, 483(7388), 187–189");

    expect(
      formatVenue({
        journal_or_publisher: "IBM J. Res. Dev.",
        volume: "5",
        pages: "183-191",
      }),
    ).toBe("IBM J. Res. Dev., 5, 183-191");

    expect(
      formatVenue({
        venue: "Cambridge University Press",
      }),
    ).toBe("Cambridge University Press");
  });

  it("filters CitationEntry items by query words", () => {
    const entries: CitationEntry[] = [
      {
        key: "landauer1961",
        entry_type: "article",
        title: "Irreversibility and heat generation",
        authors: [{ name: "Rolf Landauer", family: "Landauer" }],
        year: 1961,
        journal_or_publisher: "IBM J. Res. Dev.",
      },
      {
        key: "berut2012",
        entry_type: "article",
        title: "Experimental verification of Landauer's principle",
        authors: [{ name: "Antoine Bérut", family: "Bérut" }],
        year: 2012,
        journal_or_publisher: "Nature",
      },
    ];

    expect(filterReferences(entries, "").length).toBe(2);
    expect(filterReferences(entries, "landauer").length).toBe(2);
    expect(filterReferences(entries, "1961").map((e) => e.key)).toEqual([
      "landauer1961",
    ]);
    expect(filterReferences(entries, "nature").map((e) => e.key)).toEqual([
      "berut2012",
    ]);
    expect(filterReferences(entries, "nonexistent")).toEqual([]);
  });

  it("provides verification badge metadata and difference formatting", () => {
    const verified = verificationBadge("verified");
    expect(verified.label).toBe("Verified");
    expect(verified.colorClass).toContain("text-accent");
    expect(verified.icon).toBe("check");

    const mismatch = verificationBadge("mismatch");
    expect(mismatch.label).toBe("Mismatch");
    expect(mismatch.colorClass).toContain("text-amber-400");
    expect(mismatch.icon).toBe("warning");

    const notFound = verificationBadge("not_found");
    expect(notFound.label).toBe("Not found");
    expect(notFound.icon).toBe("references");

    const error = verificationBadge("error");
    expect(error.label).toBe("Error");
    expect(error.icon).toBe("error");

    expect(
      formatDifference({ field: "year", local: "2012", remote: "2011" }),
    ).toBe('year: local "2012" vs remote "2011"');
  });
});
