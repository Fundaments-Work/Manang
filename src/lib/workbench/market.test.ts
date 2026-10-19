import { describe, expect, it } from "vitest";

import type { BrowseEntry } from "$lib/kernel/marketApi";

import {
  browseProblem,
  compareVersions,
  errorCode,
  errorText,
  formatCount,
  formatSize,
  freshnessNotice,
  initials,
  isNewer,
  isPluginTabPath,
  permissionItems,
  pluginIdOf,
  pluginTabPath,
  problemWording,
  updateCount,
  visibleBrowse,
} from "./market";

const entry = (over: Partial<BrowseEntry> & { id: string }): BrowseEntry => ({
  name: over.id,
  author: "Someone",
  description: "Does a thing",
  tags: [],
  license: "MIT",
  latest: "1.0.0",
  published: "2026-01-01T00:00:00Z",
  size: 1000,
  downloads: null,
  compatible: true,
  installed: null,
  ...over,
});

describe("version comparison", () => {
  it("orders numerically, not as text", () => {
    expect(compareVersions("0.10.0", "0.9.0")).toBeGreaterThan(0);
    expect(compareVersions("1.2.3", "1.2.3")).toBe(0);
    expect(compareVersions("0.1.9", "0.2.0")).toBeLessThan(0);
    expect(compareVersions("2.0.0", "1.99.99")).toBeGreaterThan(0);
  });

  it("puts a release above its pre-releases and orders pre-releases", () => {
    expect(isNewer("1.0.0", "1.0.0-rc.1")).toBe(true);
    expect(isNewer("1.0.0-rc.2", "1.0.0-rc.1")).toBe(true);
    expect(isNewer("1.0.0-rc.10", "1.0.0-rc.9")).toBe(true);
    expect(isNewer("1.0.0-alpha", "1.0.0-beta")).toBe(false);
    expect(isNewer("1.0.0+build5", "1.0.0")).toBe(false);
  });

  it("never offers an update on a guess", () => {
    expect(isNewer("latest", "1.0.0")).toBe(false);
    expect(isNewer("1.0.0", "unknown")).toBe(false);
    expect(isNewer("1.0.0", "1.0.0")).toBe(false);
  });
});

describe("searching and sorting the registry list", () => {
  const list = [
    entry({
      id: "zeta",
      name: "Zeta",
      tags: ["writing"],
      downloads: 5,
      published: "2026-03-01T00:00:00Z",
    }),
    entry({
      id: "alpha",
      name: "Alpha",
      author: "Ada",
      tags: ["math", "writing"],
      downloads: 50,
      published: "2026-01-01T00:00:00Z",
    }),
    entry({
      id: "mid",
      name: "Mid",
      tags: ["math"],
      downloads: 50,
      published: "2026-06-01T00:00:00Z",
    }),
  ];
  const ids = (entries: readonly BrowseEntry[]) => entries.map((e) => e.id);

  it("sorts by name, newest and downloads, ties by name", () => {
    expect(ids(visibleBrowse(list, "", null, "name"))).toEqual([
      "alpha",
      "mid",
      "zeta",
    ]);
    expect(ids(visibleBrowse(list, "", null, "newest"))).toEqual([
      "mid",
      "zeta",
      "alpha",
    ]);
    expect(ids(visibleBrowse(list, "", null, "downloads"))).toEqual([
      "alpha",
      "mid",
      "zeta",
    ]);
  });

  it("needs every word, in any field, ignoring case", () => {
    expect(ids(visibleBrowse(list, "ADA math", null, "name"))).toEqual(["alpha"]);
    expect(ids(visibleBrowse(list, "does a thing", null, "name"))).toHaveLength(3);
    expect(visibleBrowse(list, "nothing like this", null, "name")).toEqual([]);
  });

  it("narrows to a tag and keeps the order", () => {
    expect(ids(visibleBrowse(list, "", "math", "name"))).toEqual(["alpha", "mid"]);
    expect(ids(visibleBrowse(list, "mid", "Math", "name"))).toEqual(["mid"]);
  });

  it("counts the plugins that have an update", () => {
    const withUpdate = [
      ...list,
      entry({
        id: "old",
        installed: { version: "0.1.0", update_available: true },
      }),
    ];
    expect(updateCount(list)).toBe(0);
    expect(updateCount(withUpdate)).toBe(1);
  });
});

describe("permissions in plain words", () => {
  it("says what each capability lets a plugin do", () => {
    const items = permissionItems({
      filesystem: ["read:project"],
      network: ["api.example.org"],
      buffer: ["read:active"],
    });
    const texts = items.map((i) => i.text);
    expect(texts).toContain("Read files in your project");
    expect(texts).toContain("Connect to api.example.org");
    expect(texts).toContain("Read the open document");
    for (const item of items) expect(item.meaning.length).toBeGreaterThan(10);
  });

  it("lists changes before reads and distinguishes the project from its own folders", () => {
    const items = permissionItems({
      filesystem: ["read:project", "write:project", "write:cache"],
      network: [],
      buffer: [],
    });
    expect(items.map((i) => i.text)).toEqual([
      "Read and change files in your project",
      "Read and change its own cache folder",
      "Read files in your project",
    ]);
  });

  it("shows what an update adds, last, flagged new", () => {
    const items = permissionItems(
      { filesystem: ["read:project"], network: ["a.org", "B.org"], buffer: [] },
      { filesystem: [], network: ["b.ORG"], buffer: [] },
    );
    expect(items.map((i) => [i.text, i.isNew])).toEqual([
      ["Read files in your project", false],
      ["Connect to a.org", false],
      ["Connect to B.org", true],
    ]);
  });

  it("shows what it cannot describe instead of hiding it", () => {
    const items = permissionItems({
      filesystem: ["execute:everything"],
      network: [],
      buffer: [],
    });
    expect(items[0]?.text).toBe("Access to execute:everything");
  });

  it("an empty set lists nothing", () => {
    expect(permissionItems({ filesystem: [], network: [], buffer: [] })).toEqual([]);
  });
});

describe("listing state and errors", () => {
  it("says the date of an offline list and nothing otherwise", () => {
    const notice = freshnessNotice({
      state: "offline",
      reason: "x",
      fetched_at: Date.UTC(2026, 9, 17) / 1000,
      generated: "",
    });
    expect(notice).toContain("2026-10-17");
    expect(
      freshnessNotice({ state: "fresh", reason: null, fetched_at: 1, generated: "" }),
    ).toBeNull();
  });

  it("reads the code out of a kernel error", () => {
    expect(
      errorCode("[registry_signature] the plugin list could not be verified"),
    ).toBe("registry_signature");
    expect(errorText("[registry_off] switched off")).toBe("switched off");
    expect(errorCode("plain")).toBeNull();
  });

  it("classifies problems and words the signature failure as asked", () => {
    expect(browseProblem("[registry_signature] x")).toBe("signature");
    expect(browseProblem("[registry_unreachable] x")).toBe("unreachable");
    expect(browseProblem("[registry_off] x")).toBe("off");
    expect(browseProblem("[registry_schema] x")).toBe("schema");
    expect(browseProblem("[registry_not_configured] x")).toBe("not_configured");
    expect(browseProblem("[io] x")).toBe("other");
    const w = problemWording("signature", "");
    expect(w.title).toBe("The plugin list could not be verified");
    expect(w.detail).toContain("Nothing was installed");
  });
});

describe("small formatting", () => {
  it("formats sizes and counts", () => {
    expect(formatSize(512)).toBe("512 B");
    expect(formatSize(48_211)).toBe("47 KB");
    expect(formatSize(2 * 1024 * 1024)).toBe("2.0 MB");
    expect(formatCount(999)).toBe("999");
    expect(formatCount(1200)).toBe("1.2k");
    expect(formatCount(12_480)).toBe("12k");
    expect(formatCount(null)).toBe("");
  });

  it("makes initials for an icon", () => {
    expect(initials("Word count")).toBe("WC");
    expect(initials("table-tools")).toBe("TT");
    expect(initials("citations")).toBe("CI");
    expect(initials("")).toBe("?");
  });

  it("keeps plugin page paths apart from files", () => {
    expect(pluginTabPath("word-count")).toBe("plugin:word-count");
    expect(isPluginTabPath("plugin:word-count")).toBe(true);
    expect(isPluginTabPath("/home/a/plugin.typ")).toBe(false);
    expect(pluginIdOf("plugin:word-count")).toBe("word-count");
  });
});
