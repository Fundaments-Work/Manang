/**
 * Offset arithmetic between CodeMirror and the Rust editor plugin.
 *
 * CodeMirror positions count UTF-16 code units; Rust's ropey indexes Unicode
 * scalar values (`char`s). The two agree for the Basic Multilingual Plane and
 * diverge for every astral code point (emoji, rare CJK, maths alphanumerics),
 * which occupy two UTF-16 units but one scalar. Combining marks are BMP scalars,
 * so they count as one in both worlds; neither side counts graphemes.
 *
 * These helpers accept a string or a CodeMirror `Text` (anything with
 * `sliceString`), so a document is measured in place without being flattened.
 */

/** The slice of CodeMirror's `Text` these helpers need. */
export interface Sliceable {
  readonly length: number;
  sliceString(from: number, to?: number): string;
}

function isHigh(unit: number): boolean {
  return unit >= 0xd800 && unit <= 0xdbff;
}

function isLow(unit: number): boolean {
  return unit >= 0xdc00 && unit <= 0xdfff;
}

/** Number of Unicode scalar values in `text`. A lone surrogate counts as one. */
export function scalarLength(text: string): number {
  let count = 0;
  for (let i = 0; i < text.length; i += 1) {
    const unit = text.charCodeAt(i);
    if (isHigh(unit) && i + 1 < text.length && isLow(text.charCodeAt(i + 1))) i += 1;
    count += 1;
  }
  return count;
}

/** Scalar index of the UTF-16 offset `offset` in `doc`. */
export function utf16ToScalar(doc: string | Sliceable, offset: number): number {
  const clamped = Math.max(0, Math.min(offset, doc.length));
  const head =
    typeof doc === "string" ? doc.slice(0, clamped) : doc.sliceString(0, clamped);
  return scalarLength(head);
}

/**
 * UTF-16 offset of scalar index `scalar` in `text`. Indices past the end clamp
 * to `text.length`.
 */
export function scalarToUtf16(text: string, scalar: number): number {
  let offset = 0;
  let seen = 0;
  while (seen < scalar && offset < text.length) {
    const unit = text.charCodeAt(offset);
    offset +=
      isHigh(unit) && offset + 1 < text.length && isLow(text.charCodeAt(offset + 1))
        ? 2
        : 1;
    seen += 1;
  }
  return offset;
}

/**
 * Smallest single replacement turning `before` into `after`, found by trimming
 * the common prefix and suffix. Never splits a surrogate pair.
 */
export function diffRange(
  before: string,
  after: string,
): { from: number; to: number; insert: string } {
  const max = Math.min(before.length, after.length);
  let start = 0;
  while (start < max && before.charCodeAt(start) === after.charCodeAt(start))
    start += 1;
  if (start > 0 && isHigh(before.charCodeAt(start - 1))) start -= 1;

  let endBefore = before.length;
  let endAfter = after.length;
  while (
    endBefore > start &&
    endAfter > start &&
    before.charCodeAt(endBefore - 1) === after.charCodeAt(endAfter - 1)
  ) {
    endBefore -= 1;
    endAfter -= 1;
  }
  if (endBefore < before.length && isLow(before.charCodeAt(endBefore))) {
    endBefore += 1;
    endAfter += 1;
  }

  return { from: start, to: endBefore, insert: after.slice(start, endAfter) };
}
