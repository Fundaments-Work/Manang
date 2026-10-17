import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";

import { menuEntries, setSpell, spellField, spellSpanAt } from "./spell";

const withSpans = (doc: string, spans: { from: number; to: number }[]) =>
  EditorState.create({ doc, extensions: [spellField] }).update({
    effects: setSpell.of(spans),
  }).state;

describe("spell decorations", () => {
  it("finds the misspelling under a position, ends inclusive", () => {
    const state = withSpans("say teh thing", [{ from: 4, to: 7 }]);
    expect(spellSpanAt(state, 4)).toEqual({ from: 4, to: 7 });
    expect(spellSpanAt(state, 7)).toEqual({ from: 4, to: 7 });
    expect(spellSpanAt(state, 3)).toBeNull();
    expect(spellSpanAt(state, 8)).toBeNull();
  });

  it("follows edits made before the word, so squiggles never drift", () => {
    const state = withSpans("say teh thing", [{ from: 4, to: 7 }]);
    const next = state.update({ changes: { from: 0, insert: "please " } }).state;
    expect(next.sliceDoc(11, 14)).toBe("teh");
    expect(spellSpanAt(next, 12)).toEqual({ from: 11, to: 14 });
  });

  it("drops a squiggle whose word was deleted", () => {
    const state = withSpans("say teh thing", [{ from: 4, to: 7 }]);
    const next = state.update({ changes: { from: 3, to: 8, insert: " " } }).state;
    expect(spellSpanAt(next, 4)).toBeNull();
  });

  it("replaces the whole set when a new result arrives", () => {
    const state = withSpans("teh wich", [{ from: 0, to: 3 }]);
    const next = state.update({ effects: setSpell.of([{ from: 4, to: 8 }]) }).state;
    expect(spellSpanAt(next, 1)).toBeNull();
    expect(spellSpanAt(next, 5)).toEqual({ from: 4, to: 8 });
  });
});

describe("quick-fix menu entries", () => {
  const menu = { from: 0, to: 3, word: "teh", index: 0 };

  it("lists the suggestions, then Add to dictionary", () => {
    expect(menuEntries({ ...menu, suggestions: ["the", "tech"] })).toEqual([
      { label: "the", word: "the" },
      { label: "tech", word: "tech" },
      { label: "Add to dictionary", word: null },
    ]);
  });

  it("offers only Add to dictionary while loading or when there are none", () => {
    expect(menuEntries({ ...menu, suggestions: null })).toHaveLength(1);
    expect(menuEntries({ ...menu, suggestions: [] })).toHaveLength(1);
  });
});
