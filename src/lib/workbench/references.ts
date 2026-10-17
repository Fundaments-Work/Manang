/**
 * Bibliography entries for the References panel.
 *
 * A citations plugin is coming and will own parsing, de-duplication and
 * verification. Until then `BibSource` is the seam: the panel depends on the
 * interface, and this file ships a small BibTeX reader behind it. It handles
 * nested braces, quoted values and common LaTeX accents; it does not expand
 * `@string` macros or cross-references.
 */

import { fuzzyRank } from "$lib/commands/fuzzy";

import type { Engine } from "./paths";

export interface BibEntry {
  readonly key: string;
  readonly type: string;
  readonly title: string;
  readonly authors: readonly string[];
  readonly year: string;
  readonly venue: string;
  /** Bibliography file the entry came from. */
  readonly file: string;
}

/** Where entries come from. Swap for the citations plugin when it lands. */
export interface BibSource {
  load(): Promise<BibEntry[]>;
}

const ACCENTS: Record<string, string> = {
  "'": "́",
  "`": "̀",
  '"': "̈",
  "^": "̂",
  "~": "̃",
};

function clean(value: string): string {
  return value
    .replace(/\\(['`"^~])\s*\{?([A-Za-z])\}?/g, (_m, mark: string, letter: string) =>
      `${letter}${ACCENTS[mark] ?? ""}`.normalize("NFC"),
    )
    .replace(/\\&/g, "&")
    .replace(/[{}]/g, "")
    .replace(/--+/g, "–")
    .replace(/\s+/g, " ")
    .trim();
}

/** Value starting at `index` (a `{…}` group, a `"…"` string, or a bare token). */
function readValue(text: string, index: number): { value: string; end: number } {
  const open = text[index];
  if (open === "{") {
    let depth = 0;
    for (let i = index; i < text.length; i += 1) {
      if (text[i] === "{") depth += 1;
      else if (text[i] === "}") {
        depth -= 1;
        if (depth === 0) return { value: text.slice(index + 1, i), end: i + 1 };
      }
    }
    return { value: text.slice(index + 1), end: text.length };
  }
  if (open === '"') {
    let i = index + 1;
    while (i < text.length && !(text[i] === '"' && text[i - 1] !== "\\")) i += 1;
    return { value: text.slice(index + 1, i), end: i + 1 };
  }
  const bare = /^[^,}\s]*/.exec(text.slice(index));
  const value = bare?.[0] ?? "";
  return { value, end: index + value.length };
}

function splitAuthors(raw: string): string[] {
  return raw
    .split(/\s+and\s+/i)
    .map((name) => {
      const cleaned = clean(name);
      const comma = cleaned.indexOf(",");
      return comma === -1 ? cleaned : `${cleaned.slice(comma + 1).trim()} ${cleaned.slice(0, comma).trim()}`;
    })
    .filter((name) => name.length > 0);
}

export function parseBibtex(text: string, file = ""): BibEntry[] {
  const entries: BibEntry[] = [];
  const head = /@([A-Za-z]+)\s*\{\s*([^,\s{}]+)\s*,/g;
  for (let match = head.exec(text); match !== null; match = head.exec(text)) {
    const type = (match[1] ?? "").toLowerCase();
    if (type === "comment" || type === "string" || type === "preamble") continue;
    const fields = new Map<string, string>();
    let i = head.lastIndex;
    while (i < text.length) {
      const field = /^\s*([A-Za-z][\w-]*)\s*=\s*/.exec(text.slice(i));
      if (field === null) break;
      const { value, end } = readValue(text, i + field[0].length);
      fields.set((field[1] ?? "").toLowerCase(), value);
      i = end;
      const comma = /^\s*,/.exec(text.slice(i));
      if (comma === null) break;
      i += comma[0].length;
    }
    head.lastIndex = Math.max(head.lastIndex, i);
    entries.push({
      key: match[2] ?? "",
      type,
      title: clean(fields.get("title") ?? ""),
      authors: splitAuthors(fields.get("author") ?? ""),
      year: clean(fields.get("year") ?? fields.get("date") ?? "").slice(0, 4),
      venue: clean(fields.get("journal") ?? fields.get("booktitle") ?? fields.get("publisher") ?? ""),
      file,
    });
  }
  return entries;
}

/** Fuzzy filter across key, authors, title and year. */
export function filterReferences(entries: readonly BibEntry[], query: string): BibEntry[] {
  return fuzzyRank(
    entries,
    query,
    (e) => `${e.key} ${e.authors.join(" ")} ${e.title} ${e.year}`,
  ).map((ranked) => ranked.item);
}

/** "Landauer", "Bérut and Arakelyan", "Nielsen et al." */
export function authorSummary(authors: readonly string[]): string {
  const last = (name: string): string => name.split(" ").pop() ?? name;
  if (authors.length === 0) return "Unknown author";
  if (authors.length === 1) return last(authors[0] ?? "");
  if (authors.length === 2) return `${last(authors[0] ?? "")} and ${last(authors[1] ?? "")}`;
  return `${last(authors[0] ?? "")} et al.`;
}

/** The text that cites `key` in the given engine's syntax. */
export function citationFor(key: string, engine: Engine | null): string {
  switch (engine) {
    case "latex":
      return `\\cite{${key}}`;
    case "markdown":
      return `[@${key}]`;
    default:
      return `@${key}`;
  }
}
