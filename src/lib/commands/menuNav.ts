/**
 * Keyboard and pointer navigation of the menu bar, as a pure state machine.
 *
 * Desktop conventions (Windows, GNOME, VS Code): Alt shows the mnemonics,
 * Alt+letter opens that menu, F10 focuses the bar, Left/Right move between
 * menus (an open menu stays open), Up/Down move between items skipping
 * disabled ones, Enter or Space runs, Escape closes and gives focus back,
 * hovering another menu while one is open switches to it, a click outside
 * closes.
 *
 * Folded (a narrow window): one "Menu" button opens a list of the six menus;
 * Right, Enter or hovering opens one beside it, Left or Escape goes back.
 */

export type NavMode = "closed" | "bar" | "open";

export interface NavState {
  readonly mode: NavMode;
  /** The highlighted menu (bar, open, or the folded list's row). */
  readonly active: number;
  /** The highlighted item in the open menu, -1 for none. */
  readonly item: number;
  readonly folded: boolean;
  /** Folded only: the active menu's items are shown beside the list. */
  readonly submenu: boolean;
  /** Underline the mnemonic letters (Alt held, or opened from the keyboard). */
  readonly mnemonics: boolean;
}

export const CLOSED: NavState = {
  mode: "closed",
  active: 0,
  item: -1,
  folded: false,
  submenu: false,
  mnemonics: false,
};

export type NavKey =
  | "ArrowLeft"
  | "ArrowRight"
  | "ArrowUp"
  | "ArrowDown"
  | "Home"
  | "End"
  | "Enter"
  | " "
  | "Escape"
  | "Tab";

export type NavEvent =
  | { readonly type: "alt"; readonly down: boolean }
  | { readonly type: "mnemonic"; readonly index: number }
  | { readonly type: "f10" }
  | { readonly type: "key"; readonly key: NavKey }
  | { readonly type: "click-bar"; readonly index: number }
  | { readonly type: "hover-bar"; readonly index: number }
  | { readonly type: "hover-item"; readonly index: number }
  | { readonly type: "click-item"; readonly index: number }
  /** The folded "Menu" button. */
  | { readonly type: "click-fold" }
  | { readonly type: "outside" }
  | { readonly type: "fold"; readonly folded: boolean };

export type NavEffect =
  /** Close, give focus back, then run this item of this menu. */
  | { readonly type: "run"; readonly menu: number; readonly item: number }
  /** Closed from the keyboard: give focus back to where it was. */
  | { readonly type: "restore-focus" }
  | null;

export interface NavResult {
  readonly state: NavState;
  readonly effect: NavEffect;
}

/** What the machine needs to know about the menus: how many, and which items are enabled. */
export interface NavMenus {
  readonly count: number;
  enabled(menu: number): readonly boolean[];
}

const wrap = (i: number, n: number): number => (n === 0 ? 0 : ((i % n) + n) % n);

/** The next enabled item from `from` in `step` direction, or -1 when none. */
export function nextEnabled(
  enabled: readonly boolean[],
  from: number,
  step: 1 | -1,
): number {
  const n = enabled.length;
  for (let k = 1; k <= n; k++) {
    const i = wrap(from + step * k, n);
    if (enabled[i] === true) return i;
  }
  return -1;
}

const first = (enabled: readonly boolean[]): number => nextEnabled(enabled, -1, 1);
const last = (enabled: readonly boolean[]): number =>
  nextEnabled(enabled, enabled.length, -1);

const result = (state: NavState, effect: NavEffect = null): NavResult => ({
  state,
  effect,
});

function close(state: NavState): NavState {
  return { ...CLOSED, folded: state.folded };
}

function runIfEnabled(state: NavState, menus: NavMenus, item: number): NavResult {
  if (menus.enabled(state.active)[item] !== true) return result(state);
  return result(close(state), { type: "run", menu: state.active, item });
}

/** Folded: the list of menus is showing; `submenu` says whether items are too. */
function foldedKey(state: NavState, key: NavKey, menus: NavMenus): NavResult {
  const enabled = menus.enabled(state.active);
  if (!state.submenu) {
    switch (key) {
      case "ArrowDown":
        return result({ ...state, active: wrap(state.active + 1, menus.count) });
      case "ArrowUp":
        return result({ ...state, active: wrap(state.active - 1, menus.count) });
      case "Home":
        return result({ ...state, active: 0 });
      case "End":
        return result({ ...state, active: menus.count - 1 });
      case "ArrowRight":
      case "Enter":
      case " ":
        return result({ ...state, submenu: true, item: first(enabled) });
      case "Escape":
      case "Tab":
        return result(close(state), { type: "restore-focus" });
      case "ArrowLeft":
        return result(state);
    }
  }
  switch (key) {
    case "ArrowDown":
      return result({ ...state, item: nextEnabled(enabled, state.item, 1) });
    case "ArrowUp":
      return result({
        ...state,
        item: nextEnabled(enabled, state.item < 0 ? enabled.length : state.item, -1),
      });
    case "Home":
      return result({ ...state, item: first(enabled) });
    case "End":
      return result({ ...state, item: last(enabled) });
    case "Enter":
    case " ":
      return state.item < 0 ? result(state) : runIfEnabled(state, menus, state.item);
    case "ArrowLeft":
    case "Escape":
      return result({ ...state, submenu: false, item: -1 });
    case "Tab":
      return result(close(state), { type: "restore-focus" });
    case "ArrowRight":
      return result(state);
  }
}

function barKey(state: NavState, key: NavKey, menus: NavMenus): NavResult {
  const open = state.mode === "open";
  switch (key) {
    case "ArrowLeft":
    case "ArrowRight": {
      const active = wrap(state.active + (key === "ArrowRight" ? 1 : -1), menus.count);
      return result({
        ...state,
        active,
        item: open ? first(menus.enabled(active)) : -1,
      });
    }
    case "Home":
    case "End":
      if (!open)
        return result({ ...state, active: key === "Home" ? 0 : menus.count - 1 });
      return result({
        ...state,
        item: (key === "Home" ? first : last)(menus.enabled(state.active)),
      });
    case "ArrowDown":
    case "ArrowUp": {
      const enabled = menus.enabled(state.active);
      if (!open)
        return result({
          ...state,
          mode: "open",
          item: key === "ArrowDown" ? first(enabled) : last(enabled),
        });
      const from = state.item < 0 && key === "ArrowUp" ? enabled.length : state.item;
      return result({
        ...state,
        item: nextEnabled(enabled, from, key === "ArrowDown" ? 1 : -1),
      });
    }
    case "Enter":
    case " ":
      if (!open)
        return result({
          ...state,
          mode: "open",
          item: first(menus.enabled(state.active)),
        });
      return state.item < 0 ? result(state) : runIfEnabled(state, menus, state.item);
    case "Escape":
    case "Tab":
      return result(close(state), { type: "restore-focus" });
  }
}

export function navigate(state: NavState, event: NavEvent, menus: NavMenus): NavResult {
  switch (event.type) {
    case "fold":
      return result(
        event.folded === state.folded ? state : { ...CLOSED, folded: event.folded },
      );
    case "alt":
      // Alt shows the letters while held; an open menu keeps them.
      return result({ ...state, mnemonics: event.down || state.mode !== "closed" });
    case "mnemonic": {
      if (event.index < 0 || event.index >= menus.count) return result(state);
      return result({
        ...state,
        mode: "open",
        active: event.index,
        item: first(menus.enabled(event.index)),
        submenu: state.folded,
        mnemonics: true,
      });
    }
    case "f10":
      if (state.mode !== "closed")
        return result(close(state), { type: "restore-focus" });
      return state.folded
        ? result({
            ...state,
            mode: "open",
            active: 0,
            item: -1,
            submenu: false,
            mnemonics: true,
          })
        : result({ ...state, mode: "bar", active: 0, item: -1, mnemonics: true });
    case "key":
      if (state.mode === "closed") return result(state);
      return state.folded
        ? foldedKey(state, event.key, menus)
        : barKey(state, event.key, menus);
    case "click-fold":
      if (state.mode !== "closed") return result(close(state));
      return result({ ...state, mode: "open", active: 0, item: -1, submenu: false });
    case "click-bar":
      if (state.folded)
        return result({ ...state, active: event.index, submenu: true, item: -1 });
      if (state.mode === "open" && state.active === event.index)
        return result(close(state));
      return result({ ...state, mode: "open", active: event.index, item: -1 });
    case "hover-bar":
      if (state.folded)
        return state.mode === "open"
          ? result({ ...state, active: event.index, submenu: true, item: -1 })
          : result(state);
      // Hover switches menus only once one is open.
      if (state.mode !== "open" || state.active === event.index) return result(state);
      return result({ ...state, active: event.index, item: -1 });
    case "hover-item":
      if (state.mode !== "open") return result(state);
      return result({
        ...state,
        item: menus.enabled(state.active)[event.index] === true ? event.index : -1,
      });
    case "click-item":
      if (state.mode !== "open") return result(state);
      return runIfEnabled(state, menus, event.index);
    case "outside":
      return state.mode === "closed" ? result(state) : result(close(state));
  }
}
