/**
 * The redaction diff of "Preview what will be sent": the original text with each redacted span
 * marked, followed by what replaced it. The host reports spans as UTF-8 byte offsets into the
 * original text.
 */

import type { WireRedaction } from "./types";

export type RedactionSegment =
  | { readonly kind: "text"; readonly text: string }
  | { readonly kind: "removed"; readonly text: string }
  | { readonly kind: "replacement"; readonly text: string };

/** The UTF-16 index of a UTF-8 byte offset in `text` (clamped to the text's end). */
export function byteToIndex(text: string, byte: number): number {
  let bytes = 0;
  let index = 0;
  for (const ch of text) {
    if (bytes >= byte) break;
    const code = ch.codePointAt(0) ?? 0;
    bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
    index += ch.length;
  }
  return index;
}

/** Splits `original` into plain text, removed spans and their replacements. */
export function redactionSegments(
  original: string,
  spans: readonly WireRedaction[],
): RedactionSegment[] {
  const out: RedactionSegment[] = [];
  let at = 0;
  for (const span of [...spans].sort((a, b) => a.start - b.start)) {
    const from = byteToIndex(original, span.start);
    const to = byteToIndex(original, span.end);
    if (from < at) continue;
    if (from > at) out.push({ kind: "text", text: original.slice(at, from) });
    out.push({ kind: "removed", text: original.slice(from, to) });
    out.push({ kind: "replacement", text: span.replacement });
    at = to;
  }
  if (at < original.length) out.push({ kind: "text", text: original.slice(at) });
  return out;
}
