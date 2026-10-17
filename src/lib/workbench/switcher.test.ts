import { describe, expect, it } from "vitest";

import type { ProjectEntry } from "$lib/kernel/types";

import { clampCursor, rankProjects } from "./switcher";

function entry(
  over: Partial<ProjectEntry> & { path: string; name: string },
): ProjectEntry {
  return {
    engine: "typst",
    created: 1,
    last_opened: 1,
    pinned: false,
    archived: false,
    tags: [],
    exists: true,
    has_thumbnail: false,
    ...over,
  };
}

const all = [
  entry({ path: "/p/entropy", name: "Entropy paper", last_opened: 10 }),
  entry({ path: "/p/notes", name: "Lecture notes", last_opened: 30 }),
  entry({ path: "/p/thesis", name: "Thesis", last_opened: 20, pinned: true }),
  entry({ path: "/p/old", name: "Old", archived: true }),
  entry({ path: "/p/gone", name: "Gone", exists: false }),
];

const labels = (items: readonly { entry: ProjectEntry }[]): string[] =>
  items.map((i) => i.entry.name);

describe("project switcher", () => {
  it("leaves out archived and missing projects; pinned lead, then recent", () => {
    expect(labels(rankProjects(all, "", null))).toEqual([
      "Thesis",
      "Lecture notes",
      "Entropy paper",
    ]);
  });

  it("keeps the open project, marked and last", () => {
    const items = rankProjects(all, "", "/p/thesis");
    expect(labels(items)).toEqual(["Lecture notes", "Entropy paper", "Thesis"]);
    expect(items.map((i) => i.current)).toEqual([false, false, true]);
  });

  it("matches fuzzily on name, folder and tag", () => {
    expect(labels(rankProjects(all, "lec", null))).toEqual(["Lecture notes"]);
    expect(labels(rankProjects(all, "/p/th", null))).toEqual(["Thesis"]);
    const tagged = [entry({ path: "/x/a", name: "A", tags: ["phd"] })];
    expect(labels(rankProjects(tagged, "phd", null))).toEqual(["A"]);
    expect(rankProjects(all, "zzzz", null)).toEqual([]);
  });

  it("clamps the cursor", () => {
    expect(clampCursor(5, 3)).toBe(2);
    expect(clampCursor(-1, 3)).toBe(0);
    expect(clampCursor(0, 0)).toBe(0);
  });
});
