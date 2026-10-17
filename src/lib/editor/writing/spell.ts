/**
 * Spell check: squiggles under misspelled words, and a quick-fix menu.
 *
 * Checking runs on the visible paragraphs after the author stops typing (see
 * `checker.ts`) and asks for no suggestions, which is the expensive part.
 * Suggestions are fetched only when the menu opens, for that one word.
 */

import {
  Prec,
  StateEffect,
  StateField,
  type EditorState,
  type Extension,
} from "@codemirror/state";
import {
  Decoration,
  EditorView,
  keymap,
  showTooltip,
  type DecorationSet,
  type Tooltip,
} from "@codemirror/view";

import type { Misspelling, WritingApi } from "$lib/kernel/api";

import { checker, newInvalidation } from "./checker";
import type { RelItem } from "./chunks";

export const spellInvalidate = newInvalidation();

interface Span {
  readonly from: number;
  readonly to: number;
}

export const setSpell = StateEffect.define<readonly Span[]>();

const spellMark = Decoration.mark({ class: "cm-spell" });

export const spellField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setSpell)) {
        return Decoration.set(
          effect.value.map((s) => spellMark.range(s.from, s.to)),
          true,
        );
      }
    }
    return tr.docChanged ? value.map(tr.changes) : value;
  },
  provide: (field) => EditorView.decorations.from(field),
});

/** The misspelling under `pos` (ends are inclusive, so a caret after the word counts). */
export function spellSpanAt(state: EditorState, pos: number): Span | null {
  let hit: Span | null = null;
  state.field(spellField, false)?.between(pos, pos, (from, to) => {
    if (hit === null && from <= pos && pos <= to) hit = { from, to };
  });
  return hit;
}

/* -------------------------------------------------------------------------- */
/* The quick-fix menu                                                          */
/* -------------------------------------------------------------------------- */

export interface MenuState {
  readonly from: number;
  readonly to: number;
  readonly word: string;
  /** `null` while suggestions are loading. */
  readonly suggestions: readonly string[] | null;
  readonly index: number;
}

const setMenu = StateEffect.define<MenuState | null>();
const moveMenu = StateEffect.define<number>();
const fillMenu = StateEffect.define<{ word: string; suggestions: readonly string[] }>();

const menuStateField = StateField.define<MenuState | null>({
  create: () => null,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setMenu)) return effect.value;
    }
    if (value === null) return null;
    // Typing or moving the caret dismisses it.
    if (tr.docChanged || tr.selection !== undefined) return null;
    for (const effect of tr.effects) {
      if (effect.is(fillMenu) && effect.value.word === value.word) {
        return { ...value, suggestions: effect.value.suggestions, index: 0 };
      }
      if (effect.is(moveMenu)) {
        const count = (value.suggestions?.length ?? 0) + 1;
        return { ...value, index: (value.index + effect.value + count) % count };
      }
    }
    return value;
  },
});

function menuTooltip(api: WritingApi): Extension {
  return showTooltip.compute([menuStateField], (state): Tooltip | null => {
    const menu = state.field(menuStateField);
    if (menu === null) return null;
    return {
      pos: menu.from,
      above: false,
      arrow: false,
      create: (view) => ({ dom: renderMenu(view, menu, api) }),
    };
  });
}

/** Entries of the menu: the suggestions, then "Add to dictionary". */
export function menuEntries(menu: MenuState): { label: string; word: string | null }[] {
  const entries: { label: string; word: string | null }[] = (
    menu.suggestions ?? []
  ).map((s) => ({ label: s, word: s }));
  entries.push({ label: "Add to dictionary", word: null });
  return entries;
}

function renderMenu(view: EditorView, menu: MenuState, api: WritingApi): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "cm-spell-menu";
  dom.setAttribute("role", "listbox");
  dom.setAttribute("aria-label", `Spelling: ${menu.word}`);
  if (menu.suggestions === null) {
    const loading = document.createElement("div");
    loading.className = "cm-spell-note";
    loading.textContent = "Looking for suggestions";
    dom.append(loading);
  } else if (menu.suggestions.length === 0) {
    const none = document.createElement("div");
    none.className = "cm-spell-note";
    none.textContent = "No suggestions";
    dom.append(none);
  }
  menuEntries(menu).forEach((entry, i) => {
    const row = document.createElement("div");
    row.className = "cm-spell-item";
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", String(i === menu.index));
    if (entry.word === null) row.dataset["action"] = "add-word";
    else row.dataset["fix"] = entry.word;
    row.textContent = entry.label;
    // mousedown, so the editor keeps focus and the caret does not move first.
    row.addEventListener("mousedown", (event) => {
      event.preventDefault();
      choose(view, menu, entry.word, api);
    });
    dom.append(row);
  });
  return dom;
}

function choose(
  view: EditorView,
  menu: MenuState,
  fix: string | null,
  api: WritingApi,
): void {
  if (fix !== null) {
    view.dispatch({
      changes: { from: menu.from, to: menu.to, insert: fix },
      selection: { anchor: menu.from + fix.length },
      effects: setMenu.of(null),
      userEvent: "input.complete",
    });
    view.focus();
    return;
  }
  view.dispatch({ effects: setMenu.of(null) });
  void api.addWord(menu.word).then(
    () => view.dispatch({ effects: spellInvalidate.of(null) }),
    () => undefined,
  );
  view.focus();
}

function openMenu(view: EditorView, api: WritingApi, at: number): boolean {
  const span = spellSpanAt(view.state, at);
  if (span === null) return false;
  const word = view.state.sliceDoc(span.from, span.to);
  view.dispatch({
    effects: setMenu.of({ ...span, word, suggestions: null, index: 0 }),
    scrollIntoView: true,
  });
  void api.suggest(word, 5).then(
    (result) => {
      if (view.state.field(menuStateField, false)?.word !== word) return;
      view.dispatch({
        effects: fillMenu.of({ word, suggestions: result.suggestions.slice(0, 5) }),
      });
    },
    () => view.dispatch({ effects: fillMenu.of({ word, suggestions: [] }) }),
  );
  return true;
}

function whenOpen(run: (view: EditorView, menu: MenuState) => void) {
  return (view: EditorView): boolean => {
    const menu = view.state.field(menuStateField, false);
    if (menu === null || menu === undefined) return false;
    run(view, menu);
    return true;
  };
}

export function spell(api: WritingApi): Extension {
  return [
    spellField,
    menuStateField,
    menuTooltip(api),
    checker<Misspelling, RelItem>(
      {
        request: async (text, syntax) =>
          (await api.spellCheck(text, syntax)).misspelled,
        range: (m) => m.range,
        make: (_m, start, end) => ({ start, end }),
        apply: (view, chunks, found) => {
          const spans: Span[] = [];
          chunks.forEach((chunk, i) => {
            for (const item of found[i] ?? []) {
              spans.push({ from: chunk.from + item.start, to: chunk.from + item.end });
            }
          });
          view.dispatch({ effects: setSpell.of(spans) });
        },
      },
      spellInvalidate,
    ),
    Prec.highest(
      keymap.of([
        {
          key: "ArrowDown",
          run: whenOpen((view) => view.dispatch({ effects: moveMenu.of(1) })),
        },
        {
          key: "ArrowUp",
          run: whenOpen((view) => view.dispatch({ effects: moveMenu.of(-1) })),
        },
        {
          key: "Enter",
          run: whenOpen((view, menu) => {
            const entry = menuEntries(menu)[menu.index];
            if (entry !== undefined) choose(view, menu, entry.word, api);
          }),
        },
        {
          key: "Escape",
          run: whenOpen((view) => view.dispatch({ effects: setMenu.of(null) })),
        },
      ]),
    ),
    keymap.of([
      {
        key: "Mod-.",
        run: (view) => openMenu(view, api, view.state.selection.main.head),
      },
    ]),
    EditorView.domEventHandlers({
      click(event, view) {
        if (event.button !== 0 || !view.state.selection.main.empty) return false;
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
        if (pos === null) return false;
        // Hit-test the glyphs: clicking blank space after a line must not open it.
        const span = spellSpanAt(view.state, pos);
        const target = event.target;
        if (span === null || !(target instanceof HTMLElement)) return false;
        if (target.closest(".cm-spell") === null) return false;
        return openMenu(view, api, pos);
      },
    }),
  ];
}

export const spellTheme = EditorView.baseTheme({
  ".cm-spell": {
    textDecoration: "underline wavy var(--mng-text-60)",
    textDecorationThickness: "1px",
    textUnderlineOffset: "3px",
  },
  ".cm-spell-menu": {
    minWidth: "160px",
    padding: "2px 0",
    fontFamily: "var(--font-sans)",
    fontSize: "12px",
  },
  ".cm-spell-item": {
    padding: "3px 12px",
    cursor: "pointer",
    color: "var(--mng-text)",
    whiteSpace: "nowrap",
  },
  ".cm-spell-item[aria-selected=true]": {
    backgroundColor: "color-mix(in srgb, var(--mng-accent) 28%, transparent)",
  },
  ".cm-spell-item[data-action]": {
    borderTop: "1px solid var(--mng-line)",
    color: "var(--mng-text-60)",
  },
  ".cm-spell-note": { padding: "3px 12px", color: "var(--mng-text-60)" },
});
