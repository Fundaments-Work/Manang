/**
 * A small, safe Markdown reader for assistant messages: paragraphs, lists, headings, quotes,
 * fenced code, inline code, bold and italic. It returns data, never HTML, so nothing the model
 * writes can inject markup; the view renders the blocks as ordinary text nodes.
 *
 * It accepts unfinished input (a fence that is still open while streaming) without throwing.
 */

export type Inline =
  | { readonly kind: "text"; readonly text: string }
  | { readonly kind: "code"; readonly text: string }
  | { readonly kind: "strong"; readonly text: string }
  | { readonly kind: "em"; readonly text: string };

export type Block =
  | { readonly kind: "paragraph"; readonly inline: readonly Inline[] }
  | { readonly kind: "heading"; readonly inline: readonly Inline[] }
  | { readonly kind: "quote"; readonly inline: readonly Inline[] }
  | {
      readonly kind: "list";
      readonly ordered: boolean;
      readonly items: readonly (readonly Inline[])[];
    }
  | {
      readonly kind: "code";
      readonly lang: string;
      readonly text: string;
      /** The closing fence has not arrived (still streaming). */
      readonly open: boolean;
    };

const INLINE = /`([^`\n]+)`|\*\*([^*\n]+)\*\*|(?<![*\w])\*([^*\n]+)\*(?![*\w])/g;

export function parseInline(text: string): Inline[] {
  const out: Inline[] = [];
  let at = 0;
  for (const m of text.matchAll(INLINE)) {
    const start = m.index ?? 0;
    if (start > at) out.push({ kind: "text", text: text.slice(at, start) });
    if (m[1] !== undefined) out.push({ kind: "code", text: m[1] });
    else if (m[2] !== undefined) out.push({ kind: "strong", text: m[2] });
    else if (m[3] !== undefined) out.push({ kind: "em", text: m[3] });
    at = start + m[0].length;
  }
  if (at < text.length) out.push({ kind: "text", text: text.slice(at) });
  return out;
}

const FENCE = /^\s{0,3}(`{3,}|~{3,})\s*([\w+-]*)/;
const BULLET = /^\s{0,3}[-*+]\s+(.*)$/;
const NUMBERED = /^\s{0,3}\d+[.)]\s+(.*)$/;
const HEADING = /^\s{0,3}#{1,6}\s+(.*)$/;
const QUOTE = /^\s{0,3}>\s?(.*)$/;

export function parseMarkdown(source: string): Block[] {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let paragraph: string[] = [];
  let list: { ordered: boolean; items: string[] } | null = null;
  let quote: string[] = [];

  const flush = (): void => {
    if (paragraph.length > 0) {
      blocks.push({ kind: "paragraph", inline: parseInline(paragraph.join(" ")) });
      paragraph = [];
    }
    if (list !== null) {
      blocks.push({
        kind: "list",
        ordered: list.ordered,
        items: list.items.map(parseInline),
      });
      list = null;
    }
    if (quote.length > 0) {
      blocks.push({ kind: "quote", inline: parseInline(quote.join(" ")) });
      quote = [];
    }
  };

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i] ?? "";
    const fence = FENCE.exec(line);
    if (fence !== null) {
      flush();
      const marker = fence[1] ?? "```";
      const body: string[] = [];
      let closed = false;
      for (i += 1; i < lines.length; i += 1) {
        const next = (lines[i] ?? "").trim();
        if (next.startsWith(marker) && next.replace(/[`~]/g, "") === "") {
          closed = true;
          break;
        }
        body.push(lines[i] ?? "");
      }
      blocks.push({
        kind: "code",
        lang: fence[2] ?? "",
        text: body.join("\n"),
        open: !closed,
      });
      continue;
    }
    if (line.trim() === "") {
      flush();
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading !== null) {
      flush();
      blocks.push({ kind: "heading", inline: parseInline(heading[1] ?? "") });
      continue;
    }
    const bullet = BULLET.exec(line);
    const numbered = bullet === null ? NUMBERED.exec(line) : null;
    const item = bullet ?? numbered;
    if (item !== null) {
      const ordered = numbered !== null;
      if (list === null || list.ordered !== ordered) {
        flush();
        list = { ordered, items: [] };
      }
      list.items.push(item[1] ?? "");
      continue;
    }
    const q = QUOTE.exec(line);
    if (q !== null) {
      if (quote.length === 0) flush();
      quote.push(q[1] ?? "");
      continue;
    }
    if (list !== null && /^\s+\S/.test(line)) {
      // A wrapped list item continues the previous one.
      const items = list.items;
      items[items.length - 1] = `${items[items.length - 1] ?? ""} ${line.trim()}`;
      continue;
    }
    if (list !== null || quote.length > 0) flush();
    paragraph.push(line.trim());
  }
  flush();
  return blocks;
}
