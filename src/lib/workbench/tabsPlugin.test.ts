import { describe, expect, it } from "vitest";

import {
  EMPTY_TABS,
  closeTab,
  openTab,
  removePath,
  renamePath,
  tabLabels,
  toSession,
  type TabsModel,
} from "./tabs";

const open = (...paths: string[]): TabsModel =>
  paths.reduce((m, p) => openTab(m, p), EMPTY_TABS);
const ids = (m: TabsModel): string[] => m.tabs.map((t) => t.id);

describe("plugin pages as tabs", () => {
  it("opens one tab per plugin, reused when open again, labelled by id", () => {
    let m = openTab(open("main.typ"), "plugin:word-count");
    expect(m.tabs[1]?.kind).toBe("plugin");
    expect(m.active).toBe("plugin:word-count");
    m = openTab(openTab(m, "main.typ"), "plugin:word-count");
    expect(ids(m)).toEqual(["main.typ", "plugin:word-count"]);
    expect(m.active).toBe("plugin:word-count");
    expect(tabLabels(m.tabs).get("plugin:word-count")).toEqual({
      name: "word-count",
      hint: "",
    });
  });

  it("closes like any tab", () => {
    const m = closeTab(openTab(open("a.typ"), "plugin:x"), "plugin:x");
    expect(ids(m)).toEqual(["a.typ"]);
    expect(m.active).toBe("a.typ");
  });

  it("is not a file: never remembered, never renamed away", () => {
    const m = openTab(open("a.typ"), "plugin:x");
    expect(toSession("/p", m).tabs).toEqual([{ path: "a.typ", view: "text" }]);
    expect(toSession("/p", m).active).toBeNull();
    expect(ids(renamePath(m, "/old", "/new"))).toEqual(["a.typ", "plugin:x"]);
    expect(ids(removePath(m, "/p"))).toEqual(["a.typ", "plugin:x"]);
  });
});
