/**
 * Figure content may use colour, but only through the named tokens declared
 * once in `crates/manang-figure/src/palette.json` (ADR 0012). This rule keeps
 * literal hex codes out of every figure source (tests excepted: they feed
 * version-1 files, which stored hex), and checks that the palette's ink is
 * readable on its paper in every theme.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import palette from "../../../crates/manang-figure/src/palette.json";

function files(dir: string, keep: (name: string) => boolean): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) out.push(...files(path, keep));
    else if (keep(name)) out.push(path);
  }
  return out;
}

const notTest = (name: string) => !/\.test\.ts$/.test(name);

const FIGURE_SOURCES = [
  ...files("src/lib/figure", (n) => /\.(ts|svelte)$/.test(n) && notTest(n)),
  ...files("src/lib/state", (n) => /^figures?\.svelte\.ts$/.test(n)),
  "src/lib/kernel/mockFigure.ts",
  ...files("src/lib/components", (n) => /^Figure.*\.svelte$/.test(n)),
];

const HEX = /#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{3})\b/;

function luminance(hex: string): number {
  const v = Number.parseInt(hex.slice(1), 16);
  const channels = [(v >> 16) & 255, (v >> 8) & 255, v & 255].map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  const [r = 0, g = 0, b = 0] = channels;
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return ((x ?? 0) + 0.05) / ((y ?? 0) + 0.05);
}

describe("figure palette", () => {
  it("finds the figure sources", () => {
    expect(FIGURE_SOURCES.length).toBeGreaterThan(8);
  });

  it("no hex colour literal in figure sources (use palette tokens)", () => {
    const offenders = FIGURE_SOURCES.flatMap((file) =>
      readFileSync(file, "utf8")
        .split("\n")
        .map((text, i) => ({ file, line: i + 1, text }))
        .filter((l) => HEX.test(l.text))
        .map((l) => `${l.file}:${l.line}: ${l.text.trim()}`),
    );
    expect(offenders).toEqual([]);
  });

  it("declares every token for every theme as #rrggbb", () => {
    for (const [name, values] of Object.entries(palette.tokens)) {
      for (const value of [values.dark, values.light, values.print]) {
        expect(value, name).toMatch(/^#[0-9a-f]{6}$/);
      }
    }
    for (const n of palette.neutral) expect(Object.keys(palette.tokens)).toContain(n);
  });

  it("ink reads on paper in each theme (contrast at least 7:1)", () => {
    for (const theme of ["dark", "light", "print"] as const) {
      const ratio = contrast(palette.tokens.ink[theme], palette.tokens.paper[theme]);
      expect(ratio, theme).toBeGreaterThanOrEqual(7);
    }
  });
});
