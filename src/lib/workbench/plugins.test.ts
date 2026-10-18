import { describe, expect, it } from "vitest";

import { createDevKernel } from "$lib/kernel/dev";

import { sidePanelVisible } from "./layout";
import { describeAccess, groupPlugins, listPlugins, pluginEntries } from "./plugins";

describe("plugins panel model", () => {
  it("groups the kernel's commands by plugin, sorted by name then id", async () => {
    const groups = groupPlugins(await createDevKernel().commands());
    expect(groups.map((g) => g.name)).toEqual([
      "agent",
      "citations",
      "diagram",
      "editor",
      "figure",
      "git",
      "heal",
      "latex",
      "live",
      "markdown",
      "plugins",
      "project",
      "recovery",
      "search",
      "settings",
      "typst",
      "visual",
      "writing",
    ]);
    expect(
      groups.find((g) => g.name === "citations")?.commands.map((c) => c.id),
    ).toEqual([
      "citations.configure",
      "citations.extract-keys",
      "citations.format",
      "citations.get",
      "citations.load",
      "citations.search",
      "citations.verify",
    ]);
    expect(groups.find((g) => g.name === "git")?.commands.length).toBe(27);
    expect(groups.find((g) => g.name === "settings")?.commands.length).toBe(14);
    expect(groups.find((g) => g.name === "writing")?.commands.length).toBe(14);
    expect(groups.find((g) => g.name === "heal")?.commands.map((c) => c.id)).toEqual([
      "heal.configure",
      "heal.suggest",
    ]);
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
      "typst.jump-from-click",
      "typst.jump-from-cursor",
      "typst.math-to-svg",
      "typst.render",
      "typst.svg-to-pdf",
    ]);
    // agent 25, citations 7, diagram 5, editor 8, figure 21, git 27, heal 2, latex 4,
    // live 11, markdown 4, plugins 10, project 30, recovery 4, search 6, settings 14,
    // typst 7, visual 21, writing 14.
    expect(groups.reduce((n, g) => n + g.commands.length, 0)).toBe(
      25 + 7 + 5 + 8 + 21 + 27 + 2 + 4 + 11 + 4 + 10 + 30 + 4 + 6 + 14 + 7 + 21 + 14,
    );
  });

  it("is empty, not invented, when the kernel lists nothing", () => {
    expect(groupPlugins([])).toEqual([]);
  });
});

describe("side panel visibility", () => {
  it("side panels need a project", () => {
    expect(sidePanelVisible("files", false)).toBe(false);
    expect(sidePanelVisible("files", true)).toBe(true);
    expect(sidePanelVisible(null, true)).toBe(false);
  });
});

describe("plugin entries", () => {
  it("describes manifest permissions in plain words", () => {
    expect(
      describeAccess({
        filesystem: ["read:project", "write:cache"],
        network: ["api.crossref.org", "api.openalex.org"],
      }),
    ).toEqual([
      "Reads and changes its cache",
      "Reads project files",
      "Connects to api.crossref.org, api.openalex.org",
    ]);
    expect(describeAccess({ filesystem: ["read:project", "write:project"] })).toEqual([
      "Reads and changes project files",
    ]);
    expect(describeAccess({ filesystem: ["odd"] })).toEqual(["odd"]);
    expect(describeAccess({})).toEqual([]);
  });

  it("joins manifest summaries with the command list", async () => {
    const kernel = createDevKernel();
    const entries = pluginEntries(await listPlugins(kernel), await kernel.commands());
    const latex = entries.find((e) => e.name === "latex");
    expect(latex?.tier).toBe("native");
    expect(latex?.settings).toContain("latex.engine");
    expect(latex?.commands.some((c) => c.id === "latex.compile")).toBe(true);
  });

  it("falls back to the commands when the shell has no plugin list", () => {
    const entries = pluginEntries(null, [{ id: "a.run", title: "Run", plugin: "a" }]);
    expect(entries).toEqual([
      {
        name: "a",
        version: "",
        description: "",
        tier: null,
        access: [],
        settings: [],
        commands: [{ id: "a.run", title: "Run", plugin: "a" }],
      },
    ]);
  });
});
