/** The agent UI is held to the same Fundaments.work rules as every component (see designRules.test.ts). */
/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

const FILES = [
  ...readdirSync("src/lib/components/agent").map((f) =>
    join("src/lib/components/agent", f),
  ),
  "src/lib/editor/aiDiff.ts",
  "src/lib/state/agent.svelte.ts",
];

describe("agent UI follows the design system", () => {
  it("introduces no hex colours, shadows, rounded corners or heavy weights", () => {
    for (const file of FILES) {
      const text = readFileSync(file, "utf8");
      expect(text, `${file}: hex colour`).not.toMatch(/#[0-9a-fA-F]{6}\b/);
      expect(text, `${file}: shadow`).not.toMatch(
        /\b(shadow|drop-shadow)(-[a-z0-9]+)?\b/,
      );
      expect(text, `${file}: rounded`).not.toMatch(
        /\brounded(-[a-z0-9]+)?\b|border-radius/,
      );
      expect(text, `${file}: weight`).not.toMatch(/\bfont-(medium|semibold|bold)\b/);
      expect(text, `${file}: Tailwind hover accent`).not.toMatch(/hover:text-accent/);
    }
  });

  it("the diff styles use tokens only", () => {
    const css = readFileSync("src/app.css", "utf8");
    const block = css.slice(css.indexOf(".mng-ai-removed"));
    expect(block).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
    expect(block).not.toMatch(/box-shadow|border-radius/);
  });
});
