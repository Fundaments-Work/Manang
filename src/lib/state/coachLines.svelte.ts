/**
 * Workspace onboarding coach lines state.
 *
 * Three short coach lines for the main workspace (editor, preview, figures)
 * that help new users get oriented. When dismissed, each one records its
 * dismissal, and when all are dismissed (or when the user turns off the
 * "appearance.coach-lines" setting), they never return.
 */

import { SvelteSet } from "svelte/reactivity";

import { loadJson, saveJson } from "$lib/workbench/persist";
import {
  ALL_COACH_TARGETS,
  COACH_LINE_STORAGE_KEY,
  isCoachLineVisible,
  parseDismissed,
  type CoachTarget,
} from "$lib/workbench/coachLines";
import { settings } from "./settings.svelte";

export {
  ALL_COACH_TARGETS,
  COACH_MESSAGES,
  type CoachTarget,
} from "$lib/workbench/coachLines";

function loadDismissed(): Set<CoachTarget> {
  return parseDismissed(loadJson(COACH_LINE_STORAGE_KEY));
}

export class CoachLinesState {
  #dismissed = new SvelteSet<CoachTarget>(loadDismissed());

  isVisible(target: CoachTarget): boolean {
    const settingVal = settings.value("appearance.coach-lines");
    const enabled = settingVal !== false;
    return isCoachLineVisible(target, enabled, this.#dismissed);
  }

  dismiss(target: CoachTarget): void {
    this.#dismissed.add(target);
    saveJson(COACH_LINE_STORAGE_KEY, Array.from(this.#dismissed));

    if (ALL_COACH_TARGETS.every((t) => this.#dismissed.has(t))) {
      void settings.set("appearance.coach-lines", false, "user");
    }
  }

  dismissAll(): void {
    for (const t of ALL_COACH_TARGETS) {
      this.#dismissed.add(t);
    }
    saveJson(COACH_LINE_STORAGE_KEY, Array.from(this.#dismissed));
    void settings.set("appearance.coach-lines", false, "user");
  }

  reset(): void {
    this.#dismissed.clear();
    saveJson(COACH_LINE_STORAGE_KEY, []);
    void settings.set("appearance.coach-lines", true, "user");
  }
}

export const coachLines = new CoachLinesState();
