/**
 * Incremental checking: never the whole document, never twice.
 *
 * A check works on the paragraphs (blank-line separated runs of lines) that
 * touch the visible range. Each paragraph is cached by a hash of its text, so
 * scrolling back, undoing, or typing elsewhere costs no request, and an edit
 * costs one paragraph. The cache is a small LRU, so memory stays flat however
 * long the document is. Only the paragraphs missing from the cache are sent,
 * in one request.
 */

import type { Text } from "@codemirror/state";

import type { ScalarRange } from "$lib/kernel/api";
import { scalarLength } from "$lib/kernel/offsets";

import { ScalarIndex } from "./offsets";

export interface Chunk {
  readonly from: number;
  readonly to: number;
  readonly text: string;
  readonly key: string;
}

/** An item placed inside a chunk, in UTF-16 offsets relative to the chunk. */
export interface RelItem {
  readonly start: number;
  readonly end: number;
}

export const MAX_CHUNK_CHARS = 6000;
export const MAX_CHUNKS = 120;
/** How far above the visible range a paragraph may begin, for markup context. */
const CONTEXT_LINES = 30;

/** 32-bit FNV-1a. */
export function hashString(text: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

export function chunkKey(syntax: string, text: string): string {
  return `${syntax}|${text.length}|${hashString(text)}`;
}

/**
 * The paragraphs overlapping `ranges`, in document order. Lines longer than
 * `maxChars` are skipped (minified output is not prose), and so is everything
 * beyond `maxChunks` paragraphs.
 */
export function collectChunks(
  doc: Text,
  ranges: readonly { from: number; to: number }[],
  syntax: string,
  maxChars = MAX_CHUNK_CHARS,
  maxChunks = MAX_CHUNKS,
): Chunk[] {
  const blank = (n: number): boolean => doc.line(n).text.trim() === "";

  // Line spans, each begun at the top of its paragraph (bounded) so fences and
  // math have their context, then merged where they touch.
  const spans: { first: number; last: number }[] = [];
  for (const range of ranges) {
    let first = doc.lineAt(range.from).number;
    const last = doc.lineAt(Math.min(range.to, doc.length)).number;
    for (let up = 0; first > 1 && up < CONTEXT_LINES && !blank(first - 1); up += 1) {
      first -= 1;
    }
    spans.push({ first, last });
  }
  spans.sort((a, b) => a.first - b.first);
  const merged: { first: number; last: number }[] = [];
  for (const span of spans) {
    const prev = merged[merged.length - 1];
    if (prev !== undefined && span.first <= prev.last + 1) {
      prev.last = Math.max(prev.last, span.last);
    } else {
      merged.push({ ...span });
    }
  }

  const chunks: Chunk[] = [];
  let from = -1;
  let to = -1;
  const flush = (): void => {
    if (from < 0) return;
    const text = doc.sliceString(from, to);
    chunks.push({ from, to, text, key: chunkKey(syntax, text) });
    from = -1;
  };

  for (const span of merged) {
    for (let n = span.first; n <= span.last && chunks.length < maxChunks; n += 1) {
      const line = doc.line(n);
      if (line.length > maxChars || blank(n)) {
        flush();
        continue;
      }
      if (from >= 0 && line.to - from > maxChars) flush();
      if (from < 0) from = line.from;
      to = line.to;
    }
    flush();
  }
  return chunks.slice(0, maxChunks);
}

/** Least-recently-used map with a hard entry limit. */
export class Lru<V> {
  readonly #map = new Map<string, V>();
  constructor(readonly limit: number) {}

  get(key: string): V | undefined {
    const value = this.#map.get(key);
    if (value !== undefined) {
      this.#map.delete(key);
      this.#map.set(key, value);
    }
    return value;
  }

  set(key: string, value: V): void {
    this.#map.delete(key);
    this.#map.set(key, value);
    if (this.#map.size > this.limit) {
      const oldest = this.#map.keys().next();
      if (oldest.done !== true) this.#map.delete(oldest.value);
    }
  }

  clear(): void {
    this.#map.clear();
  }

  get size(): number {
    return this.#map.size;
  }
}

export interface ResolveOptions<R, T extends RelItem> {
  readonly chunks: readonly Chunk[];
  readonly cache: Lru<readonly T[]>;
  /** Sends one text, returns raw findings in scalar offsets over that text. */
  readonly request: (text: string) => Promise<readonly R[]>;
  readonly range: (raw: R) => ScalarRange | null;
  readonly make: (raw: R, start: number, end: number) => T;
}

/**
 * Findings per chunk. Cached chunks cost nothing; the rest go out together,
 * separated by a blank line (which no rule reads across), and their findings
 * are split back by scalar offset and converted to chunk-relative UTF-16.
 */
export async function resolveChunks<R, T extends RelItem>(
  options: ResolveOptions<R, T>,
): Promise<(readonly T[])[]> {
  const { chunks, cache } = options;
  const misses = new Map<string, Chunk>();
  for (const chunk of chunks) {
    if (cache.get(chunk.key) === undefined && !misses.has(chunk.key)) {
      misses.set(chunk.key, chunk);
    }
  }

  if (misses.size > 0) {
    const list = [...misses.values()];
    const starts: number[] = [];
    let cursor = 0;
    for (const chunk of list) {
      starts.push(cursor);
      cursor += scalarLength(chunk.text) + 2;
    }
    const raws = await options.request(list.map((c) => c.text).join("\n\n"));

    const found: T[][] = list.map(() => []);
    const indexes = list.map((c) => new ScalarIndex(c.text));
    for (const raw of raws) {
      const range = options.range(raw);
      if (range === null) continue;
      let at = starts.length - 1;
      while (at > 0 && (starts[at] ?? 0) > range.start) at -= 1;
      const base = starts[at];
      const index = indexes[at];
      const bucket = found[at];
      const chunk = list[at];
      if (base === undefined || index === undefined || bucket === undefined) continue;
      if (chunk === undefined) continue;
      const start = index.toUtf16(range.start - base);
      const end = index.toUtf16(range.end - base);
      if (end > start && end <= chunk.text.length) {
        bucket.push(options.make(raw, start, end));
      }
    }
    list.forEach((chunk, i) => cache.set(chunk.key, found[i] ?? []));
  }

  return chunks.map((chunk) => cache.get(chunk.key) ?? []);
}
