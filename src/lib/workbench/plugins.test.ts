import { describe, expect, it } from "vitest";

import { createDevKernel } from "$lib/kernel/dev";

import { sidePanelVisible } from "./layout";
import { groupPlugins } from "./plugins";

describe("plugins panel model", () => {
  it("groups the kernel's commands by plugin, sorted by name then id", async () => {
    const groups = groupPlugins(await createDevKernel().commands());
    expect(groups.map((g) => g.name)).toEqual(["editor", "figure", "project", "typst"]);
    expect(groups[3]?.commands.map((c) => c.id)).toEqual([
      "typst.compile",
      "typst.export-pdf",
      "typst.render",
    ]);
    expect(groups.reduce((n, g) => n + g.commands.length, 0)).toBe(8 + 1 + 12 + 3);
  });

  it("is empty, not invented, when the kernel lists nothing", () => {
    expect(groupPlugins([])).toEqual([]);
  });
});

describe("side panel visibility", () => {
  it("project panels need a project; plugins and settings do not", () => {
    expect(sidePanelVisible("files", false)).toBe(false);
    expect(sidePanelVisible("files", true)).toBe(true);
    expect(sidePanelVisible("plugins", false)).toBe(true);
    expect(sidePanelVisible("settings", false)).toBe(true);
    expect(sidePanelVisible(null, true)).toBe(false);
  });
});
