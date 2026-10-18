/**
 * The inline diff of an AI proposal.
 *
 * The removed text is dimmed and struck through, the inserted text follows in the Sage token
 * (`--mng-math`), and a small widget carries Accept and Reject. Nothing in the document changes
 * until Accept: then ONE transaction replaces the range, so one Ctrl+Z undoes the whole edit.
 * Esc rejects. Editing inside the proposed range drops the proposal (it is stale).
 *
 * The extension holds no logic about the agent: the buttons call the handlers the agent state
 * registered, and `proposeInline` / `clearInline` are the only ways in.
 */

import type { EditorState } from "@codemirror/state";
import {
  Annotation,
  Prec,
  StateEffect,
  StateField,
  Transaction,
  type Extension,
} from "@codemirror/state";
import {
  Decoration,
  EditorView,
  WidgetType,
  keymap,
  type DecorationSet,
} from "@codemirror/view";

/** What the editor shows for one pending proposal. */
export interface InlineProposal {
  readonly id: string;
  readonly from: number;
  readonly to: number;
  /** The text the proposal inserts. */
  readonly insert: string;
  readonly explanation: string;
}

export interface InlineHandlers {
  accept(id: string): void;
  reject(id: string): void;
}

let handlers: InlineHandlers | null = null;

/** The agent state registers what the Accept and Reject buttons do. */
export function setInlineHandlers(next: InlineHandlers | null): void {
  handlers = next;
}

const show = StateEffect.define<InlineProposal>();
const hide = StateEffect.define<string | null>();

/** Marks the accepting transaction, so it is easy to find in tests and the undo history. */
export const aiEdit = Annotation.define<boolean>();

class ProposalWidget extends WidgetType {
  constructor(readonly proposal: InlineProposal) {
    super();
  }

  override eq(other: ProposalWidget): boolean {
    return (
      other.proposal.id === this.proposal.id &&
      other.proposal.insert === this.proposal.insert
    );
  }

  override toDOM(): HTMLElement {
    const root = document.createElement("span");
    root.className = "mng-ai-inline";
    root.setAttribute("data-ai-proposal", this.proposal.id);
    if (this.proposal.insert !== "") {
      const inserted = document.createElement("span");
      inserted.className = "mng-ai-inserted";
      inserted.textContent = this.proposal.insert;
      root.append(inserted);
    }
    const bar = document.createElement("span");
    bar.className = "mng-ai-bar";
    bar.setAttribute("role", "group");
    bar.setAttribute("aria-label", "AI proposal");
    const accept = document.createElement("button");
    accept.type = "button";
    accept.className = "mng-ai-btn";
    accept.textContent = "Accept";
    accept.setAttribute("data-ai-accept", "");
    accept.title = this.proposal.explanation || "Accept this change (Ctrl+Z undoes it)";
    accept.addEventListener("mousedown", (e) => e.preventDefault());
    accept.addEventListener("click", () => handlers?.accept(this.proposal.id));
    const reject = document.createElement("button");
    reject.type = "button";
    reject.className = "mng-ai-btn";
    reject.textContent = "Reject";
    reject.setAttribute("data-ai-reject", "");
    reject.title = "Reject this change (Esc)";
    reject.addEventListener("mousedown", (e) => e.preventDefault());
    reject.addEventListener("click", () => handlers?.reject(this.proposal.id));
    bar.append(accept, reject);
    root.append(bar);
    return root;
  }

  override ignoreEvent(): boolean {
    return false;
  }
}

interface Field {
  readonly proposal: InlineProposal | null;
  readonly decorations: DecorationSet;
}

function build(proposal: InlineProposal): DecorationSet {
  const ranges = [];
  if (proposal.to > proposal.from) {
    ranges.push(
      Decoration.mark({ class: "mng-ai-removed" }).range(proposal.from, proposal.to),
    );
  }
  ranges.push(
    Decoration.widget({ widget: new ProposalWidget(proposal), side: 1 }).range(
      proposal.to,
    ),
  );
  return Decoration.set(ranges, true);
}

const field = StateField.define<Field>({
  create: () => ({ proposal: null, decorations: Decoration.none }),
  update(value, tr) {
    let proposal = value.proposal;
    for (const effect of tr.effects) {
      if (effect.is(show)) proposal = effect.value;
      if (effect.is(hide) && (effect.value === null || effect.value === proposal?.id)) {
        proposal = null;
      }
    }
    if (proposal === null) {
      return value.proposal === null
        ? value
        : { proposal: null, decorations: Decoration.none };
    }
    if (tr.docChanged && !tr.effects.some((e) => e.is(show))) {
      // An edit touching the proposed range makes it stale; edits elsewhere just move it.
      let touched = false;
      tr.changes.iterChangedRanges((fromA, toA) => {
        if (fromA <= proposal!.to && toA >= proposal!.from) touched = true;
      });
      if (touched) return { proposal: null, decorations: Decoration.none };
      proposal = {
        ...proposal,
        from: tr.changes.mapPos(proposal.from, 1),
        to: tr.changes.mapPos(proposal.to, -1),
      };
    }
    return { proposal, decorations: build(proposal) };
  },
  provide: (f) => EditorView.decorations.from(f, (v) => v.decorations),
});

/** The extension to add to every editor state. */
export function aiDiff(): Extension {
  return [
    field,
    // Highest: the default Escape collapses a selection first and would swallow the key.
    Prec.highest(
      keymap.of([
        {
          key: "Escape",
          run: (view) => {
            const proposal = view.state.field(field, false)?.proposal;
            if (proposal === null || proposal === undefined) return false;
            handlers?.reject(proposal.id);
            return true;
          },
        },
      ]),
    ),
  ];
}

/** The effect that shows a proposal (exported for tests that drive a bare `EditorState`). */
export function showEffect(proposal: InlineProposal): StateEffect<InlineProposal> {
  return show.of(proposal);
}

/** The effect that hides one proposal, or any when `id` is null. */
export function hideEffect(id: string | null): StateEffect<string | null> {
  return hide.of(id);
}

/** Shows `proposal` in `view`, replacing any other. */
export function proposeInline(view: EditorView, proposal: InlineProposal): void {
  view.dispatch({
    effects: [
      show.of(proposal),
      EditorView.scrollIntoView(proposal.from, { y: "center" }),
    ],
  });
}

/** Removes the inline diff (of one proposal, or any when `id` is null). */
export function clearInline(view: EditorView, id: string | null = null): void {
  if (view.state.field(field, false)?.proposal == null) return;
  view.dispatch({ effects: hide.of(id) });
}

/** The proposal currently shown, if any. */
export function inlineProposal(state: EditorState): InlineProposal | null {
  return state.field(field, false)?.proposal ?? null;
}

/**
 * Accepts the shown proposal: ONE transaction replaces the range and clears the diff, so a single
 * undo restores the text. `change` is in the current document's coordinates.
 */
export function applyInline(
  view: EditorView,
  change: { readonly from: number; readonly to: number; readonly insert: string },
  id: string,
): void {
  view.dispatch({
    changes: change,
    selection: { anchor: change.from + change.insert.length },
    effects: hide.of(id),
    annotations: [aiEdit.of(true), Transaction.userEvent.of("input.ai")],
    scrollIntoView: true,
  });
}
