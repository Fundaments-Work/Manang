/**
 * Search: text matches inside a document, and file-name matches.
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
