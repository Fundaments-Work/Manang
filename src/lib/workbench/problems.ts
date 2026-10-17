/**
 * The problems model: compile diagnostics from the kernel, normalised for the
 * Problems panel, the status bar and the editor gutter.
 */

import type { Diagnostic } from "$lib/editor/diagnostics";
import { toEditorDiagnostics, type LintItem } from "$lib/kernel/lint";
import type {
  CompileReport,
  LatexReport,
  MarkdownDiagnostic,
  WireDiagnostic,
} from "$lib/kernel/types";

import { dirname, join } from "./paths";

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

/**
 * LaTeX diagnostics name files relative to the main file's directory; the
 * Problems panel wants project-relative paths. `info` entries are dropped.
 */
export function fromLatexReport(
  report: Pick<LatexReport, "diagnostics">,
  main: string,
): Problem[] {
  const base = dirname(main);
  const problems: Problem[] = [];
  report.diagnostics.forEach((d, index) => {
    if (d.severity === "info") return;
    const file = d.file === null ? main : join(base, projectPath(d.file));
    problems.push({
      id: `${index}:${file}:${d.line ?? 0}:${d.message}`,
      severity: d.severity,
      message: d.message,
      file,
      line: d.line,
      column: null,
      hints: d.hint === null ? [] : [d.hint],
    });
  });
  return sortProblems(problems);
}

/** Markdown diagnostics have a free-form severity string; only the first two count. */
export function fromMarkdown(
  diagnostics: readonly MarkdownDiagnostic[],
  file: string,
): Problem[] {
  const problems: Problem[] = [];
  diagnostics.forEach((d, index) => {
    const severity =
      d.severity === "error" ? "error" : d.severity === "warning" ? "warning" : null;
    if (severity === null) return;
    problems.push({
      id: `${index}:${file}:${d.line ?? 0}:${d.message}`,
      severity,
      message: d.message,
      file,
      line: d.line ?? null,
      column: null,
      hints: [],
    });
  });
  return sortProblems(problems);
}

export interface EngineFailure {
  /** The stable `[code]` prefix of a plugin error, when present. */
  readonly code: string | null;
  /** One sentence for the banner and the Output log. */
  readonly title: string;
  /** What to do about it, when we know. */
  readonly fix: string | null;
}

export const WORKER_BUILD_COMMAND =
  "cargo build -p manang-latex --features engine --bin manang-latex-worker";

const NO_TEX =
  /no tex (installation|distribution)|tex installation|latexmk.*not found|pdflatex.*not found|no latex (engine|backend)/i;

/** Turn a `[code] message` failure of `latex.*` into an actionable message. */
export function explainLatexFailure(message: string): EngineFailure {
  const match = /^\[([a-z_]+)\]\s*(.*)$/s.exec(message);
  const code = match?.[1] ?? null;
  const detail = match?.[2] ?? message;
  if (code === "no_tex" || code === "no_backend" || NO_TEX.test(message)) {
    return {
      code: "no_tex",
      title: "No TeX installation found",
      fix: "Install TeX Live (texlive.org) or MiKTeX (miktex.org) so that latexmk or pdflatex is on your PATH, then compile again.",
    };
  }
  switch (code) {
    case "worker_missing":
    case "worker_spawn":
      return {
        code: "no_tex",
        title: "No TeX installation found",
        fix: `Install TeX Live or MiKTeX, or build the optional built-in engine with \`${WORKER_BUILD_COMMAND}\`, then compile again.`,
      };
    case "bundle_unavailable":
      return {
        code,
        title: "The built-in engine's TeX files are not downloaded yet",
        fix: "Connect to the network once and choose Download TeX files, or install TeX Live or MiKTeX.",
      };
    case "timeout":
      return {
        code,
        title: "LaTeX took too long and was stopped",
        fix: "Check for an infinite macro loop, or compile again.",
      };
    case "worker_crashed":
      return {
        code,
        title: "The LaTeX engine stopped unexpectedly",
        fix: `Compile again. If it repeats, the details are: ${detail}`,
      };
    default:
      return { code, title: detail, fix: null };
  }
}
