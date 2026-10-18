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
    // Settings keeps the context it was opened over.
    expect([...contextKeys({ ...base, surface: "settings" })].sort()).toEqual(
      ["document", "project", "settings", "typst"].sort(),
    );
    expect(
      [
        ...contextKeys({ ...base, surface: "settings", editor: null, engine: null }),
      ].sort(),
    ).toEqual(["project", "settings"].sort());
    expect([...contextKeys({ ...base, surface: "help", editor: null, engine: null })].sort()).toEqual(
      ["help", "project"].sort(),
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
