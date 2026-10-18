/**
 * The Typst preview PDF read the way the viewer reads it (pdf.js, here its
 * Node build), against `testdata/typst-preview/`, which `crates/manang-typst`
 * writes from a real compile (`write_preview_fixtures`). Checks that a point
 * in the viewer's page coordinates (points from the top-left) is the point
 * `typst.jump-from-cursor` and `typst.jump-from-click` speak, and that the
 * outline, links and text search work on real Typst output.
 */

import { readFileSync } from "node:fs";

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import type { PDFDocumentProxy } from "pdfjs-dist";

import { findMatches, pageText, runsOf } from "./find";
import { flattenOutline, linkTarget, resolveDest, type OutlineNode } from "./links";

interface Fixture {
  readonly pageSizes: readonly [number, number][];
  readonly jumps: readonly { line: number; page: number; x: number; y: number }[];
}

const fixture = JSON.parse(
  readFileSync("testdata/typst-preview/fixture.json", "utf8"),
) as Fixture;

let doc: PDFDocumentProxy;
let task: { destroy(): Promise<void> } | undefined;
const views: number[][] = [];

beforeAll(async () => {
  const pdfjs = await import("pdfjs-dist/legacy/build/pdf.mjs");
  const data = new Uint8Array(readFileSync("testdata/typst-preview/fixture.pdf"));
  const loading = pdfjs.getDocument({
    data,
    useWorkerFetch: false,
  });
  task = loading;
  doc = (await loading.promise) as unknown as PDFDocumentProxy;
  for (let i = 1; i <= doc.numPages; i += 1) views.push((await doc.getPage(i)).view);
});

afterAll(async () => {
  await task?.destroy();
});

describe("the Typst preview PDF", () => {
  it("has the compiled pages at the compiled size", () => {
    expect(doc.numPages).toBe(fixture.pageSizes.length);
    fixture.pageSizes.forEach(([w, h], i) => {
      const [x0, y0, x1, y1] = views[i] ?? [];
      expect(x0).toBe(0);
      expect(y0).toBe(0);
      expect(x1).toBeCloseTo(w, 3);
      expect(y1).toBeCloseTo(h, 3);
    });
  });

  it("puts each heading where typst.jump-from-cursor says it is", async () => {
    for (const [index, word] of [
      [0, "Introduction"],
      [1, "Method"],
    ] as const) {
      const jump = fixture.jumps[index];
      if (jump === undefined) throw new Error("fixture jumps");
      const page = await doc.getPage(jump.page + 1);
      const content = await page.getTextContent();
      const item = content.items.find((i) => "str" in i && i.str === word);
      if (item === undefined || !("transform" in item)) throw new Error(`no ${word}`);
      const view = views[jump.page] ?? [0, 0, 0, 0];
      // The viewer's page point for the start of the heading's baseline.
      const x = (item.transform[4] as number) - (view[0] ?? 0);
      const y = (view[3] ?? 0) - (item.transform[5] as number);
      expect(x).toBeCloseTo(jump.x, 1);
      expect(y).toBeCloseTo(jump.y, 1);
    }
  });

  it("has an outline from the headings", async () => {
    const outline = flattenOutline((await doc.getOutline()) as OutlineNode[]);
    expect(outline.map((e) => e.title)).toEqual(["1. Introduction", "2. Method"]);
    const dest = outline[1]?.dest;
    if (dest === undefined || dest === null) throw new Error("no dest");
    expect((await resolveDest(dest, lookup()))?.page).toBe(1);
  });

  it("has an internal link to the method section and an external one", async () => {
    const page = await doc.getPage(1);
    const targets = (await page.getAnnotations()).map(linkTarget);
    const internal = targets.find((t) => t?.kind === "internal");
    const external = targets.find((t) => t?.kind === "external");
    expect(external).toEqual({ kind: "external", url: "https://typst.app/" });
    if (internal?.kind !== "internal") throw new Error("no internal link");
    const to = await resolveDest(internal.dest, lookup());
    expect(to?.page).toBe(1);
    // The destination is at the Method heading, above its baseline.
    const heading = fixture.jumps[1];
    expect(to?.y).not.toBeNull();
    expect(to?.y ?? 0).toBeLessThan(heading?.y ?? 0);
    expect((heading?.y ?? 0) - (to?.y ?? 0)).toBeLessThan(30);
  });

  it("finds text across the document", async () => {
    const pages = [];
    for (let i = 0; i < doc.numPages; i += 1) {
      const content = await (await doc.getPage(i + 1)).getTextContent();
      pages.push(pageText(runsOf(content.items, views[i] ?? [])));
    }
    const matches = findMatches(pages, "room temperature", { caseSensitive: false });
    expect(matches).toHaveLength(1);
    expect(matches[0]?.page).toBe(1);
    expect(findMatches(pages, "the", { caseSensitive: false }).length).toBeGreaterThan(
      2,
    );
  });
});

function lookup() {
  return {
    getDestination: (name: string) => doc.getDestination(name),
    getPageIndex: (ref: { num: number; gen: number }) => doc.getPageIndex(ref),
    view: (page: number) => views[page] ?? null,
  };
}
