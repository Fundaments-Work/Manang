import { describe, expect, it } from "vitest";

import { citationsPlugin, extractKeysFromSource } from "./mockCitations";
import { MockProjectHost } from "./mockProject";
import { createMockKernel } from "./mock";
import { citationsApi } from "./api";

const TEST_BIB = `
@article{landauer1961,
  author  = {Landauer, Rolf},
  title   = {Irreversibility and heat generation in the computing process},
  journal = {IBM Journal of Research and Development},
  year    = {1961},
  volume  = {5},
  pages   = {183--191},
  doi     = {10.1147/rd.53.0183}
}

@article{mismatch2012,
  author  = {B{\\'e}rut, Antoine},
  title   = {Experimental verification of Landauer's principle},
  year    = {2012}
}

@misc{notfound_entry,
  note    = {Unindexed preprint}
}
`;

const TEST_CSL = JSON.stringify([
  {
    id: "shannon1948",
    type: "article-journal",
    title: "A Mathematical Theory of Communication",
    author: [{ family: "Shannon", given: "Claude E." }],
    issued: { "date-parts": [[1948, 7]] },
    "container-title": "Bell System Technical Journal",
    volume: "27",
    page: "379-423",
    DOI: "10.1002/j.1538-7305.1948.tb01338.x",
  },
]);

function createTestContext() {
  const host = new MockProjectHost([
    {
      root: "/home/author/test-paper",
      name: "Test paper",
      engine: "typst",
      main: "main.typ",
      files: {
        "references.bib": TEST_BIB,
        "csl_refs.json": TEST_CSL,
      },
      committed: ["references.bib"],
      openedAt: 1000,
    },
  ]);
  host.open("/home/author/test-paper");

  const kernel = createMockKernel({
    plugins: [citationsPlugin(host)],
  });
  const api = citationsApi(kernel);

  return { host, kernel, api };
}

describe("mockCitations plugin", () => {
  it("loads bibliography files and tracks counts", async () => {
    const { api } = createTestContext();
    const res = await api.load({
      paths: [
        "/home/author/test-paper/references.bib",
        "/home/author/test-paper/csl_refs.json",
      ],
    });

    expect(res.loaded).toBe(4);
    expect(res.total).toBe(4);
    expect(res.errors).toEqual([]);
  });

  it("handles missing files gracefully in load diagnostics", async () => {
    const { api } = createTestContext();
    const res = await api.load({ path: "missing.bib" });
    expect(res.loaded).toBe(0);
    expect(res.errors.length).toBe(1);
    expect(res.errors[0]?.message).toContain("file not found");
  });

  it("searches entries with empty query and ranked text query", async () => {
    const { api } = createTestContext();
    await api.load({ path: "references.bib" });

    // Empty query returns all
    const all = await api.search({ query: "" });
    expect(all.results.length).toBe(3);

    // Text search matching author/title
    const landauer = await api.search({ query: "landauer" });
    expect(landauer.results.length).toBe(2);
    expect(landauer.results[0]?.key).toBe("landauer1961");
    expect(landauer.results[1]?.key).toBe("mismatch2012");

    // Exact author name query matches 1
    const rolf = await api.search({ query: "rolf" });
    expect(rolf.results.length).toBe(1);
    expect(rolf.results[0]?.key).toBe("landauer1961");

    // Text search matching year
    const byYear = await api.search({ query: "1961" });
    expect(byYear.results[0]?.key).toBe("landauer1961");
  });

  it("gets entry by key or throws if missing", async () => {
    const { api } = createTestContext();
    await api.load({ path: "references.bib" });

    const entry = await api.get("landauer1961");
    expect(entry.key).toBe("landauer1961");
    expect(entry.year).toBe(1961);
    expect(entry.doi).toBe("10.1147/rd.53.0183");

    await expect(api.get("unknown_key")).rejects.toMatchObject({
      message: "entry `unknown_key` not found",
    });
  });

  it("formats citations in APA and numeric styles", async () => {
    const { api } = createTestContext();
    await api.load({ path: "references.bib" });

    const apa = await api.format("landauer1961", "apa");
    expect(apa.style).toBe("apa");
    expect(apa.formatted).toContain("Landauer, R.");
    expect(apa.formatted).toContain("(1961).");
    expect(apa.formatted).toContain("https://doi.org/10.1147/rd.53.0183");

    const num = await api.format("landauer1961", "numeric");
    expect(num.style).toBe("numeric");
    expect(num.formatted).toContain("[1]");
    expect(num.formatted).toContain("R. Landauer");
  });

  it("extracts keys from Typst, LaTeX, and Markdown texts", () => {
    const typst = "As proved in @landauer1961 and @berut2012, not user@domain.com";
    expect(extractKeysFromSource(typst, "typst")).toEqual(["landauer1961", "berut2012"]);

    const latex = "See \\cite{landauer1961, berut2012} and \\citep{shannon1948}.";
    expect(extractKeysFromSource(latex, "latex")).toEqual([
      "landauer1961",
      "berut2012",
      "shannon1948",
    ]);

    const md = "Discussion [@landauer1961; @berut2012] and @shannon1948.";
    expect(extractKeysFromSource(md, "markdown")).toEqual([
      "landauer1961",
      "berut2012",
      "shannon1948",
    ]);
  });

  it("verifies entries against mock registry (verified, mismatch, not_found)", async () => {
    const { api } = createTestContext();
    await api.load({ path: "references.bib" });

    const verified = await api.verify("landauer1961");
    expect(verified.status).toBe("verified");
    expect(verified.differences).toEqual([]);

    const mismatch = await api.verify("mismatch2012");
    expect(mismatch.status).toBe("mismatch");
    expect(mismatch.differences.length).toBeGreaterThan(0);

    const notFound = await api.verify("notfound_entry");
    expect(notFound.status).toBe("not_found");
  });
});
