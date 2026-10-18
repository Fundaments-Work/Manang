/**
 * Live co-editing, view side: apply remote edits and draw remote carets.
 *
 * The Rust core owns the CRDT and the rope. The web view stays thin: it sends
 * rope-level edits (`binding.ts` does that, with the version handshake) and
 * receives two kinds of event from Rust:
 *
 *  - a list of remote changes, already applied to the rope, in UTF-16 units
 *    (`AppliedChange.utf16_start / utf16_delete / insert` in `manang-live`);
 *  - the resolved carets of the other people in the room.
 *
 * This module turns the first into ONE CodeMirror transaction that is
 *
 *  - marked `fromKernel` so the kernel sync does not send it back,
 *  - marked `Transaction.remote`, and
 *  - kept out of the undo history (`addToHistory: false`), so CodeMirror's
 *    history only ever undoes *local* edits and maps its stack over remote ones.
 *
 * and draws the second as square one-pixel carets and tonal selection tints
 * using only existing design tokens (no new colours, no radius, no shadow).
 */

import {
  ChangeSet,
  StateEffect,
  StateField,
  Transaction,
  type EditorState,
  type TransactionSpec,
} from "@codemirror/state";
import {
  Decoration,
  EditorView,
  WidgetType,
  type DecorationSet,
} from "@codemirror/view";

import { fromKernel } from "$lib/editor/kernelSync";

/**
 * One replacement in the document as it stands after all earlier changes in
 * the same list. UTF-16 code units, which is what CodeMirror indexes.
 */
export interface RemoteChange {
  readonly start: number;
  readonly delete: number;
  readonly insert: string;
}

/**
 * Compose sequential changes into a single `ChangeSet` against the document
 * of length `docLength`. A `ChangeSet` addresses every change in the *original*
 * document, so each sequential change is composed onto the running set.
 *
 * Throws `RangeError` when a change lies outside the document it was
 * computed against; that means the view and the core diverged and the caller
 * should resync instead of applying.
 */
export function composeRemoteChanges(
  docLength: number,
  changes: readonly RemoteChange[],
): ChangeSet {
  let set = ChangeSet.empty(docLength);
  for (const change of changes) {
    const from = change.start;
    const to = change.start + change.delete;
    if (from < 0 || to < from || to > set.newLength) {
      throw new RangeError(
        `remote change ${from}..${to} is outside a document of length ${set.newLength}`,
      );
    }
    const step = ChangeSet.of({ from, to, insert: change.insert }, set.newLength);
    set = set.compose(step);
  }
  return set;
}

/**
 * The transaction spec for a batch of remote changes. One batch is one
 * transaction, so the editor repaints once and extensions see one update.
 */
export function remoteTransaction(
  state: EditorState,
  changes: readonly RemoteChange[],
): TransactionSpec {
  return {
    changes: composeRemoteChanges(state.doc.length, changes),
    annotations: [
      fromKernel.of(true),
      Transaction.remote.of(true),
      Transaction.addToHistory.of(false),
    ],
    // Keep the local selection attached to the text it was on.
    scrollIntoView: false,
  };
}

/** Apply remote changes to a live view. */
export function applyRemoteChanges(
  view: EditorView,
  changes: readonly RemoteChange[],
): void {
  if (changes.length === 0) return;
  view.dispatch(remoteTransaction(view.state, changes));
}

/** A remote person's selection, resolved by the Rust core to UTF-16 offsets. */
export interface RemoteSelection {
  /** Stable id of the participant (decimal string: ids are 64-bit). */
  readonly peer: string;
  readonly name: string;
  /** Colour slot, `0..SLOT_COUNT-1`, chosen by the core. */
  readonly slot: number;
  readonly anchor: number;
  readonly head: number;
}

/** Replace the full set of remote selections (an authoritative update). */
export const setRemoteSelections = StateEffect.define<readonly RemoteSelection[]>();

function clamp(pos: number, length: number): number {
  return Math.max(0, Math.min(pos, length));
}

/**
 * Map a selection through a transaction's changes. `assoc = -1` keeps a caret
 * on the left of text inserted exactly at it: a local keystroke at someone
 * else's caret must not drag their caret along. The next authoritative update
 * from the core (which uses the CRDT's stable positions) corrects any
 * remaining drift, so this only has to be good for a few milliseconds.
 */
export function mapRemoteSelection(
  selection: RemoteSelection,
  changes: ChangeSet,
): RemoteSelection {
  if (changes.empty) return selection;
  return {
    ...selection,
    anchor: changes.mapPos(selection.anchor, -1),
    head: changes.mapPos(selection.head, -1),
  };
}

/** The drawable parts of one selection, clamped to the document. */
export interface SelectionGeometry {
  readonly caret: number;
  readonly from: number;
  readonly to: number;
}

export function selectionGeometry(
  selection: RemoteSelection,
  docLength: number,
): SelectionGeometry {
  const anchor = clamp(selection.anchor, docLength);
  const head = clamp(selection.head, docLength);
  return { caret: head, from: Math.min(anchor, head), to: Math.max(anchor, head) };
}

export const remoteSelectionsField = StateField.define<readonly RemoteSelection[]>({
  create: () => [],
  update(value, tr) {
    let next = value;
    if (tr.docChanged) {
      next = next.map((selection) => mapRemoteSelection(selection, tr.changes));
    }
    for (const effect of tr.effects) {
      if (effect.is(setRemoteSelections)) next = effect.value;
    }
    return next;
  },
});

/**
 * Per-slot colour: mixes of tokens that already exist (text, sage "math",
 * background). The action colour (lime) is never used: it means "the one
 * action in this view". The design system has only two hues, so the stroke
 * style also varies per slot and every caret carries its owner's name; colour
 * alone never identifies a person. Needs a design sign-off before shipping.
 */
export const SLOT_COLOURS: readonly string[] = [
  "var(--mng-text)",
  "var(--mng-math)",
  "color-mix(in srgb, var(--mng-math) 55%, var(--mng-text))",
  "color-mix(in srgb, var(--mng-text) 60%, var(--mng-bg))",
  "color-mix(in srgb, var(--mng-math) 70%, var(--mng-bg))",
  "color-mix(in srgb, var(--mng-text) 35%, var(--mng-math))",
  "color-mix(in srgb, var(--mng-text) 80%, var(--mng-bg))",
  "color-mix(in srgb, var(--mng-math) 40%, var(--mng-line))",
];
export const SLOT_STROKES: readonly string[] = [
  "solid",
  "solid",
  "dashed",
  "dashed",
  "dotted",
  "dotted",
  "double",
  "double",
];
export const SLOT_COUNT = SLOT_COLOURS.length;

function slotOf(selection: RemoteSelection): number {
  return ((selection.slot % SLOT_COUNT) + SLOT_COUNT) % SLOT_COUNT;
}

class CaretWidget extends WidgetType {
  constructor(
    readonly peer: string,
    readonly name: string,
    readonly slot: number,
  ) {
    super();
  }

  override eq(other: CaretWidget): boolean {
    return (
      other.peer === this.peer && other.name === this.name && other.slot === this.slot
    );
  }

  override toDOM(): HTMLElement {
    const caret = document.createElement("span");
    caret.className = "cm-remote-caret";
    caret.dataset["peer"] = this.peer;
    caret.style.setProperty(
      "--cm-remote",
      SLOT_COLOURS[this.slot] ?? SLOT_COLOURS[0] ?? "",
    );
    caret.style.borderLeftStyle = SLOT_STROKES[this.slot] ?? "solid";
    const flag = document.createElement("span");
    flag.className = "cm-remote-flag";
    flag.textContent = this.name;
    caret.append(flag);
    return caret;
  }

  override ignoreEvent(): boolean {
    return true;
  }
}

/** Decorations for a set of selections against a document of `docLength`. */
export function buildRemoteDecorations(
  selections: readonly RemoteSelection[],
  docLength: number,
): DecorationSet {
  const ranges = [];
  for (const selection of selections) {
    const { caret, from, to } = selectionGeometry(selection, docLength);
    const slot = slotOf(selection);
    if (from < to) {
      ranges.push(
        Decoration.mark({
          class: "cm-remote-selection",
          attributes: {
            style: `--cm-remote: ${SLOT_COLOURS[slot] ?? SLOT_COLOURS[0] ?? ""}`,
          },
        }).range(from, to),
      );
    }
    ranges.push(
      Decoration.widget({
        widget: new CaretWidget(selection.peer, selection.name, slot),
        side: 1,
      }).range(caret),
    );
  }
  return Decoration.set(ranges, true);
}

const remoteTheme = EditorView.baseTheme({
  ".cm-remote-caret": {
    position: "relative",
    display: "inline-block",
    width: "0",
    height: "1.2em",
    verticalAlign: "text-bottom",
    marginLeft: "-1px",
    borderLeft: "1px solid var(--cm-remote)",
    pointerEvents: "none",
  },
  ".cm-remote-flag": {
    position: "absolute",
    bottom: "100%",
    left: "-1px",
    whiteSpace: "nowrap",
    padding: "0 4px",
    fontFamily: "var(--font-sans)",
    fontSize: "10px",
    lineHeight: "14px",
    fontWeight: "400",
    color: "var(--mng-text)",
    backgroundColor: "var(--mng-surface-hi)",
    border: "1px solid var(--cm-remote)",
  },
  ".cm-remote-selection": {
    backgroundColor: "color-mix(in srgb, var(--cm-remote) 20%, transparent)",
  },
});

/** Extension: state, decorations and theme for remote carets. */
export function remoteSelectionsExtension() {
  return [
    remoteSelectionsField,
    EditorView.decorations.compute([remoteSelectionsField], (state) =>
      buildRemoteDecorations(state.field(remoteSelectionsField), state.doc.length),
    ),
    remoteTheme,
  ];
}
