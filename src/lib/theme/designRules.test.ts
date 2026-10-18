/**
 * The Fundaments.work rules that a reviewer would otherwise police by hand:
 * radius 0, two font weights, no shadows, no off-palette colours, no
 * all-caps eyebrows, and only classes that exist. Scans every component, so
 * work from any agent or contributor is held to the same standard.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

function svelteFiles(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) out.push(...svelteFiles(path));
    else if (name.endsWith(".svelte")) out.push(path);
  }
  return out;
}

const RULES: readonly { readonly name: string; readonly pattern: RegExp }[] = [
  {
    name: "font weight other than 300/400",
    pattern: /\bfont-(medium|semibold|bold|extrabold|black|thin|extralight)\b/,
  },
  { name: "rounded corner", pattern: /\brounded(-[a-z0-9]+)?\b/ },
  { name: "drop shadow", pattern: /\b(shadow|drop-shadow)(-[a-z0-9]+)?\b/ },
  {
    name: "colour outside the token palette",
    pattern:
      /\b(text|bg|border|ring|fill|stroke)-(red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose|slate|gray|zinc|neutral|stone)-\d{2,3}\b/,
  },
  { name: "all-caps label", pattern: /\b(uppercase|tracking-widest|tracking-wider)\b/ },
  { name: "undefined component class", pattern: /\bmng-input\b/ },
];

describe("design rules", () => {
  const files = svelteFiles("src");

  it("finds the components", () => {
    expect(files.length).toBeGreaterThan(20);
  });

  for (const rule of RULES) {
    it(`no ${rule.name}`, () => {
      const offenders = files.flatMap((file) =>
        readFileSync(file, "utf8")
          .split("\n")
          .map((line, i) => ({ file, line: i + 1, text: line }))
          .filter(
            (l) => rule.pattern.test(l.text) && !l.text.includes("design-rules: allow"),
          )
          .map((l) => `${l.file}:${l.line}: ${l.text.trim()}`),
      );
      expect(offenders).toEqual([]);
    });
  }
});
