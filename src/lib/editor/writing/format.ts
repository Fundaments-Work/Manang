/**
 * Format document: the Rust formatter returns the whole new text; the editor
 * applies it as ONE transaction (one undo step), and as the smallest set of
 * line edits so the caret, folds and scroll position stay where they were.
 */

import { EditorSelection, type ChangeSpec } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";

import type { WritingApi, WritingSyntax } from "$lib/kernel/api";

import { editingSyntax, writingSyntax } from "./syntax";

export interface FormatPlan {
  readonly changes: readonly { from: number; to: number; insert: string }[];
  /** Where a caret that was at `head` belongs afterwards. */
  readonly cursor: number;
}

/** Trim what two strings share at both ends; returns the differing middles. */
function trimEnds(a: string, b: string): { head: number; a: string; b: string } {
  const max = Math.min(a.length, b.length);
  let head = 0;
  while (head < max && a.charCodeAt(head) === b.charCodeAt(head)) head += 1;
  // Never split a surrogate pair.
  if (head > 0 && isHigh(a.charCodeAt(head - 1))) head -= 1;
  let tail = 0;
  while (
    tail < max - head &&
    a.charCodeAt(a.length - 1 - tail) === b.charCodeAt(b.length - 1 - tail)
  ) {
    tail += 1;
  }
  if (tail > 0 && isLow(a.charCodeAt(a.length - tail))) tail -= 1;
  return { head, a: a.slice(head, a.length - tail), b: b.slice(head, b.length - tail) };
}

const isHigh = (u: number): boolean => u >= 0xd800 && u <= 0xdbff;
const isLow = (u: number): boolean => u >= 0xdc00 && u <= 0xdfff;

interface Hunk {
  /** Old lines `a0..a1` are replaced by new lines `b0..b1`. */
  readonly a0: number;
  readonly a1: number;
  readonly b0: number;
  readonly b1: number;
}

/** Cells of the line-diff table; beyond this the middle is replaced as one hunk. */
const DIFF_CELLS = 1_000_000;

/** Line hunks turning `a` into `b`: shared ends trimmed, the middle diffed by LCS. */
function diffLines(a: readonly string[], b: readonly string[]): Hunk[] {
  const max = Math.min(a.length, b.length);
  let top = 0;
  while (top < max && a[top] === b[top]) top += 1;
  let bottom = 0;
  while (bottom < max - top && a[a.length - 1 - bottom] === b[b.length - 1 - bottom]) {
    bottom += 1;
  }
  const n = a.length - bottom - top;
  const m = b.length - bottom - top;
  if (n === 0 && m === 0) return [];

  const whole = [{ a0: top, a1: top + n, b0: top, b1: top + m }];
  const hunks: Hunk[] = [];
  const flush = (a0: number, a1: number, b0: number, b1: number): void => {
    if (a0 !== a1 || b0 !== b1) hunks.push({ a0, a1, b0, b1 });
  };

  if (n === m) {
    // Same number of lines: each differing line is its own hunk.
    for (let i = 0; i < n; i += 1) {
      if (a[top + i] !== b[top + i]) flush(top + i, top + i + 1, top + i, top + i + 1);
    }
    return hunks;
  }
  if ((n + 1) * (m + 1) > DIFF_CELLS) return whole;

  // Longest common subsequence of the middles.
  const width = m + 1;
  const table = new Uint32Array((n + 1) * width);
  for (let i = n - 1; i >= 0; i -= 1) {
    for (let j = m - 1; j >= 0; j -= 1) {
      table[i * width + j] =
        a[top + i] === b[top + j]
          ? (table[(i + 1) * width + j + 1] ?? 0) + 1
          : Math.max(table[(i + 1) * width + j] ?? 0, table[i * width + j + 1] ?? 0);
    }
  }
  let i = 0;
  let j = 0;
  let a0 = 0;
  let b0 = 0;
  while (i < n || j < m) {
    if (i < n && j < m && a[top + i] === b[top + j]) {
      flush(top + a0, top + i, top + b0, top + j);
      i += 1;
      j += 1;
      a0 = i;
      b0 = j;
    } else if (
      j >= m ||
      (i < n && (table[(i + 1) * width + j] ?? 0) >= (table[i * width + j + 1] ?? 0))
    ) {
      i += 1;
    } else {
      j += 1;
    }
  }
  flush(top + a0, top + i, top + b0, top + j);
  return hunks;
}

function lineStarts(lines: readonly string[]): number[] {
  const starts: number[] = [];
  let offset = 0;
  for (const line of lines) {
    starts.push(offset);
    offset += line.length + 1;
  }
  starts.push(offset); // one past the last line, for ends
  return starts;
}

/**
 * Edits turning `before` into `after`, as small as lines allow: unchanged lines
 * are never touched, a line that only changed inside is edited inside, and
 * `cursor` is where a caret at `head` belongs afterwards (same line and column
 * when its line survives, the start of the replacement when it does not).
 */
export function planFormat(before: string, after: string, head: number): FormatPlan {
  if (before === after) return { changes: [], cursor: head };
  const a = before.split("\n");
  const b = after.split("\n");
  const aStart = lineStarts(a);
  const bStart = lineStarts(b);
  const hunks = diffLines(a, b);
  const changes: { from: number; to: number; insert: string }[] = [];

  for (const h of hunks) {
    if (h.a1 - h.a0 === 1 && h.b1 - h.b0 === 1) {
      const oldLine = a[h.a0] ?? "";
      const trimmed = trimEnds(oldLine, b[h.b0] ?? "");
      const from = (aStart[h.a0] ?? 0) + trimmed.head;
      changes.push({ from, to: from + trimmed.a.length, insert: trimmed.b });
    } else if (h.a0 === h.a1) {
      // Lines added.
      const added = b.slice(h.b0, h.b1).join("\n");
      if (h.a0 >= a.length) {
        changes.push({ from: before.length, to: before.length, insert: `\n${added}` });
      } else {
        const at = aStart[h.a0] ?? 0;
        changes.push({ from: at, to: at, insert: `${added}\n` });
      }
    } else if (h.b0 === h.b1) {
      // Lines removed, with the line break that separated them.
      if (h.a1 >= a.length) {
        const from = h.a0 === 0 ? 0 : (aStart[h.a0] ?? 0) - 1;
        changes.push({ from, to: before.length, insert: "" });
      } else {
        changes.push({ from: aStart[h.a0] ?? 0, to: aStart[h.a1] ?? 0, insert: "" });
      }
    } else {
      const from = aStart[h.a0] ?? 0;
      const to = (aStart[h.a1] ?? 1) - 1;
      changes.push({ from, to, insert: b.slice(h.b0, h.b1).join("\n") });
    }
  }

  return { changes, cursor: mapCaret(head, a, b, aStart, bStart, hunks) };
}

function mapCaret(
  head: number,
  a: readonly string[],
  b: readonly string[],
  aStart: readonly number[],
  bStart: readonly number[],
  hunks: readonly Hunk[],
): number {
  let line = 0;
  while (line + 1 < a.length && (aStart[line + 1] ?? Infinity) <= head) line += 1;
  const column = head - (aStart[line] ?? 0);

  let delta = 0;
  for (const h of hunks) {
    if (line < h.a0) break;
    if (line < h.a1) {
      const oneToOne = h.a1 - h.a0 === h.b1 - h.b0;
      const target = oneToOne ? h.b0 + (line - h.a0) : Math.min(h.b0, b.length - 1);
      const length = (b[target] ?? "").length;
      const col = oneToOne ? Math.min(column, length) : 0;
      return (bStart[target] ?? 0) + col;
    }
    delta += h.b1 - h.b0 - (h.a1 - h.a0);
  }
  const target = Math.min(line + delta, b.length - 1);
  return (bStart[target] ?? 0) + Math.min(column, (b[target] ?? "").length);
}

export type FormatOutcome =
  | { kind: "formatted" }
  | { kind: "unchanged" }
  | { kind: "unsupported"; message: string }
  | { kind: "skipped"; message: string }
  | { kind: "stale" };

export const LATEX_UNSUPPORTED = "Formatting LaTeX is not supported yet.";

/** Run the formatter over the whole document and apply the result as one transaction. */
export async function formatView(
  view: Pick<EditorView, "state" | "dispatch">,
  api: WritingApi,
  detected: WritingSyntax = writingSyntax(view.state),
): Promise<FormatOutcome> {
  const syntax = editingSyntax(detected);
  if (syntax === "latex") return { kind: "unsupported", message: LATEX_UNSUPPORTED };
  if (syntax === null) {
    return { kind: "unsupported", message: "This file type cannot be formatted." };
  }

  const doc = view.state.doc;
  const source = doc.toString();
  const result = await api.format(source, syntax);
  // The author kept typing while the formatter ran: applying would clobber that.
  if (view.state.doc !== doc) return { kind: "stale" };
  if (result.skipped !== undefined) return { kind: "skipped", message: result.skipped };
  if (!result.changed || result.formatted === source) return { kind: "unchanged" };

  const selection = view.state.selection;
  const plan = planFormat(source, result.formatted, selection.main.head);
  const changes: ChangeSpec[] = plan.changes.map((c) => ({ ...c }));
  const set = view.state.changes(changes);
  view.dispatch({
    changes: set,
    selection: selection.main.empty
      ? EditorSelection.cursor(Math.min(plan.cursor, set.newLength))
      : selection.map(set),
    userEvent: "format",
    scrollIntoView: true,
  });
  return { kind: "formatted" };
}
