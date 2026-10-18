/**
 * The maths behind the inspector's numeric fields, kept pure so it is
 * testable: parsing what was typed (a number, a unit, a small sum),
 * formatting, arrow-key steps, drag-to-scrub, and the "Mixed" merge of one
 * setting across a selection.
 */

export interface Bounds {
  readonly min?: number;
  readonly max?: number;
}

/** Pixels of pointer travel that make one step while scrubbing a label. */
export const SCRUB_PX_PER_STEP = 4;
/** What Shift multiplies a step or a scrub by. */
export const SHIFT_FACTOR = 10;

export function clamp(value: number, { min, max }: Bounds = {}): number {
  let v = value;
  if (min !== undefined && v < min) v = min;
  if (max !== undefined && v > max) v = max;
  return v;
}

/** Number of decimals in a step such as 0.5 (1) or 1 (0). */
function decimalsOf(step: number): number {
  const s = String(step);
  const at = s.indexOf(".");
  return at === -1 ? 0 : s.length - at - 1;
}

/** `value` rounded to what float noise allows (at most 4 decimals, and the step's). */
export function tidy(value: number, step = 1): number {
  const places = Math.min(Math.max(decimalsOf(step), 2), 4);
  const f = 10 ** places;
  const rounded = Math.round(value * f) / f;
  return Object.is(rounded, -0) ? 0 : rounded;
}

/** A value as the field shows it: no trailing zeros, at most 2 decimals. */
export function formatField(value: number): string {
  if (!Number.isFinite(value)) return "";
  return String(Math.round(value * 100) / 100);
}

const UNIT = /\s*(px|pt|%|°|deg)$/i;

/**
 * Parses what was typed in a numeric field: a number, optionally with its
 * unit (`24px`, `45°`, `80%`), or a short sum (`120+16`, `3*8`, `100/4`).
 * Returns undefined when it is not a number (the field then reverts).
 */
export function parseField(text: string, bounds: Bounds = {}): number | undefined {
  const cleaned = text.trim().replace(/,/g, ".").replace(UNIT, "").trim();
  if (cleaned === "") return undefined;
  const value = evaluate(cleaned);
  return value === undefined ? undefined : clamp(value, bounds);
}

/** `a op b op c` with + - * / and a leading sign, left to right, `*` and `/` first. */
function evaluate(source: string): number | undefined {
  const tokens = source.match(/\d*\.?\d+(?:e[+-]?\d+)?|[+\-*/]|\S/gi);
  if (tokens === null || tokens.join("") !== source.replace(/\s+/g, "")) return undefined;
  const terms: (number | string)[] = [];
  let sign = 1;
  let expectNumber = true;
  for (const token of tokens) {
    if (expectNumber) {
      if (token === "-") sign = -sign;
      else if (token === "+") continue;
      else {
        const n = Number(token);
        if (!Number.isFinite(n)) return undefined;
        terms.push(sign * n);
        sign = 1;
        expectNumber = false;
      }
    } else {
      if (token !== "+" && token !== "-" && token !== "*" && token !== "/") {
        return undefined;
      }
      terms.push(token);
      expectNumber = true;
    }
  }
  if (expectNumber || terms.length === 0) return undefined;
  // Multiplication and division bind first.
  const collapsed: (number | string)[] = [];
  for (let i = 0; i < terms.length; i += 1) {
    const t = terms[i];
    if (t === "*" || t === "/") {
      const left = collapsed.pop();
      const right = terms[i + 1];
      if (typeof left !== "number" || typeof right !== "number") return undefined;
      if (t === "/" && right === 0) return undefined;
      collapsed.push(t === "*" ? left * right : left / right);
      i += 1;
    } else if (t !== undefined) {
      collapsed.push(t);
    }
  }
  let total = collapsed[0];
  if (typeof total !== "number") return undefined;
  for (let i = 1; i < collapsed.length; i += 2) {
    const op = collapsed[i];
    const n = collapsed[i + 1];
    if (typeof n !== "number") return undefined;
    total = op === "+" ? total + n : total - n;
  }
  return Number.isFinite(total) ? total : undefined;
}

/** One arrow-key step: `direction` is 1 or -1; Shift steps by ten. */
export function stepValue(
  value: number,
  direction: 1 | -1,
  step: number,
  shift: boolean,
  bounds: Bounds = {},
): number {
  const amount = step * (shift ? SHIFT_FACTOR : 1);
  return clamp(tidy(value + direction * amount, step), bounds);
}

/**
 * The value while a label is dragged: `deltaPx` of pointer travel from the
 * press, one step per `SCRUB_PX_PER_STEP` pixels (rightwards raises it),
 * ten times that with Shift held.
 */
export function scrubValue(
  start: number,
  deltaPx: number,
  step: number,
  shift: boolean,
  bounds: Bounds = {},
): number {
  const steps = Math.trunc(deltaPx / SCRUB_PX_PER_STEP);
  const amount = steps * step * (shift ? SHIFT_FACTOR : 1);
  return clamp(tidy(start + amount, step), bounds);
}

/** What one setting holds across a selection. */
export interface Merged<T> {
  readonly value: T | undefined;
  readonly mixed: boolean;
}

/**
 * One value when every entry agrees, else `mixed`. Numbers agree within
 * 0.005 so that float noise never shows as Mixed; an empty list is unset.
 */
export function mergeValues<T>(values: readonly (T | undefined)[]): Merged<T> {
  const [head] = values;
  const same = (a: T | undefined, b: T | undefined): boolean =>
    typeof a === "number" && typeof b === "number" ? Math.abs(a - b) < 0.005 : a === b;
  return { value: head, mixed: values.some((v) => !same(v, head)) };
}

/** The other side of a box that keeps its proportions when one side is set. */
export function keepAspect(
  changed: "width" | "height",
  next: number,
  width: number,
  height: number,
): { width: number; height: number } {
  if (width <= 0 || height <= 0) {
    return changed === "width" ? { width: next, height } : { width, height: next };
  }
  return changed === "width"
    ? { width: next, height: tidy((next * height) / width) }
    : { width: tidy((next * width) / height), height: next };
}

/** An angle normalised to 0 up to (not including) 360. */
export function normaliseAngle(angle: number): number {
  const a = angle % 360;
  return tidy(a < 0 ? a + 360 : a);
}
