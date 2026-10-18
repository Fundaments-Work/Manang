/**
 * Quick fix for compile diagnostics: Mod-. applies the fix of the diagnostic at
 * the caret, or opens a small list when it has several. The hover callout
 * (`gutter.ts`) carries the same fixes as buttons.
 */

import { Prec, StateEffect, StateField, type Extension } from "@codemirror/state";
import { keymap, showTooltip, type EditorView, type Tooltip } from "@codemirror/view";

import type { Diagnostic, FixChoice } from "./diagnostics";
import { readDiagnostics } from "./gutter";
import { runFix } from "./healBridge";

interface FixMenu {
  readonly pos: number;
  readonly problemId: string;
  readonly fixes: readonly FixChoice[];
  readonly index: number;
}

const setMenu = StateEffect.define<FixMenu | null>();
const moveMenu = StateEffect.define<number>();

const menuField = StateField.define<FixMenu | null>({
  create: () => null,
  update(value, tr) {
    for (const effect of tr.effects) {
      if (effect.is(setMenu)) return effect.value;
    }
    if (value === null) return null;
    // Typing or moving the caret dismisses it.
    if (tr.docChanged || tr.selection !== undefined) return null;
    for (const effect of tr.effects) {
      if (effect.is(moveMenu)) {
        const count = value.fixes.length;
        return { ...value, index: (value.index + effect.value + count) % count };
      }
    }
    return value;
  },
});

function renderMenu(view: EditorView, menu: FixMenu): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "cm-spell-menu cm-fix-menu";
  dom.setAttribute("role", "listbox");
  dom.setAttribute("aria-label", "Fixes");
  menu.fixes.forEach((fix, i) => {
    const row = document.createElement("div");
    row.className = "cm-spell-item";
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", String(i === menu.index));
    row.dataset["fix"] = fix.id;
    row.title = fix.why;
    row.textContent = fix.label;
    // mousedown, so the editor keeps focus and the caret does not move first.
    row.addEventListener("mousedown", (event) => {
      event.preventDefault();
      choose(view, menu, fix);
    });
    dom.append(row);
  });
  return dom;
}

function choose(view: EditorView, menu: FixMenu, fix: FixChoice): void {
  view.dispatch({ effects: setMenu.of(null) });
  runFix(menu.problemId, fix.id);
  view.focus();
}

/** The diagnostic with fixes at `pos`, if any. */
function fixableAt(view: EditorView, pos: number): Diagnostic | undefined {
  return readDiagnostics(view).find(
    (d) =>
      d.problemId !== undefined &&
      d.fixes !== undefined &&
      d.fixes.length > 0 &&
      pos >= d.from &&
      pos <= d.to,
  );
}

/** Apply or offer the fixes of the diagnostic at the caret (or on the caret's line). */
export function openFixes(view: EditorView): boolean {
  const caret = view.state.selection.main.head;
  const line = view.state.doc.lineAt(caret);
  const diagnostic =
    fixableAt(view, caret) ??
    readDiagnostics(view).find(
      (d) =>
        d.problemId !== undefined &&
        (d.fixes?.length ?? 0) > 0 &&
        d.from <= line.to &&
        d.to >= line.from,
    );
  const fixes = diagnostic?.fixes;
  if (diagnostic?.problemId === undefined || fixes === undefined) return false;
  const only = fixes[0];
  if (fixes.length === 1 && only !== undefined) {
    runFix(diagnostic.problemId, only.id);
    return true;
  }
  view.dispatch({
    effects: setMenu.of({
      pos: diagnostic.from,
      problemId: diagnostic.problemId,
      fixes,
      index: 0,
    }),
    scrollIntoView: true,
  });
  return true;
}

function whenOpen(run: (view: EditorView, menu: FixMenu) => void) {
  return (view: EditorView): boolean => {
    const menu = view.state.field(menuField, false);
    if (menu === null || menu === undefined) return false;
    run(view, menu);
    return true;
  };
}

function menuTooltip(): Extension {
  return showTooltip.compute([menuField], (state): Tooltip | null => {
    const menu = state.field(menuField);
    if (menu === null) return null;
    return {
      pos: menu.pos,
      above: false,
      arrow: false,
      create: (view) => ({ dom: renderMenu(view, menu) }),
    };
  });
}

/** The quick-fix extension: Mod-. and its list. */
export function healFixes(): Extension {
  return [
    menuField,
    menuTooltip(),
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
            const fix = menu.fixes[menu.index];
            if (fix !== undefined) choose(view, menu, fix);
          }),
        },
        {
          key: "Escape",
          run: whenOpen((view) => view.dispatch({ effects: setMenu.of(null) })),
        },
      ]),
    ),
    // After the spelling menu's own Mod-. (which declines away from a misspelling).
    keymap.of([{ key: "Mod-.", run: openFixes }]),
  ];
}
