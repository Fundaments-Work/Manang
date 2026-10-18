import { describe, expect, it } from "vitest";

import {
  CLOSED,
  navigate,
  nextEnabled,
  type NavEvent,
  type NavKey,
  type NavMenus,
  type NavState,
} from "./menuNav";

// File: 3 items, the second disabled. Edit: all disabled. View: 2 items.
const ENABLED: readonly (readonly boolean[])[] = [
  [true, false, true],
  [false, false],
  [true, true],
];
const menus: NavMenus = { count: 3, enabled: (i) => ENABLED[i] ?? [] };

function run(events: NavEvent[], from: NavState = CLOSED): NavState {
  return events.reduce((state, event) => navigate(state, event, menus).state, from);
}

const key = (k: NavKey): NavEvent => ({ type: "key", key: k });

describe("menu navigation", () => {
  it("Alt shows mnemonics while held", () => {
    expect(run([{ type: "alt", down: true }]).mnemonics).toBe(true);
    expect(
      run([
        { type: "alt", down: true },
        { type: "alt", down: false },
      ]).mnemonics,
    ).toBe(false);
  });

  it("Alt+letter opens that menu on its first enabled item", () => {
    const s = run([{ type: "mnemonic", index: 0 }]);
    expect(s).toMatchObject({ mode: "open", active: 0, item: 0, mnemonics: true });
    // A menu with nothing enabled opens with no item highlighted.
    expect(run([{ type: "mnemonic", index: 1 }]).item).toBe(-1);
  });

  it("F10 focuses the bar; arrows move; Down opens", () => {
    let s = run([{ type: "f10" }]);
    expect(s).toMatchObject({ mode: "bar", active: 0, item: -1 });
    s = run([key("ArrowRight"), key("ArrowRight")], s);
    expect(s.active).toBe(2);
    s = run([key("ArrowRight")], s);
    expect(s.active).toBe(0);
    s = run([key("ArrowLeft"), key("ArrowDown")], s);
    expect(s).toMatchObject({ mode: "open", active: 2, item: 0 });
  });

  it("Up and Down skip disabled items and wrap", () => {
    let s = run([{ type: "mnemonic", index: 0 }]);
    s = run([key("ArrowDown")], s);
    expect(s.item).toBe(2);
    s = run([key("ArrowDown")], s);
    expect(s.item).toBe(0);
    s = run([key("ArrowUp")], s);
    expect(s.item).toBe(2);
    expect(run([key("Home")], s).item).toBe(0);
    expect(run([key("End")], s).item).toBe(2);
  });

  it("Left and Right switch menus and keep the menu open", () => {
    const s = run([
      { type: "mnemonic", index: 0 },
      key("ArrowRight"),
      key("ArrowRight"),
    ]);
    expect(s).toMatchObject({ mode: "open", active: 2, item: 0 });
  });

  it("Enter runs the highlighted item and closes; a disabled item never runs", () => {
    const open = run([{ type: "mnemonic", index: 2 }, key("ArrowDown")]);
    const done = navigate(open, key("Enter"), menus);
    expect(done.effect).toEqual({ type: "run", menu: 2, item: 1 });
    expect(done.state.mode).toBe("closed");
    const file = run([{ type: "click-bar", index: 0 }]);
    expect(navigate(file, { type: "click-item", index: 1 }, menus).effect).toBeNull();
    expect(navigate(file, { type: "click-item", index: 2 }, menus).effect).toEqual({
      type: "run",
      menu: 0,
      item: 2,
    });
  });

  it("Escape closes and gives focus back", () => {
    const open = run([{ type: "mnemonic", index: 0 }]);
    const done = navigate(open, key("Escape"), menus);
    expect(done.state.mode).toBe("closed");
    expect(done.effect).toEqual({ type: "restore-focus" });
    expect(navigate(run([{ type: "f10" }]), key("Escape"), menus).effect).toEqual({
      type: "restore-focus",
    });
  });

  it("hover switches menus only once one is open; a click outside closes", () => {
    expect(run([{ type: "hover-bar", index: 2 }]).mode).toBe("closed");
    let s = run([{ type: "click-bar", index: 0 }]);
    expect(s).toMatchObject({ mode: "open", active: 0, item: -1 });
    s = run([{ type: "hover-bar", index: 2 }], s);
    expect(s).toMatchObject({ mode: "open", active: 2 });
    s = run([{ type: "hover-item", index: 1 }], s);
    expect(s.item).toBe(1);
    expect(run([{ type: "click-bar", index: 2 }], s).mode).toBe("closed");
    expect(run([{ type: "outside" }], s).mode).toBe("closed");
  });

  it("hovering a disabled item highlights nothing", () => {
    const s = run([
      { type: "click-bar", index: 0 },
      { type: "hover-item", index: 1 },
    ]);
    expect(s.item).toBe(-1);
  });

  it("folded: the Menu button lists menus, Right opens one, Left goes back", () => {
    let s = run([{ type: "fold", folded: true }, { type: "click-fold" }]);
    expect(s).toMatchObject({ mode: "open", folded: true, submenu: false, active: 0 });
    s = run([key("ArrowDown"), key("ArrowDown")], s);
    expect(s.active).toBe(2);
    s = run([key("ArrowRight")], s);
    expect(s).toMatchObject({ submenu: true, item: 0 });
    s = run([key("ArrowDown")], s);
    expect(s.item).toBe(1);
    const ran = navigate(s, key("Enter"), menus);
    expect(ran.effect).toEqual({ type: "run", menu: 2, item: 1 });
    expect(ran.state).toMatchObject({ mode: "closed", folded: true });
    s = run([key("ArrowLeft")], s);
    expect(s).toMatchObject({ submenu: false, item: -1, mode: "open" });
    expect(navigate(s, key("Escape"), menus).state.mode).toBe("closed");
  });

  it("folded: Alt+letter opens the list at that menu; hover opens beside it", () => {
    const s = run([
      { type: "fold", folded: true },
      { type: "mnemonic", index: 2 },
    ]);
    expect(s).toMatchObject({ mode: "open", active: 2, submenu: true, item: 0 });
    const hovered = run([{ type: "hover-bar", index: 0 }], s);
    expect(hovered).toMatchObject({ active: 0, submenu: true, item: -1 });
  });

  it("keys do nothing while closed; folding resets", () => {
    expect(run([key("ArrowDown")])).toEqual(CLOSED);
    const open = run([{ type: "mnemonic", index: 0 }]);
    expect(run([{ type: "fold", folded: true }], open)).toMatchObject({
      mode: "closed",
      folded: true,
    });
  });

  it("nextEnabled finds the next enabled index or -1", () => {
    expect(nextEnabled([false, true, false], -1, 1)).toBe(1);
    expect(nextEnabled([false, false], 0, 1)).toBe(-1);
    expect(nextEnabled([true, false, true], 0, -1)).toBe(2);
  });
});
