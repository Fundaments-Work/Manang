/**
 * Proposals as diffs. A text edit replaces `before` (at `from`..`to`) with `after`. Nothing is
 * applied here: the editor applies the accepted edit as one transaction, so one Ctrl+Z undoes it.
 */

import type { TextEdit } from "./types";

export type Segment =
  | { readonly kind: "same"; readonly text: string }
  | { readonly kind: "removed"; readonly text: string }
  | { readonly kind: "inserted"; readonly text: string };

/** Splits a code-unit string at the longest common prefix and suffix, never inside a surrogate pair. */
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

/**
 * The diff of one edit for the card: unchanged lead, removed text, inserted text, unchanged tail.
 * Words that did not change at the ends are shown as plain text, so the card highlights only
 * what the edit touches.
 */
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

/** The smallest change (editor coordinates) that turns the document into the proposal's result. */
export function minimalChange(
  edit: TextEdit,
  at: { readonly from: number; readonly to: number },
): { from: number; to: number; insert: string } {
  const { prefix, suffix } = commonEnds(edit.before, edit.after);
  return {
    from: at.from + prefix,
    to: at.to - suffix,
    insert: edit.after.slice(prefix, edit.after.length - suffix),
  };
}

/**
 * Where the edit applies in `doc`: the proposed range when the text there is still `before`,
 * else the single place `before` now occurs (the text moved), else null (it changed: stale).
 */
export function locateEdit(
  doc: string,
  edit: TextEdit,
): { readonly from: number; readonly to: number } | null {
  if (edit.to <= doc.length && doc.slice(edit.from, edit.to) === edit.before) {
    return { from: edit.from, to: edit.to };
  }
  if (edit.before === "") return null;
  const first = doc.indexOf(edit.before);
  if (first < 0 || doc.indexOf(edit.before, first + 1) >= 0) return null;
  return { from: first, to: first + edit.before.length };
}

/** A short plain-language line for the card header. */
export function describeEdit(edit: TextEdit): string {
  const removed = edit.before.length;
  const added = edit.after.length;
  if (removed === 0) return `Insert ${added} characters in ${edit.path}`;
  if (added === 0) return `Remove ${removed} characters in ${edit.path}`;
  return `Replace ${removed} characters with ${added} in ${edit.path}`;
}
