/**
 * The state of the visual layer inside an `EditorState`.
 *
 * One field holds the decoded span table the decorations came from, the changes
 * made since that table was produced (`pending`), and the decorations. A
 * keystroke only maps the decorations through the change (cheap, synchronous,
 * so the frame renders at once); the next table replaces them. A selection move
 * rebuilds the decorations from the table moved through `pending`, because the
 * cursor-reveal rule depends on the selection.
 */

import type { ChangeSet } from "@codemirror/state";
import {
  StateEffect,
  StateField,
  type EditorState,
  type Extension,
} from "@codemirror/state";
import { EditorView } from "@codemirror/view";

import { buildDecorations, EMPTY_BUILT, mapSpans, type Built } from "./decorations";
import type { VisualHost } from "./host";
import type { Span } from "./protocol";
import type { SpanTable } from "./table";

/** Replace the table (or drop it with `null`: the document shows as plain source). */
export const setTable = StateEffect.define<SpanTable | null>();

export interface VisualValue {
  readonly table: SpanTable | null;
  /** Changes since `table` was produced, or null when it is current. */
  readonly pending: ChangeSet | null;
  readonly built: Built;
}

const EMPTY: VisualValue = { table: null, pending: null, built: EMPTY_BUILT };

export function visualField(host: VisualHost): {
  field: StateField<VisualValue>;
  extension: Extension;
} {
  const field = StateField.define<VisualValue>({
    create: () => EMPTY,
    update(value, tr) {
      let { table, pending, built } = value;
      let rebuild = false;
      for (const effect of tr.effects) {
        if (effect.is(setTable)) {
          table = effect.value;
          pending = null;
          rebuild = true;
        }
      }
      if (table === null) return rebuild ? EMPTY : value;
      if (!rebuild) {
        if (tr.docChanged) {
          pending = pending === null ? tr.changes : pending.compose(tr.changes);
          built = {
            decorations: built.decorations.map(tr.changes),
            atomic: built.atomic.map(tr.changes),
          };
        } else if (!tr.startState.selection.eq(tr.newSelection)) {
          rebuild = true;
        }
      }
      if (rebuild) {
        built = buildDecorations(
          mapSpans(table.spans, pending),
          tr.state.doc,
          tr.newSelection.ranges,
          host,
        );
      }
      return { table, pending, built };
    },
    provide: (self) => [
      EditorView.decorations.from(self, (value) => value.built.decorations),
      EditorView.atomicRanges.of((view) => view.state.field(self).built.atomic),
    ],
  });
  return { field, extension: field };
}

/** The spans of the current table, moved to the current document. */
export function currentSpans(value: VisualValue): readonly Span[] {
  return value.table === null ? [] : mapSpans(value.table.spans, value.pending);
}

/** The document range the table describes, in current positions. */
export function coverage(value: VisualValue): { from: number; to: number } | null {
  const table = value.table;
  if (table === null) return null;
  const { from, to } = table.header;
  if (value.pending === null) return { from, to };
  return { from: value.pending.mapPos(from, 1), to: value.pending.mapPos(to, -1) };
}

/** The heading level (1-6) of the line at `pos`, or 0. */
export function headingLevelAt(
  state: EditorState,
  value: VisualValue,
  pos: number,
): number {
  const line = state.doc.lineAt(pos);
  for (const span of currentSpans(value)) {
    if (span.kind === 1 && span.start <= line.to && span.end >= line.from) {
      return span.aux;
    }
  }
  return 0;
}
