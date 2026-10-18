/**
 * Pure functions and constants for workspace onboarding coach lines.
 */

export type CoachTarget = "editor" | "preview";

export const ALL_COACH_TARGETS: readonly CoachTarget[] = ["editor", "preview"] as const;

export const COACH_MESSAGES: Record<CoachTarget, string> = {
  editor: "Live editor: write Typst, LaTeX, or Markdown with sub-50ms feedback.",
  preview: "Instant preview: double-click any element to jump directly to its source.",
};

export const COACH_LINE_STORAGE_KEY = "manang.coach-lines.dismissed.v1";

export function parseDismissed(raw: unknown): Set<CoachTarget> {
  if (!Array.isArray(raw)) return new Set();
  const set = new Set<CoachTarget>();
  for (const item of raw) {
    if (item === "editor" || item === "preview") {
      set.add(item);
    }
  }
  return set;
}

export function isCoachLineVisible(
  target: CoachTarget,
  settingEnabled: boolean,
  dismissed: ReadonlySet<CoachTarget>,
): boolean {
  if (!settingEnabled) return false;
  return !dismissed.has(target);
}

export function areAllCoachLinesDismissed(
  dismissed: ReadonlySet<CoachTarget>,
): boolean {
  return ALL_COACH_TARGETS.every((t) => dismissed.has(t));
}
