import { describe, expect, it } from "vitest";

import { createDevKernel } from "$lib/kernel/dev";

import { sidePanelVisible } from "./layout";
import { groupPlugins } from "./plugins";

describe("plugins panel model", () => {
  it("groups the kernel's commands by plugin, sorted by name then id", async () => {
    const groups = groupPlugins(await createDevKernel().commands());
    expect(groups.map((g) => g.name)).toEqual([
      "citations",
      "editor",
      "figure",
      "git",
      "latex",
      "markdown",
      "project",
      "search",
      "settings",
      "typst",
      "writing",
    ]);
    expect(
      groups.find((g) => g.name === "citations")?.commands.map((c) => c.id),
    ).toEqual([
      "citations.extract-keys",
      "citations.format",
      "citations.get",
      "citations.load",
      "citations.search",
      "citations.verify",
    ]);
    expect(groups.find((g) => g.name === "git")?.commands.length).toBe(15);
    expect(groups.find((g) => g.name === "settings")?.commands.length).toBe(14);
    expect(groups.find((g) => g.name === "writing")?.commands.length).toBe(14);
    expect(groups.find((g) => g.name === "search")?.commands.map((c) => c.id)).toEqual([
      "search.cancel",
      "search.files",
      "search.in-text",
      "search.replace-apply",
      "search.replace-preview",
      "search.text",
    ]);
    expect(groups.find((g) => g.name === "typst")?.commands.map((c) => c.id)).toEqual([
      "typst.compile",
      "typst.export-pdf",
      "typst.render",
    ]);
    // citations 6, editor 8, figure 1, git 15, latex 3, markdown 2,
    // project 18, search 6, settings 14, typst 3, writing 14.
    expect(groups.reduce((n, g) => n + g.commands.length, 0)).toBe(
      6 + 8 + 1 + 15 + 3 + 2 + 18 + 6 + 14 + 3 + 14,
    );
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
