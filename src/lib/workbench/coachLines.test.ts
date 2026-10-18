import { describe, expect, it } from "vitest";

import {
  ALL_COACH_TARGETS,
  COACH_MESSAGES,
  areAllCoachLinesDismissed,
  isCoachLineVisible,
  parseDismissed,
  type CoachTarget,
} from "./coachLines";

describe("coachLines workbench helpers", () => {
  it("defines messages for every coach target", () => {
    for (const target of ALL_COACH_TARGETS) {
      expect(COACH_MESSAGES[target]).toBeTruthy();
      expect(COACH_MESSAGES[target].length).toBeGreaterThan(15);
    }
  });

  it("parses dismissed targets safely from unknown JSON", () => {
    expect(parseDismissed(null)).toEqual(new Set());
    expect(parseDismissed("not an array")).toEqual(new Set());
    expect(parseDismissed(["editor", "invalid", "preview"])).toEqual(
      new Set(["editor", "preview"]),
    );
  });

  it("calculates visibility based on setting and dismissed set", () => {
    const empty = new Set<CoachTarget>();
    expect(isCoachLineVisible("editor", true, empty)).toBe(true);
    expect(isCoachLineVisible("preview", true, empty)).toBe(true);
    expect(isCoachLineVisible("figures", true, empty)).toBe(true);

    // Disabled setting hides all
    expect(isCoachLineVisible("editor", false, empty)).toBe(false);
    expect(isCoachLineVisible("preview", false, empty)).toBe(false);

    // Dismissed target is hidden even when setting is true
    const dismissedEditor = new Set<CoachTarget>(["editor"]);
    expect(isCoachLineVisible("editor", true, dismissedEditor)).toBe(false);
    expect(isCoachLineVisible("preview", true, dismissedEditor)).toBe(true);
  });

  it("detects when all coach lines are dismissed", () => {
    expect(areAllCoachLinesDismissed(new Set(["editor", "preview"]))).toBe(false);
    expect(
      areAllCoachLinesDismissed(new Set(["editor", "preview", "figures"])),
    ).toBe(true);
  });
});
