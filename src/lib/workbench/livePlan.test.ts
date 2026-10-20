import { describe, expect, it } from "vitest";

import type { LivePlanFile } from "$lib/kernel/liveApi";

import {
  buildChoices,
  canJoin,
  confirmLabel,
  groupPlan,
  planSummary,
  progressText,
  replacedCount,
  replaceSentence,
} from "./livePlan";

const plan: LivePlanFile[] = [
  { path: "main.typ", class: "differing" },
  { path: "notes.typ", class: "differing" },
  { path: "refs.bib", class: "identical" },
  { path: "ch/one.typ", class: "missing" },
  { path: "scratch.typ", class: "guest-only" },
];

describe("join plan", () => {
  it("groups the files and words the summary", () => {
    const g = groupPlan(plan);
    expect(g.differing).toEqual(["main.typ", "notes.typ"]);
    expect(g.guestOnly).toEqual(["scratch.typ"]);
    expect(planSummary(g)).toBe("1 the same, 1 new for you, 2 different, 1 only yours");
    expect(planSummary(groupPlan([]))).toBe("The host has no files yet.");
  });

  it("defaults to the host's version and keeping own files private", () => {
    expect(replacedCount(plan, {})).toBe(2);
    const choices = buildChoices(plan, {}, true) ?? [];
    expect(choices).toContainEqual({ path: "main.typ", action: "take-host" });
    expect(choices).toContainEqual({ path: "scratch.typ", action: "keep-private" });
    expect(choices.some((c) => c.path === "refs.bib")).toBe(false);
  });

  it("never replaces more than one file without the all-files confirmation", () => {
    expect(canJoin(plan, {}, false)).toBe(false);
    expect(buildChoices(plan, {}, false)).toBeNull();
    const choices = buildChoices(plan, {}, true) ?? [];
    expect(choices).toContainEqual({ path: "*", action: "take-host" });
  });

  it("needs no confirmation for one replaced file or none", () => {
    expect(canJoin(plan, { "notes.typ": "keep-mine" }, false)).toBe(true);
    const one = buildChoices(plan, { "notes.typ": "keep-mine" }, false) ?? [];
    expect(one.some((c) => c.path === "*")).toBe(false);
    expect(
      canJoin(plan, { "main.typ": "keep-mine", "notes.typ": "keep-mine" }, false),
    ).toBe(true);
  });

  it("shares a guest-only file only when asked", () => {
    const choices = buildChoices(plan, { "scratch.typ": "share" }, true) ?? [];
    expect(choices).toContainEqual({ path: "scratch.typ", action: "share" });
  });

  it("says plainly where the old copies go", () => {
    expect(replaceSentence(1)).toMatch(/old copy is saved to the project's trash/);
    expect(replaceSentence(3)).toMatch(/old copies are saved to the project's trash/);
    expect(replaceSentence(0)).toMatch(/kept as they are/);
    expect(confirmLabel(2)).toBe("Replace 2 of my files with the host's versions");
    expect(progressText(12, 80)).toBe("Fetching files 12/80");
  });
});
