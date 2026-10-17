/**
 * Search: text matches inside a document, file-name matches, and range splitting.
 */

import { fuzzyRank } from "$lib/commands/fuzzy";

export interface TextMatch {
  /** 1-based. */
  readonly line: number;
  /** 1-based, in UTF-16 units (the editor's addressing). */
  readonly column: number;
  readonly length: number;
  /** The line, trimmed, split around the match for highlighting. */
  readonly before: string;
  readonly hit: string;
  readonly after: string;
}

export interface LineSegment {
  readonly text: string;
  readonly isMatch: boolean;
}

export const MAX_MATCHES_PER_FILE = 100;
const CONTEXT = 36;

/** Case-insensitive literal search, line by line. */
export function searchText(
  text: string,
  query: string,
  limit = MAX_MATCHES_PER_FILE,
): TextMatch[] {
  const needle = query.toLowerCase();
  if (needle === "") return [];
  const matches: TextMatch[] = [];
  const lines = text.split("\n");
  for (let i = 0; i < lines.length && matches.length < limit; i += 1) {
    const line = lines[i] ?? "";
    const lower = line.toLowerCase();
    let from = 0;
    for (
      let at = lower.indexOf(needle, from);
      at !== -1;
      at = lower.indexOf(needle, from)
    ) {
      const start = Math.max(0, at - CONTEXT);
      matches.push({
        line: i + 1,
        column: at + 1,
        length: needle.length,
        before: `${start > 0 ? "…" : ""}${line.slice(start, at).replace(/^\s+/, "")}`,
        hit: line.slice(at, at + needle.length),
        after: line.slice(at + needle.length, at + needle.length + CONTEXT * 2),
      });
      from = at + needle.length;
      if (matches.length >= limit) break;
    }
  }
  return matches;
}

export function searchNames(
  paths: readonly string[],
  query: string,
  limit = 50,
): string[] {
  if (query.trim() === "") return [];
  return fuzzyRank(paths, query, (p) => p)
    .slice(0, limit)
    .map((r) => r.item);
}

/**
 * Split a matched line into non-matching and matching segments using the
 * returned Unicode-scalar ranges. Handles UTF-16 surrogate pairs cleanly.
 */
export function splitLineByRanges(
  line: string,
  ranges: readonly { readonly start: number; readonly end: number }[],
  textOffset = 0,
): LineSegment[] {
  if (ranges.length === 0) {
    return [{ text: line, isMatch: false }];
  }

  // Convert scalar ranges into indices on `chars` array
  const chars = Array.from(line);
  const segments: LineSegment[] = [];
  let cursor = 0;

  // Sort ranges by start
  const sorted = [...ranges].sort((a, b) => a.start - b.start);

  for (const r of sorted) {
    const relStart = Math.max(0, r.start - textOffset);
    const relEnd = Math.min(chars.length, r.end - textOffset);

    if (relStart > cursor) {
      segments.push({
        text: chars.slice(cursor, relStart).join(""),
        isMatch: false,
      });
    }

    if (relEnd > relStart && relEnd > cursor) {
      const matchStart = Math.max(cursor, relStart);
      segments.push({
        text: chars.slice(matchStart, relEnd).join(""),
        isMatch: true,
      });
      cursor = relEnd;
    }
  }

  if (cursor < chars.length) {
    segments.push({
      text: chars.slice(cursor).join(""),
      isMatch: false,
    });
  }

  return segments.filter((s) => s.text.length > 0);
}

/** Parse comma, semicolon or newline-delimited glob pattern string. */
export function parsePatternList(input: string): string[] {
  return input
    .split(/[,;\n]/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/** Format matches and files summary string in sentence case. */
export function formatMatchCount(matches: number, files: number): string {
  const m = `${matches} ${matches === 1 ? "match" : "matches"}`;
  const f = `${files} ${files === 1 ? "file" : "files"}`;
  return `${m} in ${f}`;
}
