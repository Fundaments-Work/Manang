/**
 * Self-healing, the pure parts: what to ask the `heal` plugin, how to read its
 * answer, what to say after a fix and whether the fix worked.
 *
 * The plugin returns fixes with UTF-16 edits of the text it was sent; the
 * stateful side (`state/heal.svelte.ts`) owns requests, the editor and the
 * notice. Nothing here touches the editor or the kernel, so it is testable in
 * isolation.
 */

import type { FixChoice } from "$lib/editor/diagnostics";
import type {
  HealDiagnosticIn,
  HealEdit,
  HealFinding,
  HealFix,
  HealSuggestion,
} from "$lib/kernel/healApi";

import type { Problem } from "./problems";

/** `source` of the problems this module makes from scanned findings. */
export const HEAL_SOURCE = "heal";

/**
 * Apply `edits` (UTF-16 offsets of `text`) and return the new text. Edits that
 * fall outside the text or overlap an earlier one are skipped, so a stale fix
 * cannot throw. Insertions at one offset keep their order.
 */
export function applyEdits(text: string, edits: readonly HealEdit[]): string {
  const sorted = [...edits].sort((a, b) => a.from - b.from || a.to - b.to);
  let out = "";
  let cursor = 0;
  for (const edit of sorted) {
    if (edit.from < cursor || edit.to < edit.from || edit.to > text.length) continue;
    out += text.slice(cursor, edit.from) + edit.insert;
    cursor = edit.to;
  }
  return out + text.slice(cursor);
}

/** The compile problems of one file as `heal.suggest` diagnostics. */
export function diagnosticsFor(problems: readonly Problem[]): HealDiagnosticIn[] {
  return problems.map((p) => ({
    severity: p.severity === "info" ? "warning" : p.severity,
    message: p.message,
    line: p.line,
    column: p.column,
    ...(p.range === undefined ? {} : { range: p.range }),
    hints: p.hints,
  }));
}

/** A finding of a text scan (Markdown) as a Problem, so it is listed and marked like any other. */
export function findingProblem(
  file: string,
  finding: HealFinding,
  column: number,
): Problem {
  return {
    id: `${HEAL_SOURCE}:${file}:${finding.rule}:${finding.from}`,
    severity: finding.severity,
    message: finding.message,
    file,
    line: finding.line,
    column,
    hints: [],
    source: HEAL_SOURCE,
    code: finding.rule,
  };
}

/** The fixes of one problem, certain ones first. */
export function fixesOf(
  suggestion: HealSuggestion,
  diagnosticIndex: number | null,
  findingIndex: number | null,
): HealFix[] {
  return suggestion.fixes.filter(
    (fix) =>
      (diagnosticIndex !== null && fix.diagnostics.includes(diagnosticIndex)) ||
      (findingIndex !== null && fix.finding === findingIndex),
  );
}

export function choiceOf(fix: HealFix): FixChoice {
  return { id: fix.id, label: fix.label, why: fix.why, confidence: fix.confidence };
}

const PAST: Readonly<Record<string, string>> = {
  Add: "Added",
  Change: "Changed",
  Remove: "Removed",
  Import: "Imported",
  Close: "Closed",
  Make: "Made",
  Declare: "Declared",
  Escape: "Escaped",
};

/** "Add \usepackage{graphicx}" as "Added \usepackage{graphicx}." for the notice. */
export function doneText(label: string): string {
  const [first = "", ...rest] = label.split(" ");
  const past = PAST[first];
  const sentence = past === undefined ? `Applied: ${label}` : [past, ...rest].join(" ");
  return `${sentence}.`;
}

export type Verdict = "worked" | "no-progress";

/**
 * Did a fix work? After it, the normal recompile runs; the fix worked when the
 * errors it targeted are gone or there are fewer errors than before. TeX stops
 * at the first error, so a fix often reveals the next one without lowering the
 * count: that still counts as working because its own error is gone.
 */
export function verdict(
  targeted: readonly string[],
  errorsBefore: number,
  after: readonly Problem[],
): Verdict {
  const errors = after.filter((p) => p.severity === "error");
  const still = errors.some((p) => targeted.includes(p.message));
  return !still || errors.length < errorsBefore ? "worked" : "no-progress";
}

export function noProgressText(label: string): string {
  return `${label} did not clear the error.`;
}
