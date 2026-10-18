/**
 * Span producers of the in-memory `visual` plugin: a small Markdown and Typst
 * recogniser, enough for the browser demo and the UI suite. The real adapters
 * are `crates/manang-markdown` (`visual`) and `crates/manang-typst` (`visual`);
 * this mirrors their *output shape* (kinds, `aux`, markers as children), not
 * their coverage. Offsets are UTF-16 code units: JavaScript string indexes.
 */

import { SPAN_KIND } from "$lib/editor/visual/protocol";
import type { EncodableSpan } from "$lib/editor/visual/table";

/** A span before layers are known. */
export interface RawSpan {
  readonly start: number;
  readonly end: number;
  readonly kind: number;
  readonly aux?: number;
  readonly attr?: string | null;
}

export type MockFormat = "markdown" | "typst";

/** Sort into table order and number the layers (the host's invariant, section 3). */
export function toTable(raw: readonly RawSpan[], docLen: number): EncodableSpan[] {
  const sorted = raw
    .map((span, order) => ({ span, order }))
    .filter(({ span }) => span.start < span.end && span.end <= docLen)
    .sort(
      (a, b) =>
        a.span.start - b.span.start || b.span.end - a.span.end || a.order - b.order,
    );
  const open: number[] = [];
  const out: EncodableSpan[] = [];
  for (const { span } of sorted) {
    while (open.length > 0 && (open[open.length - 1] ?? 0) <= span.start) open.pop();
    // A span crossing its parent would break the laminar family: drop it.
    const parentEnd = open[open.length - 1];
    if (parentEnd !== undefined && span.end > parentEnd) continue;
    out.push({ ...span, layer: open.length });
    open.push(span.end);
  }
  return out;
}

export function spansOf(format: MockFormat, text: string): RawSpan[] {
  return format === "markdown" ? markdownSpans(text) : typstSpans(text);
}

interface Line {
  readonly start: number;
  readonly text: string;
}

function lines(text: string): Line[] {
  const out: Line[] = [];
  let start = 0;
  for (;;) {
    const next = text.indexOf("\n", start);
    if (next === -1) {
      out.push({ start, text: text.slice(start) });
      return out;
    }
    out.push({ start, text: text.slice(start, next) });
    start = next + 1;
  }
}

/* -------------------------------------------------------------------------- */
/* Markdown                                                                   */
/* -------------------------------------------------------------------------- */

function markdownSpans(text: string): RawSpan[] {
  const out: RawSpan[] = [];
  const all = lines(text);
  let i = 0;
  if (all[0]?.text === "---") {
    const close = all.findIndex((l, n) => n > 0 && l.text === "---");
    if (close > 0) {
      const last = all[close] as Line;
      out.push({
        start: 0,
        end: last.start + last.text.length,
        kind: SPAN_KIND.frontmatter,
      });
      i = close + 1;
    }
  }
  while (i < all.length) {
    const line = all[i] as Line;
    const fence = /^(```+|~~~+)\s*([\w+-]*)\s*$/.exec(line.text);
    if (fence !== null) {
      const marker = fence[1] as string;
      let close = i + 1;
      while (close < all.length && !(all[close] as Line).text.startsWith(marker)) {
        close += 1;
      }
      const end =
        close < all.length ? (all[close] as Line) : (all[all.length - 1] as Line);
      const endPos = end.start + end.text.length;
      out.push({
        start: line.start,
        end: endPos,
        kind: SPAN_KIND.codeBlock,
        attr: fence[2] === "" ? null : (fence[2] ?? null),
      });
      out.push({
        start: line.start,
        end: line.start + line.text.length,
        kind: SPAN_KIND.marker,
      });
      if (close < all.length && close > i) {
        out.push({ start: end.start, end: endPos, kind: SPAN_KIND.marker });
      }
      i = close + 1;
      continue;
    }
    const heading = /^(#{1,6})[ \t]+\S/.exec(line.text);
    if (heading !== null) {
      const marker = /^#{1,6}[ \t]+/.exec(line.text) as RegExpExecArray;
      out.push({
        start: line.start,
        end: line.start + line.text.length,
        kind: SPAN_KIND.heading,
        aux: (heading[1] as string).length,
      });
      out.push({
        start: line.start,
        end: line.start + marker[0].length,
        kind: SPAN_KIND.marker,
      });
      inline(line.text, line.start, marker[0].length, "markdown", out);
      i += 1;
      continue;
    }
    if (/^ {0,3}([-*_])( ?\1){2,}\s*$/.test(line.text) && line.text.trim() !== "") {
      out.push({
        start: line.start,
        end: line.start + line.text.length,
        kind: SPAN_KIND.rule,
      });
      i += 1;
      continue;
    }
    if (/^>/.test(line.text)) {
      let last = i;
      while (last + 1 < all.length && /^>/.test((all[last + 1] as Line).text))
        last += 1;
      const end = all[last] as Line;
      out.push({
        start: line.start,
        end: end.start + end.text.length,
        kind: SPAN_KIND.quote,
      });
      for (let n = i; n <= last; n += 1) {
        const q = all[n] as Line;
        const marker = /^> ?/.exec(q.text) as RegExpExecArray;
        out.push({
          start: q.start,
          end: q.start + marker[0].length,
          kind: SPAN_KIND.marker,
        });
        inline(q.text, q.start, marker[0].length, "markdown", out);
      }
      i = last + 1;
      continue;
    }
    const item = /^(\s*)([-*+]|\d+[.)])[ \t]+(\[([ xX])\][ \t]+)?/.exec(line.text);
    if (item !== null && line.text.length > item[0].length) {
      const task = item[3] !== undefined;
      const ordered = /\d/.test(item[2] as string);
      const indent = (item[1] as string).length;
      out.push({
        start: line.start + indent,
        end: line.start + line.text.length,
        kind: SPAN_KIND.listItem,
        aux: task ? ((item[4] as string) === " " ? 2 : 3) : ordered ? 1 : 0,
      });
      out.push({
        start: line.start + indent,
        end: line.start + item[0].length,
        kind: SPAN_KIND.marker,
      });
      inline(line.text, line.start, item[0].length, "markdown", out);
      i += 1;
      continue;
    }
    // Block math between `$$` lines.
    if (line.text.trim() === "$$") {
      let close = i + 1;
      while (close < all.length && (all[close] as Line).text.trim() !== "$$")
        close += 1;
      if (close < all.length) {
        const end = all[close] as Line;
        out.push({
          start: line.start,
          end: end.start + end.text.length,
          kind: SPAN_KIND.math,
          aux: 1,
          attr: "latex",
        });
        i = close + 1;
        continue;
      }
    }
    inline(line.text, line.start, 0, "markdown", out);
    i += 1;
  }
  return out;
}

/* -------------------------------------------------------------------------- */
/* Inline                                                                     */
/* -------------------------------------------------------------------------- */

interface Pattern {
  readonly kind: number;
  readonly re: RegExp;
}

const MD_INLINE: readonly Pattern[] = [
  { kind: SPAN_KIND.code, re: /`([^`\n]+)`/y },
  { kind: SPAN_KIND.image, re: /!\[([^\]\n]*)\]\(([^)\s]+)\)/y },
  { kind: SPAN_KIND.link, re: /\[([^\]\n]+)\]\(([^)\s]*)\)/y },
  { kind: SPAN_KIND.math, re: /\$\$([^$\n]+)\$\$/y },
  { kind: SPAN_KIND.math, re: /\$([^$\s][^$\n]*)\$/y },
  { kind: SPAN_KIND.strong, re: /\*\*(?=\S)([^\n]*?\S)\*\*/y },
  { kind: SPAN_KIND.strike, re: /~~(?=\S)([^\n]*?\S)~~/y },
  { kind: SPAN_KIND.emph, re: /\*(?=[^\s*])([^\n*]*?[^\s*])\*/y },
  { kind: SPAN_KIND.emph, re: /\b_(?=[^\s_])([^\n_]*?[^\s_])_\b/y },
];

const TYP_INLINE: readonly Pattern[] = [
  { kind: SPAN_KIND.code, re: /`([^`\n]+)`/y },
  { kind: SPAN_KIND.image, re: /#image\("([^"\n]+)"[^)\n]*\)/y },
  { kind: SPAN_KIND.link, re: /#link\("([^"\n]*)"\)\[([^\]\n]*)\]/y },
  { kind: SPAN_KIND.math, re: /\$([^$\n]+)\$/y },
  { kind: SPAN_KIND.strong, re: /\*(?=[^\s*])([^\n*]*?[^\s*])\*/y },
  { kind: SPAN_KIND.emph, re: /\b_(?=[^\s_])([^\n_]*?[^\s_])_\b/y },
  { kind: SPAN_KIND.ref, re: /@([A-Za-z][\w:.-]*)/y },
  { kind: SPAN_KIND.label, re: /<([A-Za-z][\w:.-]*)>/y },
];

/**
 * The inline spans of `text.slice(from)`, reported at `base + index`. The text
 * is scanned left to right; a recognised construct recurses into its content.
 */
function inline(
  text: string,
  base: number,
  from: number,
  format: MockFormat,
  out: RawSpan[],
  to: number = text.length,
): void {
  const patterns = format === "markdown" ? MD_INLINE : TYP_INLINE;
  const region = text.slice(0, to);
  let at = from;
  while (at < region.length) {
    let hit: { pattern: Pattern; match: RegExpExecArray } | null = null;
    for (const pattern of patterns) {
      pattern.re.lastIndex = at;
      const match = pattern.re.exec(region);
      if (match !== null) {
        hit = { pattern, match };
        break;
      }
    }
    if (hit === null) {
      at += 1;
      continue;
    }
    const { pattern, match } = hit;
    const start = at;
    const end = at + match[0].length;
    emit(pattern.kind, match, text, base, start, end, format, out);
    at = end;
  }
}

function emit(
  kind: number,
  match: RegExpExecArray,
  text: string,
  base: number,
  start: number,
  end: number,
  format: MockFormat,
  out: RawSpan[],
): void {
  const mark = (a: number, b: number): void => {
    out.push({ start: base + a, end: base + b, kind: SPAN_KIND.marker });
  };
  const typst = format === "typst";
  switch (kind) {
    case SPAN_KIND.code: {
      out.push({ start: base + start, end: base + end, kind });
      mark(start, start + 1);
      mark(end - 1, end);
      return;
    }
    case SPAN_KIND.strong:
    case SPAN_KIND.emph:
    case SPAN_KIND.strike: {
      const width = typst || kind === SPAN_KIND.emph ? 1 : 2;
      out.push({ start: base + start, end: base + end, kind });
      mark(start, start + width);
      mark(end - width, end);
      inline(text, base, start + width, format, out, end - width);
      return;
    }
    case SPAN_KIND.link: {
      out.push({
        start: base + start,
        end: base + end,
        kind,
        attr: typst ? (match[1] ?? "") : (match[2] ?? ""),
      });
      if (typst) {
        const textStart = start + (end - start) - (match[2] ?? "").length - 1;
        mark(start, textStart);
        mark(end - 1, end);
      } else {
        const labelEnd = start + 1 + (match[1] ?? "").length;
        mark(start, start + 1);
        mark(labelEnd, end);
        inline(text, base, start + 1, format, out, labelEnd);
      }
      return;
    }
    case SPAN_KIND.image: {
      out.push({
        start: base + start,
        end: base + end,
        kind,
        aux: 0,
        attr: typst ? (match[1] ?? "") : (match[2] ?? ""),
      });
      return;
    }
    case SPAN_KIND.math: {
      const doubled = !typst && match[0].startsWith("$$");
      const block =
        doubled || (typst && /^\$\s/.test(match[0]) && /\s\$$/.test(match[0]));
      out.push({
        start: base + start,
        end: base + end,
        kind,
        aux: block ? 1 : 0,
        attr: typst ? "typst" : "latex",
      });
      return;
    }
    case SPAN_KIND.ref:
    case SPAN_KIND.label:
      out.push({ start: base + start, end: base + end, kind, attr: match[1] ?? "" });
      return;
    default:
  }
}

/* -------------------------------------------------------------------------- */
/* Typst                                                                      */
/* -------------------------------------------------------------------------- */

function typstSpans(text: string): RawSpan[] {
  const out: RawSpan[] = [];
  for (const line of lines(text)) {
    const end = line.start + line.text.length;
    const heading = /^(=+)[ \t]+\S/.exec(line.text);
    if (heading !== null) {
      const marker = /^=+[ \t]+/.exec(line.text) as RegExpExecArray;
      out.push({
        start: line.start,
        end,
        kind: SPAN_KIND.heading,
        aux: Math.min(6, (heading[1] as string).length),
      });
      out.push({
        start: line.start,
        end: line.start + marker[0].length,
        kind: SPAN_KIND.marker,
      });
      inline(line.text, line.start, marker[0].length, "typst", out);
      continue;
    }
    const directive = /^#(set|show|let|import|include|bibliography)\b/.exec(line.text);
    if (directive !== null) {
      out.push({
        start: line.start,
        end,
        kind: SPAN_KIND.opaque,
        attr: directive[1] ?? null,
      });
      continue;
    }
    const figure = /^#figure\(\s*image\("([^"\n]+)"/.exec(line.text);
    if (figure !== null) {
      out.push({
        start: line.start,
        end,
        kind: SPAN_KIND.image,
        aux: 1,
        attr: figure[1] ?? null,
      });
      continue;
    }
    const comment = /^\s*\/\/.*$/.exec(line.text);
    if (comment !== null) {
      const at = line.text.indexOf("//");
      out.push({ start: line.start + at, end, kind: SPAN_KIND.comment });
      continue;
    }
    const item = /^(\s*)([-+])[ \t]+/.exec(line.text);
    if (item !== null && line.text.length > item[0].length) {
      const indent = (item[1] as string).length;
      out.push({
        start: line.start + indent,
        end,
        kind: SPAN_KIND.listItem,
        aux: item[2] === "+" ? 1 : 0,
      });
      out.push({
        start: line.start + indent,
        end: line.start + item[0].length,
        kind: SPAN_KIND.marker,
      });
      inline(line.text, line.start, item[0].length, "typst", out);
      continue;
    }
    inline(line.text, line.start, 0, "typst", out);
  }
  return out;
}
