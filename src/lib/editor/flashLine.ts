/**
 * Briefly highlight one line, so the eye finds where a jump from the preview
 * landed. Reuses the editor's search-match style (accent tint, no new colour).
 *
 * The field installs itself on first use (`StateEffect.appendConfig`), so the
 * editor's base extensions need not know about it.
 */

import { StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, type DecorationSet } from "@codemirror/view";

const FLASH_MS = 1200;

const setFlash = StateEffect.define<number | null>();

const flashMark = Decoration.line({
  class: "cm-searchMatch cm-searchMatch-selected",
  attributes: { "data-source-flash": "" },
});

const flashField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    let next = value.map(tr.changes);
    for (const effect of tr.effects) {
      if (!effect.is(setFlash)) continue;
      next =
        effect.value === null
          ? Decoration.none
          : Decoration.set([flashMark.range(tr.state.doc.lineAt(effect.value).from)]);
    }
    return next;
  },
  provide: (field) => EditorView.decorations.from(field),
});

const timers = new WeakMap<EditorView, ReturnType<typeof setTimeout>>();

/** Highlight the line holding `pos` for a moment. */
export function flashLine(view: EditorView, pos: number): void {
  const at = Math.max(0, Math.min(pos, view.state.doc.length));
  // A field added by a reconfiguration starts from `create`; the effect has
  // to come in a later transaction.
  if (view.state.field(flashField, false) === undefined) {
    view.dispatch({ effects: StateEffect.appendConfig.of(flashField) });
  }
  view.dispatch({ effects: setFlash.of(at) });
  clearTimeout(timers.get(view));
  timers.set(
    view,
    setTimeout(() => {
      if (view.state.field(flashField, false) !== undefined)
        view.dispatch({ effects: setFlash.of(null) });
    }, FLASH_MS),
  );
}
