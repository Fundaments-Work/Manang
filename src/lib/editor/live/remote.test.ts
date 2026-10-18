import { history, isolateHistory, redo, undo } from "@codemirror/commands";
import {
  ChangeSet,
  EditorState,
  Transaction,
  type TransactionSpec,
} from "@codemirror/state";
import { describe, expect, it } from "vitest";

import { isKernelTransaction, opsForTransactions } from "$lib/editor/kernelSync";

import {
  buildRemoteDecorations,
  composeRemoteChanges,
  mapRemoteSelection,
  remoteSelectionsField,
  remoteTransaction,
  selectionGeometry,
  setRemoteSelections,
  type RemoteChange,
  type RemoteSelection,
} from "./remote";

/** The reference semantics: apply sequential changes to a plain string. */
function applySequentially(text: string, changes: readonly RemoteChange[]): string {
  let out = text;
  for (const c of changes) {
    out = out.slice(0, c.start) + c.insert + out.slice(c.start + c.delete);
  }
  return out;
}

function applyComposed(text: string, changes: readonly RemoteChange[]): string {
  return composeRemoteChanges(text.length, changes)
    .apply(EditorState.create({ doc: text }).doc)
    .toString();
}

describe("composeRemoteChanges", () => {
  it("equals sequential application for a multi-change batch", () => {
    const text = "hello brave new world";
    const changes: RemoteChange[] = [
      { start: 6, delete: 5, insert: "" }, // hello  new world
      { start: 6, delete: 0, insert: "cruel" }, // hello cruel new world
      { start: 0, delete: 5, insert: "goodbye" },
      { start: 20, delete: 0, insert: "!" }, // after the earlier edits
    ];
    expect(applyComposed(text, changes)).toBe(applySequentially(text, changes));
  });

  it("counts astral characters as two UTF-16 units, like CodeMirror", () => {
    const text = "a😀b";
    // delete the emoji: start 1, two units
    const changes: RemoteChange[] = [{ start: 1, delete: 2, insert: "é" }];
    expect(applyComposed(text, changes)).toBe("aéb");
    expect(applySequentially(text, changes)).toBe("aéb");
  });

  it("handles a later change that depends on an earlier insertion", () => {
    const text = "ab";
    const changes: RemoteChange[] = [
      { start: 1, delete: 0, insert: "XYZ" }, // aXYZb
      { start: 3, delete: 1, insert: "" }, // aXYb  (indexes the inserted text)
    ];
    expect(applyComposed(text, changes)).toBe("aXYb");
  });

  it("refuses a change outside the document (view and core diverged)", () => {
    expect(() =>
      composeRemoteChanges(3, [{ start: 2, delete: 5, insert: "" }]),
    ).toThrow(RangeError);
    expect(() =>
      composeRemoteChanges(3, [{ start: -1, delete: 0, insert: "x" }]),
    ).toThrow(RangeError);
  });

  it("randomised: composed equals sequential", () => {
    let seed = 12345;
    const rnd = (n: number): number => {
      seed = (Math.imul(seed, 1103515245) + 12345) & 0x7fffffff;
      return seed % n;
    };
    const pieces = ["a", "é", "😀", "日本", "\n", "xyz"];
    for (let round = 0; round < 300; round += 1) {
      let text = "start 😀 text é\n";
      const original = text;
      const changes: RemoteChange[] = [];
      for (let i = 0; i < 1 + rnd(6); i += 1) {
        const start = rnd(text.length + 1);
        const del = rnd(Math.min(4, text.length - start) + 1);
        const insert = pieces[rnd(pieces.length)] ?? "";
        // Never split a surrogate pair (the core never emits such a change).
        const isLow = (u: number): boolean => u >= 0xdc00 && u <= 0xdfff;
        if (start < text.length && isLow(text.charCodeAt(start))) continue;
        if (start + del < text.length && del > 0 && isLow(text.charCodeAt(start + del)))
          continue;
        changes.push({ start, delete: del, insert });
        text = text.slice(0, start) + insert + text.slice(start + del);
      }
      expect(applyComposed(original, changes)).toBe(text);
    }
  });
});

describe("remoteTransaction", () => {
  const changes: RemoteChange[] = [{ start: 0, delete: 0, insert: "R:" }];

  it("is one transaction, remote, out of history, and never echoed", () => {
    const state = EditorState.create({ doc: "text", extensions: [history()] });
    const tr = state.update(remoteTransaction(state, changes));
    expect(tr.state.doc.toString()).toBe("R:text");
    expect(tr.annotation(Transaction.remote)).toBe(true);
    expect(tr.annotation(Transaction.addToHistory)).toBe(false);
    expect(isKernelTransaction(tr)).toBe(true);
    // The kernel sync turns user transactions into commands; this yields none.
    expect(opsForTransactions([tr])).toEqual([]);
  });

  it("a user transaction still produces kernel ops (control)", () => {
    const state = EditorState.create({ doc: "text" });
    const tr = state.update({ changes: { from: 0, insert: "U:" } });
    expect(opsForTransactions([tr])).toEqual([[{ kind: "insert", at: 0, text: "U:" }]]);
  });

  it("local undo undoes only local edits and survives remote edits in between", () => {
    let state = EditorState.create({ doc: "base", extensions: [history()] });
    const run = (spec: TransactionSpec): void => {
      state = state.update(spec).state;
    };
    const dispatchTarget = {
      get state(): EditorState {
        return state;
      },
      dispatch(tr: Transaction): void {
        state = tr.state;
      },
    };

    // local edit, then a remote edit in front, then another local edit
    // Isolated so the two local edits are separate undo steps (CodeMirror
    // would otherwise group edits typed within its group delay).
    run({
      changes: { from: 4, insert: " mine1" },
      userEvent: "input.type",
      annotations: isolateHistory.of("full"),
    });
    run(remoteTransaction(state, [{ start: 0, delete: 0, insert: "THEIRS " }]));
    expect(state.doc.toString()).toBe("THEIRS base mine1");
    run({
      changes: { from: state.doc.length, insert: " mine2" },
      userEvent: "input.type",
      annotations: isolateHistory.of("full"),
    });
    expect(state.doc.toString()).toBe("THEIRS base mine1 mine2");

    // Undo once: only the newest local edit goes.
    undo(dispatchTarget);
    expect(state.doc.toString()).toBe("THEIRS base mine1");
    // Undo again: the older local edit goes, the remote text stays.
    undo(dispatchTarget);
    expect(state.doc.toString()).toBe("THEIRS base");
    // Nothing left to undo: the remote edit is not on the stack.
    expect(undo(dispatchTarget)).toBe(false);
    expect(state.doc.toString()).toBe("THEIRS base");

    // Redo re-applies the local edit at the mapped position.
    redo(dispatchTarget);
    expect(state.doc.toString()).toBe("THEIRS base mine1");
  });

  it("keeps the local selection on the text it was on", () => {
    const state = EditorState.create({
      doc: "hello world",
      selection: { anchor: 6, head: 11 }, // "world"
    });
    const next = state.update(
      remoteTransaction(state, [{ start: 0, delete: 0, insert: ">>> " }]),
    ).state;
    expect(next.selection.main.from).toBe(10);
    expect(next.selection.main.to).toBe(15);
    expect(next.sliceDoc(next.selection.main.from, next.selection.main.to)).toBe(
      "world",
    );
  });
});

describe("remote selections", () => {
  const sel = (anchor: number, head: number): RemoteSelection => ({
    peer: "42",
    name: "Ada",
    slot: 1,
    anchor,
    head,
  });

  it("maps a caret through local edits; insertion at the caret leaves it put", () => {
    const state = EditorState.create({
      doc: "0123456789",
      extensions: [remoteSelectionsField],
    });
    let s = state.update({ effects: setRemoteSelections.of([sel(5, 5)]) }).state;
    expect(s.field(remoteSelectionsField)[0]?.head).toBe(5);
    // insert before: shifts
    s = s.update({ changes: { from: 0, insert: "abc" } }).state;
    expect(s.field(remoteSelectionsField)[0]?.head).toBe(8);
    // insert exactly at the caret: stays on the left of the new text
    s = s.update({ changes: { from: 8, insert: "XX" } }).state;
    expect(s.field(remoteSelectionsField)[0]?.head).toBe(8);
    // delete a range containing the caret: collapses to the range start
    s = s.update({ changes: { from: 6, to: 10 } }).state;
    expect(s.field(remoteSelectionsField)[0]?.head).toBe(6);
  });

  it("maps a selection range through a remote batch", () => {
    const base = sel(2, 6);
    const changes = composeRemoteChanges(10, [
      { start: 0, delete: 0, insert: "ab" },
      { start: 5, delete: 2, insert: "" },
    ]);
    const mapped = mapRemoteSelection(base, changes);
    // 2 -> 4 after the first insert; 6 -> 8, then the deletion 5..7 pulls it to 6
    expect([mapped.anchor, mapped.head]).toEqual([4, 6]);
  });

  it("an authoritative update replaces everything", () => {
    let s = EditorState.create({ doc: "abc", extensions: [remoteSelectionsField] });
    s = s.update({ effects: setRemoteSelections.of([sel(1, 1)]) }).state;
    s = s.update({ effects: setRemoteSelections.of([]) }).state;
    expect(s.field(remoteSelectionsField)).toEqual([]);
  });

  it("clamps geometry and orders a backwards selection", () => {
    expect(selectionGeometry(sel(8, 2), 5)).toEqual({ caret: 2, from: 2, to: 5 });
    expect(selectionGeometry(sel(-3, 99), 10)).toEqual({ caret: 10, from: 0, to: 10 });
  });

  it("builds a caret widget and a mark per ranged selection", () => {
    const set = buildRemoteDecorations([sel(1, 4), { ...sel(6, 6), peer: "7" }], 10);
    let count = 0;
    set.between(0, 10, () => {
      count += 1;
    });
    expect(count).toBe(3); // mark + widget for the first, widget for the second
  });

  it("ChangeSet identity: empty batch composes to an empty set", () => {
    expect(composeRemoteChanges(5, []).empty).toBe(true);
    expect(ChangeSet.empty(5).length).toBe(5);
  });
});
