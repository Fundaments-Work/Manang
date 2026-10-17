/**
 * Rust reports positions in Unicode scalar values; CodeMirror counts UTF-16
 * code units. They differ for every astral code point (emoji, rare CJK, maths
 * alphanumerics): one scalar, two units. Combining marks are BMP scalars and
 * count as one on both sides, so they need no special handling, only the
 * assurance a test gives.
 */

import { scalarLength } from "$lib/kernel/offsets";

const HAS_SURROGATE = /[\ud800-\udfff]/;

/** Converts scalar indices of one text to UTF-16 offsets in O(1) after setup. */
export class ScalarIndex {
  readonly #table: Uint32Array | null;
  readonly #length: number;

  constructor(text: string) {
    this.#length = text.length;
    if (!HAS_SURROGATE.test(text)) {
      this.#table = null;
      return;
    }
    const table = new Uint32Array(scalarLength(text) + 1);
    let scalar = 0;
    for (let unit = 0; unit < text.length; scalar += 1) {
      table[scalar] = unit;
      const code = text.charCodeAt(unit);
      const pair =
        code >= 0xd800 &&
        code <= 0xdbff &&
        unit + 1 < text.length &&
        text.charCodeAt(unit + 1) >= 0xdc00 &&
        text.charCodeAt(unit + 1) <= 0xdfff;
      unit += pair ? 2 : 1;
    }
    table[scalar] = text.length;
    this.#table = table;
  }

  /** UTF-16 offset of `scalar`, clamped to the end of the text. */
  toUtf16(scalar: number): number {
    const at = Math.max(0, scalar);
    if (this.#table === null) return Math.min(at, this.#length);
    return this.#table[Math.min(at, this.#table.length - 1)] ?? this.#length;
  }
}
