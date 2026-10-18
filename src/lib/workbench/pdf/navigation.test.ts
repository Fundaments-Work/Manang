import { describe, expect, it } from "vitest";

import { ARROW_STEP, navigationKey, parsePageInput } from "./navigation";

describe("page box", () => {
  it("reads a 1-based page number into a 0-based index", () => {
    expect(parsePageInput("1", 10)).toBe(0);
    expect(parsePageInput(" 7 ", 10)).toBe(6);
    expect(parsePageInput("7/10", 10)).toBe(6);
    expect(parsePageInput("7 / 10", 10)).toBe(6);
  });

  it("clamps past the end", () => {
    expect(parsePageInput("99", 10)).toBe(9);
  });

  it("rejects what is not a page", () => {
    expect(parsePageInput("", 10)).toBeNull();
    expect(parsePageInput("0", 10)).toBeNull();
    expect(parsePageInput("-2", 10)).toBeNull();
    expect(parsePageInput("2.5", 10)).toBeNull();
    expect(parsePageInput("two", 10)).toBeNull();
    expect(parsePageInput("3", 0)).toBeNull();
  });
});

describe("viewer keys", () => {
  const press = (
    key: string,
    mods: Partial<Record<"ctrlKey" | "metaKey" | "altKey" | "shiftKey", boolean>> = {},
  ) =>
    navigationKey({
      key,
      ctrlKey: false,
      metaKey: false,
      altKey: false,
      shiftKey: false,
      ...mods,
    });

  it("pages, ends and arrows", () => {
    expect(press("PageDown")).toEqual({ kind: "page", delta: 1 });
    expect(press("PageUp")).toEqual({ kind: "page", delta: -1 });
    expect(press("Home")).toEqual({ kind: "first" });
    expect(press("End", { ctrlKey: true })).toEqual({ kind: "last" });
    expect(press("ArrowDown")).toEqual({ kind: "scroll", dx: 0, dy: ARROW_STEP });
    expect(press("ArrowLeft")).toEqual({ kind: "scroll", dx: -ARROW_STEP, dy: 0 });
    expect(press(" ")).toEqual({ kind: "screen", direction: 1 });
    expect(press(" ", { shiftKey: true })).toEqual({ kind: "screen", direction: -1 });
  });

  it("lets other keys and Alt combinations through", () => {
    expect(press("a")).toBeNull();
    expect(press("PageDown", { altKey: true })).toBeNull();
    expect(press("PageDown", { ctrlKey: true })).toBeNull();
    expect(press("ArrowDown", { metaKey: true })).toBeNull();
  });
});
