import { history, undo } from "@codemirror/commands";
import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";

import { aiDiff, aiEdit, hideEffect, inlineProposal, showEffect } from "./aiDiff";

const proposal = { id: "p1", from: 4, to: 9, insert: "hi", explanation: "" };

function state(doc = "say hello there") {
  return EditorState.create({ doc, extensions: [history(), aiDiff()] });
}

describe("inline AI diff state", () => {
  it("shows a proposal without touching the document", () => {
    const s = state().update({ effects: showEffect(proposal) }).state;
    expect(inlineProposal(s)).toEqual(proposal);
    expect(s.doc.toString()).toBe("say hello there");
  });

  it("moves with edits elsewhere and is dropped by an edit inside the range", () => {
    let s = state().update({ effects: showEffect(proposal) }).state;
    s = s.update({ changes: { from: 0, insert: "oh, " } }).state;
    expect(inlineProposal(s)).toMatchObject({ from: 8, to: 13 });
    s = s.update({ changes: { from: 9, to: 10, insert: "E" } }).state;
    expect(inlineProposal(s)).toBeNull();
  });

  it("hides by id and ignores another id", () => {
    const shown = state().update({ effects: showEffect(proposal) }).state;
    expect(
      inlineProposal(shown.update({ effects: hideEffect("other") }).state),
    ).not.toBeNull();
    expect(
      inlineProposal(shown.update({ effects: hideEffect("p1") }).state),
    ).toBeNull();
    expect(
      inlineProposal(shown.update({ effects: hideEffect(null) }).state),
    ).toBeNull();
  });

  it("accepting is ONE transaction that one undo reverses", () => {
    const shown = state().update({ effects: showEffect(proposal) }).state;
    const accepted = shown.update({
      changes: { from: 4, to: 9, insert: "hi" },
      effects: hideEffect("p1"),
      annotations: [aiEdit.of(true)],
      userEvent: "input.ai",
    }).state;
    expect(accepted.doc.toString()).toBe("say hi there");
    expect(inlineProposal(accepted)).toBeNull();
    let undone = accepted;
    const ran = undo({
      state: accepted,
      dispatch: (tr) => {
        undone = tr.state;
      },
    });
    expect(ran).toBe(true);
    expect(undone.doc.toString()).toBe("say hello there");
  });
});
