/**
 * Kernel lint results.
 *
 * Plugins that implement an `on_lint`-style pass expose it as the command
 * `lint.diagnostics`, returning `{file, line, column, severity, message}[]`
 * (1-based line and column; the column counts Unicode scalars, like the rest of
 * the kernel). The command is optional: when no plugin provides it, nothing is
 * shown and nothing fails.
 */

import type { Diagnostic } from "$lib/editor/diagnostics";

import type { Kernel } from "./client";
import { scalarToUtf16 } from "./offsets";

export const LINT_COMMAND = "lint.diagnostics";

export interface LintItem {
  readonly file: string;
  readonly line: number;
  readonly column: number;
  readonly severity: "error" | "warning";
  readonly message: string;
}

/** Validate untrusted kernel output; malformed entries are dropped. */
export function parseLint(raw: unknown): LintItem[] {
  if (!Array.isArray(raw)) return [];
  const items: LintItem[] = [];
  for (const entry of raw as unknown[]) {
    if (typeof entry !== "object" || entry === null) continue;
    const e = entry as Record<string, unknown>;
    if (
      typeof e["line"] !== "number" ||
      typeof e["column"] !== "number" ||
      typeof e["message"] !== "string" ||
      !Number.isInteger(e["line"]) ||
      !Number.isInteger(e["column"]) ||
      e["line"] < 1 ||
      e["column"] < 1
    ) {
      continue;
    }
    items.push({
      file: typeof e["file"] === "string" ? e["file"] : "",
      line: e["line"],
      column: e["column"],
      // The gutter knows two severities; anything else reads as a warning.
      severity: e["severity"] === "error" ? "error" : "warning",
      message: e["message"],
    });
  }
  return items;
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

    out.push({ from, to, message: item.message, severity: item.severity });
  }
  return out;
}

/** Ask the kernel for diagnostics. Absent or failing command yields `[]`. */
export async function fetchLint(kernel: Kernel, file?: string): Promise<LintItem[]> {
  try {
    const raw = await kernel.run(LINT_COMMAND, file === undefined ? {} : { file });
    return parseLint(raw);
  } catch {
    return [];
  }
}
