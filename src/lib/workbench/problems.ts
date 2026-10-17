/**
 * The problems model: compile diagnostics from the kernel, normalised for the
 * Problems panel, the status bar and the editor gutter.
 */

import type { Diagnostic } from "$lib/editor/diagnostics";
import { toEditorDiagnostics, type LintItem } from "$lib/kernel/lint";
import type { CompileReport, WireDiagnostic } from "$lib/kernel/types";

export type Severity = "error" | "warning";

export interface Problem {
  readonly id: string;
  readonly severity: Severity;
  readonly message: string;
  /** Project-relative path, or "" when the diagnostic names no project file. */
  readonly file: string;
  /** 1-based; null when the compiler gave no position. */
  readonly line: number | null;
  readonly column: number | null;
  readonly hints: readonly string[];
}

export interface Counts {
  readonly errors: number;
  readonly warnings: number;
}

/** `/sections/a.typ` becomes `sections/a.typ`; package files have no project path. */
export function projectPath(file: string | null): string {
  if (file === null || file.startsWith("@")) return "";
  return file.replace(/^\/+/, "");
}

export function fromWire(diagnostics: readonly WireDiagnostic[]): Problem[] {
  return sortProblems(
    diagnostics.map((d, index) => ({
      id: `${index}:${d.file ?? ""}:${d.line ?? 0}:${d.message}`,
      severity: d.severity,
      message: d.message,
      file: projectPath(d.file),
      line: d.line,
      column: d.column,
      hints: d.hints,
    })),
  );
}

export function fromReport(report: CompileReport): Problem[] {
  return fromWire(report.diagnostics);
}

/** Errors first, then by file and position. */
export function sortProblems(problems: readonly Problem[]): Problem[] {
  return [...problems].sort(
    (a, b) =>
      (a.severity === b.severity ? 0 : a.severity === "error" ? -1 : 1) ||
      a.file.localeCompare(b.file) ||
      (a.line ?? 0) - (b.line ?? 0) ||
      (a.column ?? 0) - (b.column ?? 0),
  );
}

export function countProblems(problems: readonly Problem[]): Counts {
  let errors = 0;
  let warnings = 0;
  for (const problem of problems) {
    if (problem.severity === "error") errors += 1;
    else warnings += 1;
  }
  return { errors, warnings };
}

const plural = (n: number, word: string): string => `${n} ${word}${n === 1 ? "" : "s"}`;

/** "No problems", "2 errors", "1 error, 3 warnings". */
export function countLabel(counts: Counts): string {
  if (counts.errors === 0 && counts.warnings === 0) return "No problems";
  const parts: string[] = [];
  if (counts.errors > 0) parts.push(plural(counts.errors, "error"));
  if (counts.warnings > 0) parts.push(plural(counts.warnings, "warning"));
  return parts.join(", ");
}

export function problemsIn(problems: readonly Problem[], file: string): Problem[] {
  return problems.filter((problem) => problem.file === file);
}

/** Gutter markers for one file's text. Problems without a position are skipped. */
export function toGutter(text: string, problems: readonly Problem[]): Diagnostic[] {
  const items: LintItem[] = [];
  for (const problem of problems) {
    if (problem.line === null) continue;
    items.push({
      file: problem.file,
      line: problem.line,
      column: problem.column ?? 1,
      severity: problem.severity,
      message: problem.message,
    });
  }
  return toEditorDiagnostics(text, items);
}

/** Group by file, preserving the sorted order, for the panel's sections. */
export function groupByFile(problems: readonly Problem[]): { file: string; items: Problem[] }[] {
  const groups: { file: string; items: Problem[] }[] = [];
  for (const problem of problems) {
    const last = groups[groups.length - 1];
    if (last !== undefined && last.file === problem.file) last.items.push(problem);
    else groups.push({ file: problem.file, items: [problem] });
  }
  return groups;
}
