/**
 * Placing compile diagnostics in editor text.
 *
 * Problems carry 1-based lines and columns counted in Unicode scalars, like the
 * rest of the kernel; the editor wants UTF-16 offsets.
 */

import type { Diagnostic, FixChoice } from "$lib/editor/diagnostics";

import { scalarToUtf16 } from "./offsets";

export interface LintItem {
  readonly file: string;
  readonly line: number;
  readonly column: number;
  readonly severity: "error" | "warning";
  readonly message: string;
  /** The Problem this marker stands for, so a quick fix can find it. */
  readonly problemId?: string;
  readonly fixes?: readonly FixChoice[];
}

/**
 * Place lint items in `text`. The marker spans from the reported column to the
 * end of the token there (at least one character), clamped to the line.
 */
export function toEditorDiagnostics(
  text: string,
  items: readonly LintItem[],
): Diagnostic[] {
  const lineStarts = [0];
  for (let i = 0; i < text.length; i += 1) {
    if (text.charCodeAt(i) === 10) lineStarts.push(i + 1);
  }

  const out: Diagnostic[] = [];
  for (const item of items) {
    const start = lineStarts[item.line - 1];
    if (start === undefined) continue;
    const next = lineStarts[item.line];
    const lineEnd = next === undefined ? text.length : next - 1;
    const line = text.slice(start, lineEnd);

    const col = scalarToUtf16(line, item.column - 1);
    const from = start + Math.min(col, line.length);
    let to = from;
    while (to < lineEnd && !/\s/.test(text.charAt(to))) to += 1;
    if (to <= from) to = Math.min(from + 1, text.length);
    if (to <= from) continue;

    out.push({
      from,
      to,
      message: item.message,
      severity: item.severity,
      ...(item.problemId === undefined ? {} : { problemId: item.problemId }),
      ...(item.fixes === undefined || item.fixes.length === 0
        ? {}
        : { fixes: item.fixes }),
    });
  }
  return out;
}
