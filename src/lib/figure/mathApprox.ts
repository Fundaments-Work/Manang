/**
 * The browser dev kernel's stand-in for the formula renderers.
 *
 * The real engines are Rust: Typst's compiler for Typst math and RaTeX for
 * LaTeX math (`crates/manang-typst`, `crates/manang-figure/src/math.rs`);
 * both draw glyph outlines. Neither exists in the browser, so this module
 * returns an APPROXIMATE rendering, clearly marked as such in the SVG: the
 * source set in a serif italic, sized from the same metrics the plugin uses
 * for an unmeasured formula. It exists so the editor's code paths (language
 * switch, debounce, cache, measured box, errors with a position) run in the
 * dev kernel and in `verify-ui`; it is never what ships.
 */

import { measure } from "./text";
import type { MathLanguage } from "./scene";

export interface ApproxMath {
  readonly svg: string;
  readonly width: number;
  readonly height: number;
}

export interface ApproxMathError {
  readonly message: string;
  readonly line?: number;
  readonly column?: number;
  readonly length?: number;
}

function escape(text: string): string {
  return text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

/** Where the first unbalanced bracket of `source` is, as the engines report it. */
function bracketError(
  source: string,
  pairs: Readonly<Record<string, string>>,
  what: string,
): ApproxMathError | undefined {
  const closers = new Set(Object.values(pairs));
  const stack: { char: string; line: number; column: number }[] = [];
  let line = 1;
  let column = 1;
  for (const c of source) {
    if (c === "\n") {
      line += 1;
      column = 1;
      continue;
    }
    if (pairs[c] !== undefined) stack.push({ char: c, line, column });
    else if (closers.has(c)) {
      const open = stack.pop();
      if (open === undefined || pairs[open.char] !== c) {
        return { message: `unexpected ${what} \`${c}\``, line, column, length: 1 };
      }
    }
    column += 1;
  }
  const open = stack.pop();
  return open === undefined
    ? undefined
    : {
        message: `unclosed ${what} \`${open.char}\``,
        line: open.line,
        column: open.column,
        length: 1,
      };
}

/** The error a real engine would give for obviously broken source, if any. */
export function approxMathError(
  language: MathLanguage,
  source: string,
): ApproxMathError | undefined {
  if (source.trim() === "") return { message: "the formula is empty" };
  return language === "latex"
    ? bracketError(source, { "{": "}", "[": "]" }, "brace")
    : bracketError(source, { "(": ")", "[": "]", "{": "}" }, "delimiter");
}

/** An approximate rendering at `size`; throws an `ApproxMathError` for broken source. */
export function approxMath(
  language: MathLanguage,
  source: string,
  size: number,
): ApproxMath {
  const error = approxMathError(language, source);
  if (error !== undefined) throw error;
  const width =
    Math.ceil(Math.max(measure(source, size, "mono") * 0.8, size) * 10) / 10;
  const height = Math.ceil(size * 1.4 * 10) / 10;
  const svg =
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${width} ${height}" width="${width}" height="${height}">` +
    `<!-- manang browser mock: approximate ${language} math, not the real renderer -->` +
    `<text x="0" y="${Math.round(size * 1.05 * 10) / 10}" fill="currentColor" font-family="serif" font-style="italic" font-size="${size}" data-approximate-math="${language}">${escape(source.replaceAll("\n", " "))}</text>` +
    `</svg>`;
  return { svg, width, height };
}

/** `svg` with every `currentColor` paint replaced by `colour`. */
export function paintApprox(svg: string, colour: string): string {
  return svg.replaceAll('fill="currentColor"', `fill="${colour}"`);
}
