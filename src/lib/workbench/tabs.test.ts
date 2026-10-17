import { describe, expect, it } from "vitest";

import {
  EMPTY_TABS,
  activeTab,
  closeOthers,
  closeTab,
  closeToRight,
  cycleTab,
  fromSession,
  moveTab,
  openTab,
  removePath,
  renamePath,
  sanitizeSession,
  setView,
  tabByShortcut,
  tabLabels,
  toSession,
  type TabsModel,
} from "./tabs";

const open = (...paths: string[]): TabsModel =>
  paths.reduce((m, p) => openTab(m, p), EMPTY_TABS);
const ids = (m: TabsModel): string[] => m.tabs.map((t) => t.id);

describe("opening", () => {
  it("opens a tab and focuses it; reopening focuses instead of duplicating", () => {
    const m = openTab(open("a.typ", "b.typ"), "a.typ");
    expect(ids(m)).toEqual(["a.typ", "b.typ"]);
    expect(m.active).toBe("a.typ");
  });

  it("places new tabs to the right of the active one", () => {
    const m = openTab(openTab(open("a.typ", "b.typ", "c.typ"), "a.typ"), "d.typ");
    expect(ids(m)).toEqual(["a.typ", "d.typ", "b.typ", "c.typ"]);
  });

  it("classifies figures and slides", () => {
    const m = openTab(open("main.typ"), "figures/x.figure.json");
    expect(m.tabs[1]?.kind).toBe("figure");
    const slides = setView(m, "main.typ", "slides");
    expect(slides.tabs[0]?.view).toBe("slides");
    // A figure has no slide view.
    expect(setView(m, "figures/x.figure.json", "slides").tabs[1]?.view).toBe("text");
  });
});

describe("closing", () => {
  it("returns to the most recently used tab, not just the neighbour", () => {
    let m = open("a.typ", "b.typ", "c.typ");
    m = openTab(m, "a.typ");
    m = openTab(m, "c.typ");
    expect(closeTab(m, "c.typ").active).toBe("a.typ");
  });

  it("closing an inactive tab keeps the active one", () => {
    const m = open("a.typ", "b.typ", "c.typ");
    expect(closeTab(m, "a.typ").active).toBe("c.typ");
  });

  it("empties cleanly", () => {
    const m = closeTab(open("a.typ"), "a.typ");
    expect(m.tabs).toEqual([]);
    expect(m.active).toBeNull();
    expect(activeTab(m)).toBeUndefined();
  });

  it("closes others and tabs to the right", () => {
    const m = open("a.typ", "b.typ", "c.typ", "d.typ");
    expect(ids(closeOthers(m, "b.typ"))).toEqual(["b.typ"]);
    expect(ids(closeToRight(m, "b.typ"))).toEqual(["a.typ", "b.typ"]);
  });
});

describe("navigation", () => {
  it("cycles with wrap in strip order", () => {
    const m = open("a.typ", "b.typ", "c.typ");
    expect(cycleTab(m, 1).active).toBe("a.typ");
    expect(cycleTab(openTab(m, "a.typ"), -1).active).toBe("c.typ");
    expect(cycleTab(EMPTY_TABS, 1)).toBe(EMPTY_TABS);
  });

  it("jumps by digit, with 9 meaning the last tab", () => {
    const m = open("a.typ", "b.typ", "c.typ");
    expect(tabByShortcut(m, 2).active).toBe("b.typ");
    expect(tabByShortcut(m, 9).active).toBe("c.typ");
    expect(tabByShortcut(m, 7)).toBe(m);
  });

  it("reorders, clamping the target", () => {
    const m = open("a.typ", "b.typ", "c.typ");
    expect(ids(moveTab(m, "a.typ", 2))).toEqual(["b.typ", "c.typ", "a.typ"]);
    expect(ids(moveTab(m, "c.typ", -5))).toEqual(["c.typ", "a.typ", "b.typ"]);
  });
});

describe("file operations", () => {
  it("follows a renamed file or folder", () => {
    const m = renamePath(open("sections/a.typ", "b.typ"), "sections", "chapters");
    expect(ids(m)).toEqual(["chapters/a.typ", "b.typ"]);
    expect(m.active).toBe("b.typ");
    expect(renamePath(open("a.typ"), "a.typ", "a.figure.json").tabs[0]?.kind).toBe(
      "figure",
    );
  });

  it("closes tabs inside a deleted folder", () => {
    const m = removePath(open("sections/a.typ", "sections/b.typ", "c.typ"), "sections");
    expect(ids(m)).toEqual(["c.typ"]);
    expect(m.active).toBe("c.typ");
  });
});

describe("labels", () => {
  it("adds the folder only when two tabs share a name", () => {
    const labels = tabLabels(open("a/index.md", "b/index.md", "main.typ").tabs);
    expect(labels.get("a/index.md")).toEqual({ name: "index.md", hint: "a" });
    expect(labels.get("main.typ")).toEqual({ name: "main.typ", hint: "" });
  });
});

describe("session", () => {
  it("round-trips tabs, views and the active tab", () => {
    const m = setView(openTab(open("a.typ", "b.typ"), "a.typ"), "b.typ", "slides");
    const restored = fromSession(
      sanitizeSession(JSON.parse(JSON.stringify(toSession("/p", m)))),
    );
    expect(ids(restored)).toEqual(["a.typ", "b.typ"]);
    expect(restored.active).toBe("a.typ");
    expect(restored.tabs[1]?.view).toBe("slides");
  });

  it("rejects junk, duplicates, and tabs without a project", () => {
    expect(sanitizeSession(7)).toEqual({ project: null, tabs: [], active: null });
    const s = sanitizeSession({
      project: "/p",
      tabs: [{ path: "a" }, { path: "a" }, 3, { path: "" }],
      active: "zzz",
    });
    expect(s.tabs).toHaveLength(1);
    expect(s.active).toBeNull();
    expect(sanitizeSession({ project: null, tabs: [{ path: "a" }] }).tabs).toEqual([]);
  });
});
