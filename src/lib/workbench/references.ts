/**
 * Bibliography entries for the References panel.
 *
 * A citations plugin is coming and will own parsing, de-duplication and
 * verification. Until then `BibSource` is the seam: the panel depends on the
 * interface, and this file ships a small BibTeX reader behind it. It handles
 * nested braces, quoted values and common LaTeX accents; it does not expand
 * `@string` macros or cross-references.
 */

import type { Engine } from "./paths";

import type {
  CitationAuthor,
  CitationDifference,
  CitationEntry,
  CitationVerificationStatus,
} from "$lib/kernel/api";

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
      return comma === -1
        ? cleaned
        : `${cleaned.slice(comma + 1).trim()} ${cleaned.slice(0, comma).trim()}`;
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
      venue: clean(
        fields.get("journal") ??
          fields.get("booktitle") ??
          fields.get("publisher") ??
          "",
      ),
      file,
    });
  }
  return entries;
}

const fold = (value: string): string =>
  value
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();

/**
 * Filter across key, authors, title and year. Every word you type must appear
 * somewhere in the entry; this is stricter than the palette's fuzzy match on
 * purpose, because a reference list is read, not guessed at.
 */
export function filterReferences<T extends BibEntry | CitationEntry>(
  entries: readonly T[],
  query: string,
): T[] {
  const words = fold(query)
    .split(/\s+/)
    .filter((w) => w.length > 0);
  if (words.length === 0) return [...entries];
  return entries.filter((e) => {
    const authorText = "authors" in e && Array.isArray(e.authors)
      ? e.authors.map((a) => (typeof a === "string" ? a : a.name)).join(" ")
      : "";
    const venueText = "venue" in e && typeof e.venue === "string"
      ? e.venue
      : "journal_or_publisher" in e && typeof e.journal_or_publisher === "string"
        ? e.journal_or_publisher
        : "";
    const yearText = e.year !== undefined && e.year !== null ? String(e.year) : "";
    const titleText = e.title ?? "";

    const haystack = fold(
      `${e.key} ${authorText} ${titleText} ${yearText} ${venueText}`,
    );
    return words.every((word) => haystack.includes(word));
  });
}

function extractSurname(author: string | CitationAuthor): string {
  if (typeof author === "object" && author !== null) {
    if (author.family && author.family.trim().length > 0) {
      return author.family.trim();
    }
    return author.name.split(" ").pop() ?? author.name;
  }
  return author.split(" ").pop() ?? author;
}

/** "Landauer", "Bérut and Arakelyan", "Nielsen et al." */
export function authorSummary(
  authors?: readonly (string | CitationAuthor)[] | null,
): string {
  if (!authors || authors.length === 0) return "Unknown author";
  const first = extractSurname(authors[0] ?? "");
  if (authors.length === 1) return first;
  const second = extractSurname(authors[1] ?? "");
  if (authors.length === 2) return `${first} and ${second}`;
  return `${first} et al.`;
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

/** Formats entry year or returns empty string. */
export function formatYear(year?: number | string | null): string {
  if (year === undefined || year === null || year === "") return "";
  return String(year);
}

/** Formats venue container string including volume, number, and pages. */
export function formatVenue(
  entry: Partial<CitationEntry> & { venue?: string },
): string {
  const base = entry.journal_or_publisher || entry.venue || "";
  if (!base) return "";

  const parts: string[] = [base];
  if (entry.volume) {
    let volStr = entry.volume;
    if (entry.number) {
      volStr += `(${entry.number})`;
    }
    parts.push(volStr);
  } else if (entry.number) {
    parts.push(`No. ${entry.number}`);
  }
  if (entry.pages) {
    parts.push(entry.pages);
  }
  return parts.join(", ");
}

export interface VerificationBadgeInfo {
  readonly label: string;
  readonly colorClass: string;
  readonly icon: "check" | "warning" | "error" | "references";
  readonly description: string;
}

/** UI badge styling and accessible label for verification states. */
export function verificationBadge(
  status: CitationVerificationStatus,
): VerificationBadgeInfo {
  switch (status) {
    case "verified":
      return {
        label: "Verified",
        colorClass: "text-accent border-accent/40 bg-accent/10",
        icon: "check",
        description: "Verified against authoritative registries (Crossref, OpenAlex, arXiv)",
      };
    case "mismatch":
      return {
        label: "Mismatch",
        colorClass: "text-amber-400 border-amber-400/40 bg-amber-400/10",
        icon: "warning",
        description: "Local metadata differs from authoritative registry records",
      };
    case "not_found":
      return {
        label: "Not found",
        colorClass: "text-text-muted border-line bg-surface",
        icon: "references",
        description: "No matching record found in online registries",
      };
    case "error":
      return {
        label: "Error",
        colorClass: "text-red-400 border-red-400/40 bg-red-400/10",
        icon: "error",
        description: "Verification request failed",
      };
  }
}

/** Formats a difference between local and remote metadata into readable text. */
export function formatDifference(diff: CitationDifference): string {
  return `${diff.field}: local "${diff.local}" vs remote "${diff.remote}"`;
}
