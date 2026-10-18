import { describe, expect, it } from "vitest";

import { contextKeys, whenMatches, type UiContext } from "./context";

const base: UiContext = {
  surface: "workbench",
  project: true,
  editor: "document",
  engine: "typst",
  zen: false,
};

describe("ui context", () => {
  it("lists the keys of the current context", () => {
    expect([...contextKeys(base)].sort()).toEqual(
      ["document", "project", "typst", "workbench"].sort(),
    );
    expect([...contextKeys({ ...base, surface: "home" })].sort()).toEqual(
      ["home", "project"].sort(),
    );
  });

  it("evaluates when clauses", () => {
    const keys = contextKeys(base);
    expect(whenMatches(undefined, keys)).toBe(true);
    expect(whenMatches("workbench && document", keys)).toBe(true);
    expect(whenMatches("latex || typst", keys)).toBe(true);
    expect(whenMatches("!home", keys)).toBe(true);
    expect(whenMatches("home", keys)).toBe(false);
    expect(whenMatches("figure && workbench", keys)).toBe(false);
    expect(whenMatches("unknown", keys)).toBe(false);
  });
});
