import { describe, expect, it } from "vitest";

import type { PluginAccess, PluginState } from "$lib/kernel/pluginsApi";

import {
  EMPTY_ACCESS,
  PLUGIN_FILTERS,
  accessDifference,
  accessIsEmpty,
  accessLines,
  accessWithin,
  canToggle,
  canUninstall,
  disabledReason,
  filterCounts,
  installedDate,
  matchesFilter,
  matchesQuery,
  needsAttention,
  stateLabel,
  tierLabel,
  visiblePlugins,
  withPlugin,
} from "./pluginManager";

const access = (a: Partial<PluginAccess>): PluginAccess => ({ ...EMPTY_ACCESS, ...a });

function plugin(over: Partial<PluginState> & { name: string }): PluginState {
  return {
    version: "0.1.0",
    description: "",
    tier: "wasm",
    core: false,
    permissions: EMPTY_ACCESS,
    granted: EMPTY_ACCESS,
    new_permissions: EMPTY_ACCESS,
    enabled: true,
    status: "loaded",
    signed: false,
    commands: [],
    settings: [],
    contexts: ["app"],
    update_pending: false,
    ...over,
  };
}

const LIST: PluginState[] = [
  plugin({ name: "editor", tier: "native", core: true }),
  plugin({ name: "git", tier: "native", description: "Version history" }),
  plugin({
    name: "word-count",
    description: "Counts words",
    commands: [{ id: "word-count.count", title: "Count words" }],
  }),
  plugin({ name: "latex-lint", status: "failed", error: "boom" }),
  plugin({
    name: "citation-fetch",
    status: "needs_approval",
    publisher: "Example Labs",
  }),
  plugin({ name: "slide-themes", status: "disabled", enabled: false }),
];

describe("labels", () => {
  it("names the tier and the state in plain words", () => {
    expect(tierLabel({ tier: "native" })).toBe("Built in");
    expect(tierLabel({ tier: "wasm" })).toBe("Sandboxed");
    expect(stateLabel({ status: "loaded" })).toBe("Enabled");
    expect(stateLabel({ status: "disabled" })).toBe("Disabled");
    expect(stateLabel({ status: "failed" })).toBe("Failed");
    expect(stateLabel({ status: "needs_approval" })).toBe("Needs approval");
  });

  it("explains who switched a plugin off", () => {
    expect(disabledReason({ disabled_by: "project" })).toMatch(/project/);
    expect(disabledReason({})).toMatch(/you/);
  });

  it("formats the install date from Unix seconds", () => {
    expect(installedDate(0)).toBe("1970-01-01");
    expect(installedDate(1_760_745_600)).toBe("2025-10-18");
  });
});

describe("what a plugin may do", () => {
  it("lets core plugins stay on and built-ins stay installed", () => {
    expect(canToggle({ core: true })).toBe(false);
    expect(canToggle({ core: false })).toBe(true);
    expect(canUninstall({ core: false, tier: "wasm" })).toBe(true);
    expect(canUninstall({ core: false, tier: "native" })).toBe(false);
    expect(canUninstall({ core: true, tier: "wasm" })).toBe(false);
  });

  it("flags failed and waiting plugins as needing attention", () => {
    expect(needsAttention({ status: "failed" })).toBe(true);
    expect(needsAttention({ status: "needs_approval" })).toBe(true);
    expect(needsAttention({ status: "disabled" })).toBe(false);
    expect(needsAttention({ status: "loaded" })).toBe(false);
  });
});

describe("filters and search", () => {
  it("has the four chips, in order", () => {
    expect(PLUGIN_FILTERS.map((f) => f.label)).toEqual([
      "All",
      "Installed",
      "Built in",
      "Needs attention",
    ]);
  });

  it("splits built-in from installed and finds what needs attention", () => {
    const names = (f: Parameters<typeof matchesFilter>[1]) =>
      LIST.filter((p) => matchesFilter(p, f)).map((p) => p.name);
    expect(names("all")).toHaveLength(6);
    expect(names("builtin")).toEqual(["editor", "git"]);
    expect(names("installed")).toEqual([
      "word-count",
      "latex-lint",
      "citation-fetch",
      "slide-themes",
    ]);
    expect(names("attention")).toEqual(["latex-lint", "citation-fetch"]);
    expect(filterCounts(LIST)).toEqual({
      all: 6,
      installed: 4,
      builtin: 2,
      attention: 2,
    });
  });

  it("searches name, description, publisher and command ids and titles", () => {
    const hits = (q: string) =>
      LIST.filter((p) => matchesQuery(p, q)).map((p) => p.name);
    expect(hits("")).toHaveLength(6);
    expect(hits("  ")).toHaveLength(6);
    expect(hits("GIT")).toEqual(["git"]);
    expect(hits("version history")).toEqual(["git"]);
    expect(hits("example labs")).toEqual(["citation-fetch"]);
    expect(hits("word-count.count")).toEqual(["word-count"]);
    expect(hits("count words")).toEqual(["word-count"]);
    expect(hits("zzz")).toEqual([]);
  });

  it("lists what needs attention first, then by name", () => {
    expect(visiblePlugins(LIST, "all", "").map((p) => p.name)).toEqual([
      "citation-fetch",
      "latex-lint",
      "editor",
      "git",
      "slide-themes",
      "word-count",
    ]);
    expect(visiblePlugins(LIST, "installed", "lint").map((p) => p.name)).toEqual([
      "latex-lint",
    ]);
  });

  it("replaces a plugin in the list or adds it, keeping the order", () => {
    const next = plugin({ name: "git", status: "disabled", enabled: false });
    const replaced = withPlugin(LIST, next);
    expect(replaced).toHaveLength(6);
    expect(replaced.find((p) => p.name === "git")?.status).toBe("disabled");
    const added = withPlugin(LIST, plugin({ name: "aaa" }));
    expect(added[0]?.name).toBe("aaa");
  });
});

describe("permission sets", () => {
  const approved = access({ filesystem: ["read:project"], network: ["Example.org"] });
  const asked = access({
    filesystem: ["read:project", "write:project"],
    network: ["example.org", "api.example.org"],
    buffer: ["read:active"],
  });

  it("compares, ignoring the case of network hosts", () => {
    expect(accessWithin(approved, asked)).toBe(true);
    expect(accessWithin(asked, approved)).toBe(false);
    expect(accessWithin(EMPTY_ACCESS, approved)).toBe(true);
  });

  it("finds what an update adds", () => {
    expect(accessDifference(asked, approved)).toEqual(
      access({
        filesystem: ["write:project"],
        network: ["api.example.org"],
        buffer: ["read:active"],
      }),
    );
    expect(accessIsEmpty(accessDifference(approved, asked))).toBe(true);
  });

  it("highlights the new lines of an update, listed after the kept ones", () => {
    const added = accessDifference(asked, approved);
    const lines = accessLines(asked, added);
    expect(lines.filter((l) => l.isNew).length).toBeGreaterThan(0);
    const firstNew = lines.findIndex((l) => l.isNew);
    expect(lines.slice(0, firstNew).every((l) => !l.isNew)).toBe(true);
    expect(lines.some((l) => l.text.includes("api.example.org") && l.isNew)).toBe(true);
    expect(lines.some((l) => l.text.includes("the open document") && l.isNew)).toBe(
      true,
    );
  });

  it("does not highlight when everything is new (a first install) or nothing is", () => {
    expect(accessLines(asked, asked).every((l) => !l.isNew)).toBe(true);
    expect(accessLines(asked).every((l) => !l.isNew)).toBe(true);
    expect(accessLines(asked, EMPTY_ACCESS).every((l) => !l.isNew)).toBe(true);
    expect(accessLines(EMPTY_ACCESS)).toEqual([]);
  });
});
