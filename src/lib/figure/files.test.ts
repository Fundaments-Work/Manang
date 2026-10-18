import { describe, expect, it } from "vitest";

import { figureStem, isFigureName, NEW_FIGURE_NAME } from "./files";

describe("figure file names", () => {
  it("recognises the new and the legacy suffix, in any case", () => {
    expect(isFigureName("state.figure")).toBe(true);
    expect(isFigureName("State.Figure")).toBe(true);
    expect(isFigureName("state.figure.json")).toBe(true);
    expect(isFigureName("state.json")).toBe(false);
    expect(isFigureName("figure")).toBe(false);
    expect(isFigureName("notes.figures")).toBe(false);
  });

  it("strips either suffix to the stem", () => {
    expect(figureStem("a.b.figure")).toBe("a.b");
    expect(figureStem("a.figure.json")).toBe("a");
    expect(figureStem("plain.svg")).toBe("plain.svg");
  });

  it("starts new figures with the new extension", () => {
    expect(NEW_FIGURE_NAME).toBe("untitled.figure");
  });
});
