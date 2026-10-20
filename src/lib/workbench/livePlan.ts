/**
 * The join plan of a project session, as the dialog decides it: which files
 * fall in which group, what each choice defaults to, and the choices sent to
 * the plugin. Plain functions, tested without a kernel.
 *
 * The rules (owner decisions, 2026-10-20): per file the host's version wins and
 * the guest's old copy goes to a trash folder first; a join that replaces more
 * than one file needs one explicit all-files confirmation (the `*` choice);
 * files only the guest has stay private unless the guest says otherwise.
 */

import type { LiveChoice, LiveChoiceAction, LivePlanFile } from "$lib/kernel/liveApi";

/** The wildcard path of the one all-files confirmation. */
export const ALL_FILES = "*";

export interface PlanGroups {
  /** The same on both sides: nothing to decide. */
  readonly identical: readonly string[];
  /** Only the host has them: they are added here. */
  readonly missing: readonly string[];
  /** Both have them and they differ: the guest decides. */
  readonly differing: readonly string[];
  /** Only the guest has them: kept private unless shared. */
  readonly guestOnly: readonly string[];
}

export function groupPlan(plan: readonly LivePlanFile[]): PlanGroups {
  const pick = (c: LivePlanFile["class"]): string[] =>
    plan
      .filter((f) => f.class === c)
      .map((f) => f.path)
      .sort((a, b) => a.localeCompare(b));
  return {
    identical: pick("identical"),
    missing: pick("missing"),
    differing: pick("differing"),
    guestOnly: pick("guest-only"),
  };
}

/** What a file does when the guest has not touched its choice. */
export function defaultAction(cls: LivePlanFile["class"]): LiveChoiceAction | null {
  switch (cls) {
    case "differing":
      return "take-host";
    case "guest-only":
      return "keep-private";
    default:
      return null;
  }
}

/** The action for `path`: the guest's pick, else the default. */
export function actionFor(
  plan: readonly LivePlanFile[],
  picks: Readonly<Record<string, LiveChoiceAction>>,
  path: string,
): LiveChoiceAction | null {
  const file = plan.find((f) => f.path === path);
  if (file === undefined) return null;
  return picks[path] ?? defaultAction(file.class);
}

/** How many of the guest's own files this join would replace. */
export function replacedCount(
  plan: readonly LivePlanFile[],
  picks: Readonly<Record<string, LiveChoiceAction>>,
): number {
  return plan.filter(
    (f) => f.class === "differing" && actionFor(plan, picks, f.path) === "take-host",
  ).length;
}

/** More than one replaced file needs the explicit all-files confirmation. */
export function needsConfirmation(replaced: number): boolean {
  return replaced > 1;
}

/** Whether Join may be pressed: the confirmation is given when it is needed. */
export function canJoin(
  plan: readonly LivePlanFile[],
  picks: Readonly<Record<string, LiveChoiceAction>>,
  confirmed: boolean,
): boolean {
  return !needsConfirmation(replacedCount(plan, picks)) || confirmed;
}

/**
 * The choices to send: one for every file that needs an answer, and the `*`
 * confirmation when more than one file is replaced. `null` while the
 * confirmation is still missing: nothing is ever replaced silently.
 */
export function buildChoices(
  plan: readonly LivePlanFile[],
  picks: Readonly<Record<string, LiveChoiceAction>>,
  confirmed: boolean,
): LiveChoice[] | null {
  if (!canJoin(plan, picks, confirmed)) return null;
  const choices: LiveChoice[] = [];
  for (const file of plan) {
    const action = actionFor(plan, picks, file.path);
    if (action !== null) choices.push({ path: file.path, action });
  }
  if (needsConfirmation(replacedCount(plan, picks))) {
    choices.push({ path: ALL_FILES, action: "take-host" });
  }
  return choices;
}

/** "3 the same, 2 new for you, 1 different, 1 only yours". Empty groups are left out. */
export function planSummary(groups: PlanGroups): string {
  const parts: string[] = [];
  if (groups.identical.length > 0) parts.push(`${groups.identical.length} the same`);
  if (groups.missing.length > 0) parts.push(`${groups.missing.length} new for you`);
  if (groups.differing.length > 0) parts.push(`${groups.differing.length} different`);
  if (groups.guestOnly.length > 0) parts.push(`${groups.guestOnly.length} only yours`);
  return parts.length === 0 ? "The host has no files yet." : parts.join(", ");
}

/** The sentence under the differing files: what happens to the old copies. */
export function replaceSentence(replaced: number): string {
  if (replaced === 0) return "Your versions of these files are kept as they are.";
  return `The host's ${replaced === 1 ? "version is" : "versions are"} used. Your old ${replaced === 1 ? "copy is" : "copies are"} saved to a trash folder (live-trash) first, so nothing is lost.`;
}

/** The label of the all-files confirmation. */
export function confirmLabel(replaced: number): string {
  return `Replace ${replaced} of my files with the host's versions`;
}

/** Fetch progress for the status bar: "Fetching files 12/80". */
export function progressText(done: number, total: number): string {
  return `Fetching files ${done}/${total}`;
}
