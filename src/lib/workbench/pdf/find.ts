/**
 * Find in PDF: matching over the text pdf.js extracts from each page, and the
 * mapping from a match back to the text-layer runs it covers so they can be
 * highlighted. Pure.
 *
 * A page's text is its runs (`str`) joined in order, with a space after a run
 * that ends a line, so "entropy production" matches across a line break. A
 * space in the query matches any run of whitespace. Matches never span pages.
 */

/** The parts of a pdf.js text item the search needs. */
export interface TextRun {
  readonly str: string;
  readonly hasEOL?: boolean;
  /** Points from the page's top edge to the run's top; for scrolling to it. */
  readonly top?: number;
}

export interface PageText {
  readonly text: string;
  /** Offset in `text` where each run starts. */
  readonly starts: readonly number[];
  /** Length of each run's own text (without the line-break space). */
  readonly lengths: readonly number[];
  readonly tops: readonly number[];
}

export interface FindMatch {
  readonly page: number;
  readonly start: number;
  readonly end: number;
}

export interface FindOptions {
  readonly caseSensitive: boolean;
}

/** One stretch of a match inside one run. */
export interface RunRange {
  readonly run: number;
  readonly start: number;
  readonly end: number;
}

/** A pdf.js text item, or a marked-content marker (no `str`). */
export interface TextItemLike {
  /** Set on marked-content markers (`beginMarkedContent`...). */
  readonly type?: string;
  readonly str?: string;
  readonly hasEOL?: boolean;
  readonly transform?: readonly number[];
  readonly height?: number;
}

/**
 * The runs of a page from pdf.js text content, in the order the text layer
 * creates its elements (markers skipped), with each run's top in points from
 * the top of the page box `view` (`[x0, y0, x1, y1]`).
 */
export function runsOf(
  items: readonly TextItemLike[],
  view: readonly number[],
): TextRun[] {
  const top = view[3] ?? 0;
  const runs: TextRun[] = [];
  for (const item of items) {
    if (typeof item.str !== "string") continue;
    const baseline = item.transform?.[5] ?? top;
    runs.push({
      str: item.str,
      hasEOL: item.hasEOL === true,
      top: Math.max(0, top - baseline - (item.height ?? 0)),
    });
  }
  return runs;
}

export function pageText(runs: readonly TextRun[]): PageText {
  let text = "";
  const starts: number[] = [];
  const lengths: number[] = [];
  const tops: number[] = [];
  for (const run of runs) {
    starts.push(text.length);
    lengths.push(run.str.length);
    tops.push(run.top ?? 0);
    text += run.str;
    if (run.hasEOL === true && !/\s$/.test(run.str)) text += " ";
  }
  return { text, starts, lengths, tops };
}

function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** The search as a global regular expression, or null for an empty query. */
export function queryPattern(query: string, options: FindOptions): RegExp | null {
  const trimmed = query.trim();
  if (trimmed === "") return null;
  const source = trimmed.split(/\s+/).map(escapeRegExp).join("\\s+");
  return new RegExp(source, options.caseSensitive ? "gu" : "giu");
}

/** Every match, in reading order: page by page, left to right within a page. */
export function findMatches(
  pages: readonly PageText[],
  query: string,
  options: FindOptions,
): FindMatch[] {
  const pattern = queryPattern(query, options);
  if (pattern === null) return [];
  const out: FindMatch[] = [];
  pages.forEach((page, index) => {
    pattern.lastIndex = 0;
    for (const match of page.text.matchAll(pattern)) {
      const start = match.index;
      if (match[0].length === 0) continue;
      out.push({ page: index, start, end: start + match[0].length });
    }
  });
  return out;
}

/** The runs a match covers on its page, with offsets inside each run. */
export function runRanges(page: PageText, match: FindMatch): RunRange[] {
  const out: RunRange[] = [];
  page.starts.forEach((runStart, run) => {
    const runEnd = runStart + (page.lengths[run] ?? 0);
    const start = Math.max(match.start, runStart);
    const end = Math.min(match.end, runEnd);
    if (start < end) out.push({ run, start: start - runStart, end: end - runStart });
  });
  return out;
}

export interface RunHit {
  readonly start: number;
  readonly end: number;
  readonly current: boolean;
}

/**
 * The highlights of one page, per text-layer run: every match on `page`
 * split over the runs it covers, the one at index `current` marked.
 */
export function hitsByRun(
  page: PageText,
  matches: readonly FindMatch[],
  pageIndex: number,
  current: number,
): Map<number, RunHit[]> {
  const hits = new Map<number, RunHit[]>();
  matches.forEach((match, i) => {
    if (match.page !== pageIndex) return;
    for (const range of runRanges(page, match)) {
      const list = hits.get(range.run) ?? [];
      list.push({ start: range.start, end: range.end, current: i === current });
      hits.set(range.run, list);
    }
  });
  return hits;
}

/** Points from the top of the page to the first run of a match. */
export function matchTop(page: PageText, match: FindMatch): number {
  const first = runRanges(page, match)[0];
  return first === undefined ? 0 : (page.tops[first.run] ?? 0);
}

/** Next (1) or previous (-1) match index, wrapping around the document. */
export function stepMatch(current: number, total: number, direction: 1 | -1): number {
  if (total <= 0) return -1;
  if (current < 0) return direction === 1 ? 0 : total - 1;
  return (current + direction + total) % total;
}

/** The first match on or after `page`, wrapping to the start: where a new search begins. */
export function firstFrom(matches: readonly FindMatch[], page: number): number {
  if (matches.length === 0) return -1;
  const at = matches.findIndex((m) => m.page >= page);
  return at === -1 ? 0 : at;
}

/** "3 of 17", "No matches", or "" before anything is typed. */
export function countLabel(current: number, total: number, query: string): string {
  if (query.trim() === "") return "";
  if (total === 0) return "No matches";
  return `${current + 1} of ${total}`;
}

/**
 * Split a run's text into plain and highlighted pieces for the text layer:
 * `hits` are the ranges inside this run, `current` marks the ones of the
 * current match.
 */
export function splitRun(
  text: string,
  hits: readonly {
    readonly start: number;
    readonly end: number;
    readonly current: boolean;
  }[],
): { readonly text: string; readonly hit: "none" | "match" | "current" }[] {
  const sorted = [...hits].sort((a, b) => a.start - b.start);
  const out: { text: string; hit: "none" | "match" | "current" }[] = [];
  let at = 0;
  for (const h of sorted) {
    const start = Math.max(at, h.start);
    if (start >= h.end) continue;
    if (start > at) out.push({ text: text.slice(at, start), hit: "none" });
    out.push({ text: text.slice(start, h.end), hit: h.current ? "current" : "match" });
    at = h.end;
  }
  if (at < text.length) out.push({ text: text.slice(at), hit: "none" });
  return out;
}
