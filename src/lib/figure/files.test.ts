import { describe, expect, it } from "vitest";

import {
  figureNameProblem,
  figureStem,
  isFigureName,
  newFigureRequest,
  NEW_FIGURE_NAME,
} from "./files";

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

describe("naming a new figure", () => {
  it("refuses empty names, slashes and a leading dot", () => {
    expect(figureNameProblem("")).not.toBeNull();
    expect(figureNameProblem("  ")).not.toBeNull();
    expect(figureNameProblem("a/b")).not.toBeNull();
    expect(figureNameProblem("a\\b")).not.toBeNull();
    expect(figureNameProblem(".hidden")).not.toBeNull();
    expect(figureNameProblem("orbit")).toBeNull();
  });

  it("asks for a name and offers one Create", () => {
    const request = newFigureRequest("untitled");
    expect(request.input?.value).toBe("untitled");
    expect(request.actions.map((a) => a.id)).toEqual(["create"]);
  });
});
