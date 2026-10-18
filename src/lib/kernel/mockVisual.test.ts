import { describe, expect, it } from "vitest";

import { visualApi } from "$lib/editor/visual/client";
import { SPAN_KIND } from "$lib/editor/visual/protocol";

import { createMockKernel } from "./mock";
import { plan, visualPlugin } from "./mockVisual";

function setup() {
  const kernel = createMockKernel({ plugins: [visualPlugin] });
  return { kernel, api: visualApi(kernel) };
}

function apply(
  text: string,
  changes: readonly { from: number; to: number; insert: string }[],
) {
  let out = text;
  for (const c of [...changes].reverse())
    out = out.slice(0, c.from) + c.insert + out.slice(c.to);
  return out;
}

describe("mock visual plugin", () => {
  it("opens by extension and returns a valid table for Markdown", async () => {
    const { api } = setup();
    const text = "# Title\n\nSome **bold** and *it* and `code`.\n\n- one\n- two\n";
    const open = await api.open({ text, path: "a.md" });
    expect(open).toMatchObject({ adapter: "markdown", revision: 0, len: text.length });
    const result = await api.blocks({
      session: open.session,
      from: 0,
      to: text.length,
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const kinds = result.table.spans.map((s) => s.kind);
    expect(kinds).toContain(SPAN_KIND.heading);
    expect(kinds).toContain(SPAN_KIND.strong);
    expect(kinds).toContain(SPAN_KIND.emph);
    expect(kinds).toContain(SPAN_KIND.code);
    expect(kinds.filter((k) => k === SPAN_KIND.listItem)).toHaveLength(2);
    const heading = result.table.spans[0];
    expect(heading).toMatchObject({ kind: SPAN_KIND.heading, aux: 1, layer: 0 });
    // The heading's marker is its child and covers `# `.
    expect(result.table.spans[1]).toMatchObject({
      kind: SPAN_KIND.marker,
      start: 0,
      end: 2,
      layer: 1,
    });
  });

  it("slices to the viewport, keeping whole spans", async () => {
    const { api } = setup();
    const text = "# A\n\nparagraph with **strong text** here\n\n# B\n";
    const { session } = await api.open({ text, adapter: "markdown" });
    const strongStart = text.indexOf("**");
    const result = await api.blocks({
      session,
      from: strongStart + 4,
      to: strongStart + 6,
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const strong = result.table.spans.find((s) => s.kind === SPAN_KIND.strong);
    expect(strong?.start).toBe(strongStart);
    expect(result.table.spans.some((s) => s.kind === SPAN_KIND.heading)).toBe(false);
  });

  it("reads Typst headings, emphasis and opaque directives", async () => {
    const { api } = setup();
    const text =
      '#set page(paper: "a4")\n\n= Intro\n\nSome *bold* and _it_ text with $x^2$.\n';
    const { session } = await api.open({ text, path: "m.typ" });
    const result = await api.blocks({ session, from: 0, to: text.length });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const spans = result.table.spans;
    expect(spans[0]).toMatchObject({ kind: SPAN_KIND.opaque, attr: "set" });
    expect(spans.some((s) => s.kind === SPAN_KIND.heading && s.aux === 1)).toBe(true);
    expect(spans.some((s) => s.kind === SPAN_KIND.strong)).toBe(true);
    expect(spans.some((s) => s.kind === SPAN_KIND.math && s.attr === "typst")).toBe(
      true,
    );
  });

  it("applies edits, bumps the revision and detects a desync", async () => {
    const { api } = setup();
    const { session } = await api.open({ text: "abc", adapter: "markdown" });
    const edited = await api.edit({
      session,
      changes: [{ from: 1, to: 2, insert: "XYZ" }],
      len: 5,
    });
    expect(edited).toMatchObject({ revision: 1, len: 5 });
    expect(edited.dirty).not.toBeNull();
    await expect(api.edit({ session, changes: [], len: 99 })).rejects.toMatchObject({
      message: expect.stringContaining("[visual.desync]") as unknown,
    });
    const reset = await api.reset({ session, text: "hello" });
    expect(reset).toMatchObject({ len: 5 });
    expect(reset.revision).toBeGreaterThan(edited.revision);
  });

  it("reports no-session after close", async () => {
    const { api } = setup();
    const { session } = await api.open({ text: "x", adapter: "typst" });
    await api.close(session);
    await expect(api.outline(session)).rejects.toMatchObject({
      message: expect.stringContaining("[visual.no-session]") as unknown,
    });
  });
});

describe("mock edit plans", () => {
  const md = (text: string) => ({ format: "markdown" as const, text });
  const typ = (text: string) => ({ format: "typst" as const, text });

  it("wraps and unwraps bold around a selection", () => {
    const wrapped = plan(md("a word here"), "bold", 2, 6);
    expect(apply("a word here", wrapped.changes)).toBe("a **word** here");
    expect(wrapped.selection).toEqual({ from: 4, to: 8 });
    const text = "a **word** here";
    const unwrapped = plan(md(text), "bold", 4, 8);
    expect(apply(text, unwrapped.changes)).toBe("a word here");
    expect(unwrapped.selection).toEqual({ from: 2, to: 6 });
  });

  it("wraps the word at a collapsed caret, and inserts a pair in empty space", () => {
    const wrapped = plan(md("hello world"), "italic", 8, 8);
    expect(apply("hello world", wrapped.changes)).toBe("hello *world*");
    const pair = plan(md("a  b"), "bold", 2, 2);
    expect(apply("a  b", pair.changes)).toBe("a **** b");
    expect(pair.selection).toEqual({ from: 4, to: 4 });
  });

  it("uses Typst marks for Typst", () => {
    expect(apply("one two", plan(typ("one two"), "bold", 0, 3).changes)).toBe(
      "*one* two",
    );
    expect(apply("one two", plan(typ("one two"), "italic", 4, 7).changes)).toBe(
      "one _two_",
    );
  });

  it("sets and clears heading levels on the touched lines", () => {
    expect(apply("Title", plan(md("Title"), "heading", 0, 0, "2").changes)).toBe(
      "## Title",
    );
    expect(apply("# Title", plan(md("# Title"), "heading", 3, 3, "0").changes)).toBe(
      "Title",
    );
    expect(apply("# Title", plan(md("# Title"), "heading", 3, 3, "3").changes)).toBe(
      "### Title",
    );
    expect(apply("Title", plan(typ("Title"), "heading", 0, 0, "2").changes)).toBe(
      "== Title",
    );
  });

  it("toggles lists and quotes", () => {
    const text = "one\ntwo";
    const listed = apply(
      text,
      plan(md(text), "list", 0, text.length, "bullet").changes,
    );
    expect(listed).toBe("- one\n- two");
    expect(
      apply(listed, plan(md(listed), "list", 0, listed.length, "bullet").changes),
    ).toBe(text);
    expect(apply(text, plan(md(text), "list", 0, text.length, "ordered").changes)).toBe(
      "1. one\n2. two",
    );
    expect(apply(text, plan(md(text), "quote", 0, text.length).changes)).toBe(
      "> one\n> two",
    );
  });

  it("builds links, images and math", () => {
    const link = plan(md("see page"), "link", 4, 8, "https://x.org");
    expect(apply("see page", link.changes)).toBe("see [page](https://x.org)");
    expect(link.selection).toEqual({ from: 5, to: 9 });
    expect(apply("", plan(md(""), "image", 0, 0, "f.svg").changes)).toBe("![](f.svg)");
    expect(apply("", plan(typ(""), "figure", 0, 0, "f.svg").changes)).toBe(
      '#figure(image("f.svg"), caption: [])',
    );
    expect(apply("", plan(md(""), "math", 0, 0, "inline").changes)).toBe("$x$");
    expect(apply("", plan(typ(""), "math", 0, 0, "block").changes)).toBe("$ x $");
  });
});
