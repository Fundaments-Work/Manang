/**
 * Formula rendering on the canvas, the pure part: cache keys, the size of a
 * rendering, and the structured error the plugin answers with
 * (`[math_error] {json}`, see `crates/manang-figure/src/math.rs`).
 */

import type { MathLanguage, MathNode } from "./scene";
import { measure } from "./text";

/** A formula's problem, positioned inside its source. */
export interface MathError {
  readonly message: string;
  /** 1-based. */
  readonly line?: number;
  /** 1-based, in characters. */
  readonly column?: number;
  readonly length?: number;
  readonly hints?: readonly string[];
}

/** What the canvas keeps per `(language, source, size, colour)`. */
export interface MathRender {
  /** A blob URL of the SVG, for `<image>`. */
  readonly url?: string;
  readonly width?: number;
  readonly height?: number;
  readonly error?: MathError;
}

/** The cache key of one rendering. Editing anything else never changes it. */
export function mathKey(
  language: MathLanguage,
  source: string,
  size: number,
  colour: string,
): string {
  return `${language}\n${size}\n${colour}\n${source}`;
}

/** The size of a rendering from its root `viewBox="0 0 w h"`. */
export function svgSize(svg: string): { width: number; height: number } | undefined {
  const root = /<svg\b[^>]*>/.exec(svg)?.[0];
  const box = root === undefined ? null : /viewBox="([^"]*)"/.exec(root);
  const nums = box?.[1]?.split(/[\s,]+/).map(Number) ?? [];
  const [, , width, height] = nums;
  return width !== undefined && height !== undefined && width > 0 && height > 0
    ? { width, height }
    : undefined;
}

/** The structured error inside a plugin error message, else the message as it is. */
export function parseMathError(message: string): MathError {
  const tagged = message.split("[math_error] ")[1];
  if (tagged !== undefined) {
    const end = tagged.lastIndexOf("}");
    try {
      const value: unknown = JSON.parse(end === -1 ? tagged : tagged.slice(0, end + 1));
      if (
        typeof value === "object" &&
        value !== null &&
        typeof (value as { message?: unknown }).message === "string"
      ) {
        return value as MathError;
      }
    } catch {
      // Fall through: show the raw message.
    }
  }
  return { message };
}

/** "line 2, column 5" for an error with a position, else an empty string. */
export function errorPlace(error: MathError): string {
  return error.line === undefined
    ? ""
    : `line ${error.line}${error.column === undefined ? "" : `, column ${error.column}`}`;
}

/** A formula's box: measured when the plugin has rendered it, else estimated. */
export function mathBox(node: MathNode): { width: number; height: number } {
  return {
    width:
      node.width !== undefined && node.width > 0
        ? node.width
        : Math.max(measure(node.source, node.size, "mono"), node.size),
    height:
      node.height !== undefined && node.height > 0 ? node.height : node.size * 1.4,
  };
}
