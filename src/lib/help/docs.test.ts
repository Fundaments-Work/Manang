import { readdirSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { ALL_DOCS, DOCS_BY_SLUG, getDoc, searchDocs } from "./docs";

describe("help docs", () => {
  it("bundles every markdown file from docs/user/*.md", () => {
    const onDisk = readdirSync(new URL("../../../docs/user", import.meta.url)).filter(
      (name) => name.endsWith(".md"),
    ).length;
    expect(ALL_DOCS.length).toBe(onDisk);
    expect(DOCS_BY_SLUG.size).toBe(onDisk);
    expect(ALL_DOCS[0]?.slug).toBe("index");
    expect(ALL_DOCS[0]?.title).toContain("User Guide");
  });

  it("can retrieve docs by slug", () => {
    const gettingStarted = getDoc("getting-started");
    expect(gettingStarted).toBeDefined();
    expect(gettingStarted?.title).toBe("Getting Started");
    expect(gettingStarted?.raw).toContain("Home");

    const shortcuts = getDoc("menus-and-shortcuts");
    expect(shortcuts).toBeDefined();
    expect(shortcuts?.raw).toContain("shortcuts");
  });

  it("searches documentation by keyword", () => {
    const results = searchDocs("SyncTeX");
    expect(results.length).toBeGreaterThan(0);
    expect(results.some((r) => r.doc.slug === "preview-and-pdf")).toBe(true);

    const shortcutResults = searchDocs("keyboard shortcuts");
    expect(shortcutResults.length).toBeGreaterThan(0);
  });

  it("returns empty results for empty or unmatched query", () => {
    expect(searchDocs("")).toEqual([]);
    expect(searchDocs("   ")).toEqual([]);
    expect(searchDocs("xyznonexistentterm12345")).toEqual([]);
  });
});
