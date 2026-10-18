/**
 * Deterministic text metrics and wrapping; mirrors `text.rs` (same advance
 * table, same greedy algorithm), so the canvas breaks lines where the export
 * does.
 */

import type { FontFamily } from "./scene";

export const LINE_HEIGHT = 1.25;

const NARROW = new Set("ijl.,:;'|!I");
const SEMI = new Set('ftr()[]{}-"/\\');

/** Advance width of one character (a Unicode scalar) in ems. */
export function advance(c: string, family: FontFamily): number {
  if (family === "mono") return 0.6;
  if (c === " ") return 0.26;
  if (NARROW.has(c)) return 0.24;
  if (SEMI.has(c)) return 0.34;
  if (c === "m" || c === "w") return 0.82;
  if (c === "M" || c === "W") return 0.9;
  if (c >= "A" && c <= "Z") return 0.66;
  if (c >= "0" && c <= "9") return 0.58;
  if (c >= "a" && c <= "z") return 0.54;
  const code = c.codePointAt(0) ?? 0;
  if (code < 0x80) return 0.56;
  if (code >= 0x2e80) return 1.0;
  return 0.6;
}

/** Width of `text` on one line. */
export function measure(text: string, size: number, family: FontFamily): number {
  let sum = 0;
  for (const c of text) sum += advance(c, family);
  return sum * size;
}

/**
 * Greedy word wrap to `maxWidth`. Explicit newlines are kept; a word longer
 * than a line is broken between characters; a non-positive width disables
 * wrapping.
 */
export function wrap(
  text: string,
  maxWidth: number,
  size: number,
  family: FontFamily,
): string[] {
  const lines: string[] = [];
  for (const paragraph of text.split("\n")) {
    if (maxWidth <= 0) {
      lines.push(paragraph);
      continue;
    }
    let line = "";
    for (const word of paragraph.split(" ").filter((w) => w !== "")) {
      const candidate = line === "" ? word : `${line} ${word}`;
      if (measure(candidate, size, family) <= maxWidth) {
        line = candidate;
        continue;
      }
      if (line !== "") {
        lines.push(line);
        line = "";
      }
      for (const c of word) {
        const next = line + c;
        if (line !== "" && measure(next, size, family) > maxWidth) {
          lines.push(line);
          line = c;
        } else {
          line = next;
        }
      }
    }
    lines.push(line);
  }
  return lines;
}

/** Width and height of `text` without a width limit. */
export function naturalSize(
  text: string,
  size: number,
  family: FontFamily,
): { width: number; height: number } {
  const lines = text.split("\n");
  const width = lines.reduce((w, l) => Math.max(w, measure(l, size, family)), 0);
  return { width, height: lines.length * size * LINE_HEIGHT };
}
