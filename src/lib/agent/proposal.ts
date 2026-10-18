/**
 * Proposals as diffs. A proposal is one `edit_text` call: one or more hunks of one file, each
 * replacing `before` with `after`. Nothing is applied here: the editor applies the accepted edit
 * as one transaction, so one Ctrl+Z undoes all of it.
 *
 * The host located every hunk in the text the model read and sends UTF-16 offsets, but the
 * editor may have moved on: hunks are found again by their text (the proposed range first, else
 * the single place the text now occurs), and a hunk that cannot be found makes the proposal stale.
 */

import type { EditHunk, TextEdit } from "./types";

export type Segment =
  | { readonly kind: "same"; readonly text: string }
  | { readonly kind: "removed"; readonly text: string }
  | { readonly kind: "inserted"; readonly text: string };

/** The longest common prefix and suffix, never splitting a surrogate pair. */
function commonEnds(a: string, b: string): { prefix: number; suffix: number } {
  const max = Math.min(a.length, b.length);
  let prefix = 0;
  while (prefix < max && a.charCodeAt(prefix) === b.charCodeAt(prefix)) prefix += 1;
  if (prefix > 0 && isHighSurrogate(a.charCodeAt(prefix - 1))) prefix -= 1;
  let suffix = 0;
  while (
    suffix < max - prefix &&
    a.charCodeAt(a.length - 1 - suffix) === b.charCodeAt(b.length - 1 - suffix)
  ) {
    suffix += 1;
  }
  if (suffix > 0 && isLowSurrogate(a.charCodeAt(a.length - suffix))) suffix -= 1;
  return { prefix, suffix };
}

function isHighSurrogate(code: number): boolean {
  return code >= 0xd800 && code <= 0xdbff;
}

function isLowSurrogate(code: number): boolean {
  return code >= 0xdc00 && code <= 0xdfff;
}

/** The diff of one hunk: unchanged lead, removed text, inserted text, unchanged tail. */
export function diffSegments(before: string, after: string): Segment[] {
  const { prefix, suffix } = commonEnds(before, after);
  const out: Segment[] = [];
  const lead = before.slice(0, prefix);
  const removed = before.slice(prefix, before.length - suffix);
  const inserted = after.slice(prefix, after.length - suffix);
  const tail = suffix > 0 ? before.slice(before.length - suffix) : "";
  if (lead !== "") out.push({ kind: "same", text: lead });
  if (removed !== "") out.push({ kind: "removed", text: removed });
  if (inserted !== "") out.push({ kind: "inserted", text: inserted });
  if (tail !== "") out.push({ kind: "same", text: tail });
  return out;
}

/** A change in the editor's coordinates. */
export interface Change {
  readonly from: number;
  readonly to: number;
  readonly insert: string;
}

/** The smallest change that turns the text at `at` into the hunk's result. */
export function minimalChange(
  hunk: Pick<EditHunk, "before" | "after">,
  at: { readonly from: number; readonly to: number },
): Change {
  const { prefix, suffix } = commonEnds(hunk.before, hunk.after);
  return {
    from: at.from + prefix,
    to: at.to - suffix,
    insert: hunk.after.slice(prefix, hunk.after.length - suffix),
  };
}

/**
 * Where a hunk applies in `doc`: the proposed range when the text there is still `before`, else
 * the single place `before` now occurs (the text moved), else null (it changed: stale).
 */
export function locateHunk(
  doc: string,
  hunk: EditHunk,
): { readonly from: number; readonly to: number } | null {
  if (hunk.to <= doc.length && doc.slice(hunk.from, hunk.to) === hunk.before) {
    return { from: hunk.from, to: hunk.to };
  }
  if (hunk.before === "") return null;
  const first = doc.indexOf(hunk.before);
  if (first < 0 || doc.indexOf(hunk.before, first + 1) >= 0) return null;
  return { from: first, to: first + hunk.before.length };
}

/**
 * Every hunk's change in `doc`, in document order, or null when any hunk is stale or two hunks
 * now overlap.
 */
export function locateEdit(doc: string, edit: TextEdit): Change[] | null {
  const found: Change[] = [];
  let ends = -1;
  const located = edit.edits
    .map((hunk) => ({ hunk, at: locateHunk(doc, hunk) }))
    .sort((a, b) => (a.at?.from ?? 0) - (b.at?.from ?? 0));
  for (const { hunk, at } of located) {
    if (at === null || at.from < ends) return null;
    ends = at.to;
    found.push(minimalChange(hunk, at));
  }
  return found.length === 0 ? null : found;
}

/** `Edit to main.typ, 2 changes`: the line the chat shows for a proposal. */
export function describeEdit(edit: TextEdit): string {
  const n = edit.edits.length;
  return `Edit to ${edit.path}, ${n} change${n === 1 ? "" : "s"}`;
}

/** Characters removed and added by all hunks (for a tooltip). */
export function sizeOf(edit: TextEdit): { removed: number; added: number } {
  return edit.edits.reduce(
    (sum, h) => ({
      removed: sum.removed + h.before.length,
      added: sum.added + h.after.length,
    }),
    { removed: 0, added: 0 },
  );
}
