/**
 * Figure colours: the named tokens of `crates/manang-figure/src/palette.json`
 * (the one place they are declared), resolved per theme exactly like
 * `palette.rs`. Figure content uses these tokens; UI chrome never does.
 */

import palette from "../../../crates/manang-figure/src/palette.json";

export type FigureTheme = "dark" | "light" | "print";

interface TokenValues {
  readonly dark: string;
  readonly light: string;
  readonly print: string;
}

const TOKENS: Readonly<Record<string, TokenValues>> = palette.tokens;
const NEUTRAL: ReadonlySet<string> = new Set(palette.neutral);
const TINT: number = palette.tint;

/** Every token name, sorted like Rust's `BTreeMap`. */
export const TOKEN_NAMES: readonly string[] = Object.keys(TOKENS).sort();

/** Neutral tokens fill solid; hues fill as a tint. */
export const NEUTRAL_TOKENS: readonly string[] = palette.neutral;

/** Hue tokens, for swatches. */
export const HUE_TOKENS: readonly string[] = TOKEN_NAMES.filter((t) => !NEUTRAL.has(t));

export function isToken(name: string): boolean {
  return Object.hasOwn(TOKENS, name);
}

function isHex(value: string): boolean {
  return /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(value);
}

/** `none`, a token or a `#rgb`/`#rrggbb` literal. */
export function isColour(value: string): boolean {
  return value === "none" || isToken(value) || isHex(value);
}

function resolve(value: string, theme: FigureTheme): string | undefined {
  if (isHex(value)) return value.toLowerCase();
  const token = TOKENS[value];
  return token === undefined ? undefined : token[theme];
}

/** The SVG colour of a stroke value; unknown values fall back to `ink`. */
export function strokeColour(value: string, theme: FigureTheme): string {
  return resolve(value, theme) ?? resolve("ink", theme) ?? "";
}

export interface Paint {
  readonly colour: string;
  readonly alpha: number;
}

/** The SVG paint of a fill value: hue tokens tint, the rest paint as they are. */
export function fillPaint(value: string, theme: FigureTheme): Paint {
  if (value === "none") return { colour: "none", alpha: 1 };
  const alpha = isToken(value) && !NEUTRAL.has(value) ? TINT : 1;
  const colour = resolve(value, theme);
  return colour === undefined ? { colour: "none", alpha: 1 } : { colour, alpha };
}

/** The raw value of a token in a theme (for tests and swatches). */
export function tokenValue(name: string, theme: FigureTheme): string | undefined {
  return TOKENS[name]?.[theme];
}
