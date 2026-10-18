import { describe as suite, expect, it } from "vitest";

import { describe } from "./log";

suite("describe", () => {
  it("keeps an Error's stack, so the terminal shows where it came from", () => {
    const error = new Error("boom");
    expect(describe(error)).toBe(error.stack);
  });

  it("reads a kernel error's message", () => {
    expect(describe({ message: "[timeout] compile took too long" })).toBe(
      "[timeout] compile took too long",
    );
  });

  it("prints strings as they are and other values as JSON", () => {
    expect(describe("plain")).toBe("plain");
    expect(describe({ code: 3 })).toBe('{"code":3}');
  });

  it("survives values JSON cannot print", () => {
    const loop: Record<string, unknown> = {};
    loop["self"] = loop;
    expect(describe(loop)).toBe("[object Object]");
  });
});
