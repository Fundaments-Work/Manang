/**
 * The problems model: compile diagnostics from the kernel, normalised for the
 * Problems panel, the status bar and the editor gutter.
 */

import type { Text } from "@codemirror/state";

import type { Diagnostic } from "$lib/editor/diagnostics";
import type { LintFinding } from "$lib/editor/writing/bus";
import { toEditorDiagnostics, type LintItem } from "$lib/kernel/lint";
import { scalarLength } from "$lib/kernel/offsets";
import type { CompileReport, WireDiagnostic } from "$lib/kernel/types";

/** `info` is style advice from the writing tools; it ranks below warnings. */
export type Severity = "error" | "warning" | "info";

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
  /** Who reported it: "writing" for prose lint; absent for compiler diagnostics. */
  readonly source?: string;
  /** Rule id of a writing finding, e.g. `repeated-word`. */
  readonly code?: string;
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

const RANK: Record<Severity, number> = { error: 0, warning: 1, info: 2 };

/** Prose-lint findings of `file` as problems, placed by line and scalar column. */
export function fromWriting(
  file: string,
  doc: Pick<Text, "lineAt">,
  findings: readonly LintFinding[],
): Problem[] {
  return findings.map((finding, index) => {
    const line = doc.lineAt(finding.from);
    const column = scalarLength(line.text.slice(0, finding.from - line.from)) + 1;
    return {
      id: `writing:${index}:${finding.from}:${finding.code}`,
      severity: finding.severity,
      message: finding.message,
      file,
      line: line.number,
      column,
      hints: finding.hints,
      source: "writing",
      code: finding.code,
    };
  });
}

/** Errors first, then by file and position. */
export function sortProblems(problems: readonly Problem[]): Problem[] {
  return [...problems].sort(
    (a, b) =>
      RANK[a.severity] - RANK[b.severity] ||
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
    else if (problem.severity === "warning") warnings += 1;
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
    // Writing findings are underlined in the text, not marked in the gutter.
    if (problem.line === null || problem.source === "writing") continue;
    items.push({
      file: problem.file,
      line: problem.line,
      column: problem.column ?? 1,
      severity: problem.severity === "error" ? "error" : "warning",
      message: problem.message,
    });
  }
  return toEditorDiagnostics(text, items);
}

/** Group by file, preserving the sorted order, for the panel's sections. */
export function groupByFile(
  problems: readonly Problem[],
): { file: string; items: Problem[] }[] {
  const groups: { file: string; items: Problem[] }[] = [];
  for (const problem of problems) {
    const last = groups[groups.length - 1];
    if (last !== undefined && last.file === problem.file) last.items.push(problem);
    else groups.push({ file: problem.file, items: [problem] });
  }
  return groups;
}
