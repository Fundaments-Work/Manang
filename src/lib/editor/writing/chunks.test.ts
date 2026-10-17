import { Text } from "@codemirror/state";
import { describe, expect, it, vi } from "vitest";

import { writingApi, type Misspelling } from "$lib/kernel/api";
import { createMockKernel } from "$lib/kernel/mock";
import { writingPlugin } from "$lib/kernel/mockWriting";

import {
  chunkKey,
  collectChunks,
  hashString,
  Lru,
  resolveChunks,
  type RelItem,
} from "./chunks";

const doc = (text: string): Text => Text.of(text.split("\n"));
const whole = (d: Text) => [{ from: 0, to: d.length }];

describe("collectChunks", () => {
  it("splits paragraphs at blank lines", () => {
    const d = doc("one\ntwo\n\nthree\n\n\nfour");
    const chunks = collectChunks(d, whole(d), "typst");
    expect(chunks.map((c) => c.text)).toEqual(["one\ntwo", "three", "four"]);
    expect(chunks.map((c) => d.sliceString(c.from, c.to))).toEqual(
      chunks.map((c) => c.text),
    );
  });

  it("only looks at the visible lines, begun at their paragraph's top", () => {
    const lines = Array.from({ length: 1000 }, (_, i) => `line ${i}`);
    lines[500] = "";
    const d = doc(lines.join("\n"));
    const from = d.line(502).from;
    const to = d.line(504).to;
    const chunks = collectChunks(d, [{ from, to }], "typst");
    expect(chunks).toHaveLength(1);
    expect(chunks[0]?.text.startsWith("line 501")).toBe(true);
    expect(chunks[0]?.text.endsWith("line 503")).toBe(true);
  });

  it("merges overlapping ranges instead of repeating a paragraph", () => {
    const d = doc("a\nb\nc\nd");
    const chunks = collectChunks(
      d,
      [
        { from: 0, to: 3 },
        { from: 2, to: 7 },
      ],
      "typst",
    );
    expect(chunks.map((c) => c.text)).toEqual(["a\nb\nc\nd"]);
  });

  it("splits oversized paragraphs and skips absurdly long lines", () => {
    const d = doc(
      `${"word ".repeat(30)}\n${"word ".repeat(30)}\n${"x".repeat(500)}\nshort`,
    );
    const chunks = collectChunks(d, whole(d), "typst", 200);
    expect(chunks.map((c) => c.text.length)).toEqual([150, 150, 5]);
  });

  it("caps how many paragraphs one pass can ask about", () => {
    const d = doc(Array.from({ length: 500 }, (_, i) => `p${i}`).join("\n\n"));
    expect(collectChunks(d, whole(d), "typst", 6000, 40)).toHaveLength(40);
  });

  it("keys chunks by syntax and content", () => {
    expect(chunkKey("typst", "abc")).not.toBe(chunkKey("markdown", "abc"));
    expect(chunkKey("typst", "abc")).not.toBe(chunkKey("typst", "abd"));
    expect(hashString("abc")).toBe(hashString("abc"));
  });
});

describe("Lru", () => {
  it("evicts the least recently used entry beyond its limit", () => {
    const lru = new Lru<number>(2);
    lru.set("a", 1);
    lru.set("b", 2);
    expect(lru.get("a")).toBe(1);
    lru.set("c", 3);
    expect(lru.get("b")).toBeUndefined();
    expect(lru.size).toBe(2);
  });

  it("stays bounded for a very long document's worth of paragraphs", () => {
    const lru = new Lru<number>(400);
    for (let i = 0; i < 100_000; i += 1) lru.set(`k${i}`, i);
    expect(lru.size).toBe(400);
  });
});

function spellApi() {
  const kernel = createMockKernel({ plugins: [writingPlugin()] });
  const run = vi.spyOn(kernel, "run");
  return { api: writingApi(kernel), run };
}

const spellOptions = (api: ReturnType<typeof writingApi>) => ({
  request: async (text: string) => (await api.spellCheck(text, "typst")).misspelled,
  range: (m: Misspelling) => m.range,
  make: (_m: Misspelling, start: number, end: number): RelItem => ({ start, end }),
});

describe("resolveChunks", () => {
  it("maps Rust scalar offsets to UTF-16 inside each chunk (emoji, combining marks)", async () => {
    const text = "😀 teh start\n\ncafé and 𝔸𝔹 wich more\n\nclean";
    const d = doc(text);
    const chunks = collectChunks(d, whole(d), "typst");
    const { api } = spellApi();
    const found = await resolveChunks({
      chunks,
      cache: new Lru<readonly RelItem[]>(10),
      ...spellOptions(api),
    });
    const words = chunks.flatMap((chunk, i) =>
      (found[i] ?? []).map((item) =>
        d.sliceString(chunk.from + item.start, chunk.from + item.end),
      ),
    );
    expect(words).toEqual(["teh", "wich"]);
  });

  it("asks only about paragraphs it has not seen, in one request", async () => {
    const d = doc("teh one\n\nclean two\n\nwich three");
    const chunks = collectChunks(d, whole(d), "typst");
    const { api, run } = spellApi();
    const cache = new Lru<readonly RelItem[]>(10);
    await resolveChunks({ chunks, cache, ...spellOptions(api) });
    expect(run).toHaveBeenCalledTimes(1);

    // Same paragraphs again (scrolling back): no request.
    await resolveChunks({ chunks, cache, ...spellOptions(api) });
    expect(run).toHaveBeenCalledTimes(1);

    // One paragraph edited: exactly one paragraph is sent.
    const edited = doc("teh one\n\nclean two\n\nwich thre");
    const next = collectChunks(edited, whole(edited), "typst");
    await resolveChunks({ chunks: next, cache, ...spellOptions(api) });
    expect(run).toHaveBeenCalledTimes(2);
    const sent = run.mock.calls[1]?.[1]?.["text"];
    expect(sent).toBe("wich thre");
    expect(sent).not.toContain("teh");
  });

  it("sends no request at all when everything is cached", async () => {
    const d = doc("alpha\n\nbeta");
    const chunks = collectChunks(d, whole(d), "typst");
    const request = vi.fn(() => Promise.resolve([] as Misspelling[]));
    const cache = new Lru<readonly RelItem[]>(10);
    const options = {
      chunks,
      cache,
      request,
      range: (m: Misspelling) => m.range,
      make: (_m: Misspelling, start: number, end: number): RelItem => ({ start, end }),
    };
    await resolveChunks(options);
    await resolveChunks(options);
    expect(request).toHaveBeenCalledTimes(1);
  });

  it("deduplicates identical paragraphs within one request", async () => {
    const d = doc("teh\n\nteh\n\nteh");
    const chunks = collectChunks(d, whole(d), "typst");
    const { api, run } = spellApi();
    const found = await resolveChunks({
      chunks,
      cache: new Lru<readonly RelItem[]>(10),
      ...spellOptions(api),
    });
    expect(run.mock.calls[0]?.[1]?.["text"]).toBe("teh");
    expect(found.map((f) => f.length)).toEqual([1, 1, 1]);
  });
});
