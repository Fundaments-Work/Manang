import { describe, expect, it } from "vitest";

import { ascii, textWidth, writePdf } from "./mockPdf";

async function open(bytes: Uint8Array) {
  const pdfjs = await import("pdfjs-dist/legacy/build/pdf.mjs");
  const loading = pdfjs.getDocument({ data: bytes.slice(), useWorkerFetch: false });
  return { loading, doc: await loading.promise };
}

describe("mock PDF writer", () => {
  it("writes pages pdf.js reads, with text where it was placed (top-left points)", async () => {
    const bytes = writePdf({
      width: 400,
      height: 600,
      pages: [
        { texts: [{ x: 50, y: 100, size: 12, text: "First (page) \\ text" }] },
        { texts: [{ x: 60, y: 200, size: 10, text: "Second", font: "mono" }] },
      ],
    });
    const { loading, doc } = await open(bytes);
    try {
      expect(doc.numPages).toBe(2);
      const page = await doc.getPage(1);
      expect(page.view).toEqual([0, 0, 400, 600]);
      const item = (await page.getTextContent()).items[0];
      if (item === undefined || !("str" in item)) throw new Error("no text");
      expect(item.str).toBe("First (page) \\ text");
      expect(item.transform[4]).toBeCloseTo(50);
      expect(600 - (item.transform[5] as number)).toBeCloseTo(100);
      expect(item.width).toBeCloseTo(textWidth("First (page) \\ text", 12), 0);
    } finally {
      await loading.destroy();
    }
  });

  it("writes links and an outline", async () => {
    const bytes = writePdf({
      width: 400,
      height: 600,
      pages: [
        {
          texts: [],
          links: [
            {
              rect: { x: 10, y: 20, width: 30, height: 10 },
              to: { url: "https://typst.app" },
            },
            { rect: { x: 10, y: 40, width: 30, height: 10 }, to: { page: 1, y: 100 } },
          ],
        },
        { texts: [] },
      ],
      outline: [
        { title: "One", page: 0, y: 0 },
        { title: "Two", page: 1, y: 100 },
      ],
    });
    const { loading, doc } = await open(bytes);
    try {
      const [web, inside] = await (await doc.getPage(1)).getAnnotations();
      expect(web?.url).toBe("https://typst.app/");
      expect(web?.rect).toEqual([10, 570, 40, 580]);
      const dest = inside?.dest as unknown[];
      expect(dest[1]).toEqual({ name: "XYZ" });
      expect(dest[3]).toBe(500);
      expect(await doc.getPageIndex(dest[0] as { num: number; gen: number })).toBe(1);
      const outline = await doc.getOutline();
      expect(outline?.map((o) => o.title)).toEqual(["One", "Two"]);
    } finally {
      await loading.destroy();
    }
  });

  it("measures Helvetica and Courier, and writes other characters as ?", () => {
    expect(textWidth("ii", 10)).toBeCloseTo(4.44);
    expect(textWidth("ii", 10, "mono")).toBeCloseTo(12);
    expect(ascii("β ≥ x")).toBe("? ? x");
  });
});
