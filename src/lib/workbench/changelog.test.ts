import { describe, expect, it } from "vitest";

import { latestChanges } from "./changelog";

const SAMPLE = `# Changelog

## [Unreleased]

### Added
- **Figure Studio:** A typed scene graph. More detail follows here.
- Plain item with \`code\` and a [link](https://example.org).

### Fixed
- Saving no longer says the file was deleted.

## [0.0.1] - 2026-01-01
- Old thing.
`;

describe("what's new", () => {
  it("lists the newest section only, cut at the first sentence", () => {
    const changes = latestChanges(SAMPLE);
    expect(changes.heading).toBe("Unreleased");
    expect(changes.items).toEqual([
      "Figure Studio: A typed scene graph.",
      "Plain item with code and a link.",
      "Saving no longer says the file was deleted.",
    ]);
  });

  it("honours the limit and copes with no sections", () => {
    expect(latestChanges(SAMPLE, 1).items).toHaveLength(1);
    expect(latestChanges("nothing here")).toEqual({ heading: "", items: [] });
  });
});
