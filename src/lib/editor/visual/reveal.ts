/**
 * Cursor reveal: when the source of a construct shows.
 *
 * The rule of Obsidian's Live Preview and Overleaf's Visual Editor: syntax is
 * hidden until the selection touches the construct, then shown so it can be
 * edited. "Touches" includes the construct's own ends, so a caret right after
 * a closing `**` already reveals it (the caret is where typing would extend it).
 *
 * Block constructs (heading, list item, quote) reveal per line, so a caret in
 * one item of a list does not unhide every marker of the list.
 */

import type { Text } from "@codemirror/state";

import { SPAN_KIND, type Span } from "./protocol";

export interface Sel {
  readonly from: number;
  readonly to: number;
}

/** Whether any selection range touches `[from, to]` (inclusive at both ends). */
export function touches(selection: readonly Sel[], from: number, to: number): boolean {
  for (const range of selection) {
    if (range.from <= to && range.to >= from) return true;
  }
  return false;
}

/** Kinds whose markers belong to a line, not to the whole construct. */
const LINE_SCOPED: ReadonlySet<number> = new Set([
  SPAN_KIND.heading,
  SPAN_KIND.listItem,
  SPAN_KIND.quote,
]);

/** Kinds the web view never decorates inside (only their markers). */
const LEAVES: ReadonlySet<number> = new Set([
  SPAN_KIND.math,
  SPAN_KIND.code,
  SPAN_KIND.codeBlock,
  SPAN_KIND.opaque,
  SPAN_KIND.frontmatter,
  SPAN_KIND.comment,
  SPAN_KIND.image,
  SPAN_KIND.rule,
  SPAN_KIND.table,
  SPAN_KIND.ref,
  SPAN_KIND.label,
  SPAN_KIND.cite,
]);

export const isLeaf = (kind: number): boolean => LEAVES.has(kind) || kind >= 1024;

/** Whether a marker of `parent` shows its source for this selection. */
export function markerRevealed(
  doc: Text,
  selection: readonly Sel[],
  parent: Span | undefined,
  marker: Span,
): boolean {
  // A marker with no construct around it is not ours to hide.
  if (parent === undefined) return true;
  if (LINE_SCOPED.has(parent.kind as number)) {
    return touches(
      selection,
      doc.lineAt(marker.start).from,
      doc.lineAt(Math.max(marker.start, marker.end - 1)).to,
    );
  }
  return touches(selection, parent.start, parent.end);
}

/** Whether a replaced construct (image, formula, rule, front matter) shows its source. */
export function sourceRevealed(
  doc: Text,
  selection: readonly Sel[],
  span: Span,
): boolean {
  if (span.kind === SPAN_KIND.rule) {
    return touches(selection, doc.lineAt(span.start).from, doc.lineAt(span.end).to);
  }
  return touches(selection, span.start, span.end);
}
