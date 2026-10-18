import { describe, expect, it } from "vitest";

import {
  flattenOutline,
  isSafeExternal,
  linkBox,
  linkTarget,
  resolveDest,
  type DestLookup,
} from "./links";

const VIEW = [0, 0, 400, 600];
const REF_A = { num: 10, gen: 0 };
const REF_B = { num: 12, gen: 0 };

const lookup: DestLookup = {
  getDestination: (name) =>
    Promise.resolve(name === "method" ? [REF_B, { name: "XYZ" }, 40, 565, 0] : null),
  getPageIndex: (ref) => Promise.resolve(ref.num === 10 ? 0 : 1),
  view: (page) => (page < 2 ? VIEW : null),
};

describe("link targets", () => {
  it("external links: http, https and mail only", () => {
    expect(linkTarget({ subtype: "Link", url: "https://typst.app/" })).toEqual({
      kind: "external",
      url: "https://typst.app/",
    });
    expect(linkTarget({ subtype: "Link", unsafeUrl: "mailto:a@b.c" })).toEqual({
      kind: "external",
      url: "mailto:a@b.c",
    });
    expect(
      linkTarget({ subtype: "Link", unsafeUrl: "javascript:alert(1)" }),
    ).toBeNull();
    expect(linkTarget({ subtype: "Link", unsafeUrl: "file:///etc/passwd" })).toBeNull();
    expect(isSafeExternal("not a url")).toBe(false);
  });

  it("internal links: named and explicit destinations", () => {
    expect(linkTarget({ subtype: "Link", dest: "method" })).toEqual({
      kind: "internal",
      dest: "method",
    });
    const explicit = [REF_A, { name: "Fit" }];
    expect(linkTarget({ subtype: "Link", dest: explicit })).toEqual({
      kind: "internal",
      dest: explicit,
    });
  });

  it("ignores other annotations and empty links", () => {
    expect(linkTarget({ subtype: "Widget", url: "https://x.y" })).toBeNull();
    expect(linkTarget({ subtype: "Link" })).toBeNull();
    expect(linkTarget({ subtype: "Link", dest: [] })).toBeNull();
  });
});

describe("link boxes", () => {
  it("turns a PDF rectangle into fractions of the page from the top-left", () => {
    const box = linkBox([100, 500, 200, 520], VIEW);
    expect(box?.left).toBeCloseTo(0.25);
    expect(box?.width).toBeCloseTo(0.25);
    expect(box?.top).toBeCloseTo(80 / 600);
    expect(box?.height).toBeCloseTo(20 / 600);
  });

  it("accepts reversed corners and an offset page box", () => {
    const box = linkBox([200, 520, 100, 500], [50, 50, 450, 650]);
    expect(box?.left).toBeCloseTo(50 / 400);
    expect(box?.top).toBeCloseTo(130 / 600);
  });

  it("rejects empty boxes", () => {
    expect(linkBox([1, 1, 1, 5], VIEW)).toBeNull();
    expect(linkBox([1, 1], VIEW)).toBeNull();
  });
});

describe("destinations", () => {
  it("a named XYZ destination: page and point from the top-left", async () => {
    expect(await resolveDest("method", lookup)).toEqual({ page: 1, x: 40, y: 35 });
  });

  it("explicit destinations of each fit type", async () => {
    expect(await resolveDest([REF_A, { name: "Fit" }], lookup)).toEqual({
      page: 0,
      x: null,
      y: null,
    });
    expect(await resolveDest([REF_B, { name: "FitH" }, 500], lookup)).toEqual({
      page: 1,
      x: null,
      y: 100,
    });
    expect(
      await resolveDest([REF_A, { name: "XYZ" }, null, null, null], lookup),
    ).toEqual({ page: 0, x: null, y: null });
    expect(
      await resolveDest([REF_A, { name: "FitR" }, 10, 20, 30, 590], lookup),
    ).toEqual({
      page: 0,
      x: 10,
      y: 10,
    });
  });

  it("a page given as an index", async () => {
    expect(await resolveDest([1, { name: "XYZ" }, 0, 600, 0], lookup)).toEqual({
      page: 1,
      x: 0,
      y: 0,
    });
  });

  it("leads nowhere when the name or the page is unknown", async () => {
    expect(await resolveDest("nope", lookup)).toBeNull();
    expect(await resolveDest(["x", { name: "XYZ" }], lookup)).toBeNull();
    expect(await resolveDest([], lookup)).toBeNull();
  });
});

describe("outline", () => {
  it("flattens nested entries with their depth", () => {
    const entries = flattenOutline([
      {
        title: "1. Introduction",
        dest: "intro",
        url: null,
        items: [{ title: "1.1  Scope\n", dest: "scope", url: null, items: [] }],
      },
      { title: "", dest: null, url: "https://x.y", items: [] },
    ]);
    expect(entries.map((e) => [e.id, e.title, e.depth])).toEqual([
      ["0", "1. Introduction", 0],
      ["0.0", "1.1 Scope", 1],
      ["1", "Untitled", 0],
    ]);
  });

  it("no outline gives no entries", () => {
    expect(flattenOutline(null)).toEqual([]);
  });

  it("stops at the depth limit", () => {
    const deep = {
      title: "a",
      dest: null,
      url: null,
      items: [{ title: "b", dest: null, url: null, items: [] }],
    };
    expect(flattenOutline([deep], 1)).toHaveLength(1);
  });
});
