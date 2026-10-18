/**
 * Custom colours: hex and HSL conversion for the in-house picker, and the
 * per-user "recent colours" row. Custom colours come from user input only:
 * nothing here (or anywhere in source) hard-codes one. The scene stores them
 * as lowercase `#rrggbb` next to the palette tokens.
 */

import { isCustomColour, normalizeColour } from "./palette";

export interface Hsl {
  /** Hue, 0 to 360 (exclusive). */
  readonly h: number;
  /** Saturation, 0 to 100. */
  readonly s: number;
  /** Lightness, 0 to 100. */
  readonly l: number;
}

/** `#rrggbb` as HSL; undefined when `hex` is not a hex colour. */
export function hexToHsl(hex: string): Hsl | undefined {
  if (!isCustomColour(hex)) return undefined;
  const full = normalizeColour(hex);
  const [r, g, b] = [1, 3, 5].map(
    (i) => Number.parseInt(full.slice(i, i + 2), 16) / 255,
  ) as [number, number, number];
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return { h: 0, s: 0, l: l * 100 };
  const s = d / (1 - Math.abs(2 * l - 1));
  let h: number;
  if (max === r) h = ((g - b) / d) % 6;
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  h *= 60;
  if (h < 0) h += 360;
  return { h, s: s * 100, l: l * 100 };
}

const clamp = (v: number, lo: number, hi: number): number =>
  Math.min(Math.max(Number.isFinite(v) ? v : lo, lo), hi);

/** HSL as lowercase `#rrggbb`. Out-of-range input is clamped. */
export function hslToHex(hsl: Hsl): string {
  const h = ((clamp(hsl.h, 0, 360) % 360) + 360) % 360;
  const s = clamp(hsl.s, 0, 100) / 100;
  const l = clamp(hsl.l, 0, 100) / 100;
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l - c / 2;
  const [r, g, b] =
    h < 60
      ? [c, x, 0]
      : h < 120
        ? [x, c, 0]
        : h < 180
          ? [0, c, x]
          : h < 240
            ? [0, x, c]
            : h < 300
              ? [x, 0, c]
              : [c, 0, x];
  const byte = (v: number) =>
    Math.round((v + m) * 255)
      .toString(16)
      .padStart(2, "0");
  return `#${byte(r as number)}${byte(g as number)}${byte(b as number)}`;
}

/**
 * What a hex field accepts: three or six hex digits, with or without the
 * leading `#`, in any case, with spaces around. Returns lowercase `#rrggbb`,
 * or undefined while the text is not a colour.
 */
export function parseHexInput(text: string): string | undefined {
  const trimmed = text.trim();
  const candidate = trimmed.startsWith("#") ? trimmed : `#${trimmed}`;
  return isCustomColour(candidate) ? normalizeColour(candidate) : undefined;
}

/** The most recent custom colours kept. */
export const RECENT_LIMIT = 8;

/** `hex` at the front of `recent`, without duplicates, at most [`RECENT_LIMIT`]. */
export function pushRecent(recent: readonly string[], hex: string): string[] {
  const value = normalizeColour(hex);
  if (!isCustomColour(value)) return [...recent];
  return [value, ...recent.filter((c) => c !== value)].slice(0, RECENT_LIMIT);
}

export const RECENT_KEY = "manang.figure.recent-colours";

interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

function defaultStorage(): StorageLike | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}

/** The stored recent colours; empty when storage is missing, blocked or corrupt. */
export function loadRecent(
  storage: StorageLike | undefined = defaultStorage(),
): string[] {
  try {
    const raw = storage?.getItem(RECENT_KEY);
    if (raw === null || raw === undefined) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((c): c is string => typeof c === "string" && isCustomColour(c))
      .map(normalizeColour)
      .slice(0, RECENT_LIMIT);
  } catch {
    return [];
  }
}

/** Stores `recent`; storage that cannot be written is not an error here. */
export function saveRecent(
  recent: readonly string[],
  storage: StorageLike | undefined = defaultStorage(),
): void {
  try {
    storage?.setItem(RECENT_KEY, JSON.stringify(recent.slice(0, RECENT_LIMIT)));
  } catch {
    // Private windows and blocked site data: the row just will not persist.
  }
}
