import { describe, expect, it } from "vitest";

import type { ProjectEntry } from "$lib/kernel/types";

import {
  RECENT_LIMIT,
  applyPatch,
  emptyState,
  matchesQuery,
  parseTags,
  relativeTime,
  sectionCounts,
  selectProjects,
  slugify,
  suggestLocation,
} from "./home";

function entry(over: Partial<ProjectEntry> & { path: string }): ProjectEntry {
  return {
    name: over.path.split("/").pop() ?? over.path,
    engine: "typst",
    created: 100,
    last_opened: 100,
    pinned: false,
    archived: false,
    tags: [],
    exists: true,
    has_thumbnail: false,
    ...over,
  };
}

const alpha = entry({
  path: "/p/alpha",
  name: "Alpha",
  last_opened: 30,
  created: 5,
  tags: ["phd"],
});
const beta = entry({
  path: "/p/beta",
  name: "beta",
  last_opened: 50,
  created: 9,
  pinned: true,
});
const gamma = entry({
  path: "/p/gamma",
  name: "Gamma",
  last_opened: 10,
  created: 7,
  archived: true,
});
const delta = entry({
  path: "/q/delta",
  name: "Delta",
  last_opened: 40,
  created: 1,
  exists: false,
});
const sample: ProjectEntry[] = [alpha, beta, gamma, delta];

const names = (list: readonly ProjectEntry[]): string[] => list.map((e) => e.name);

describe("home sections", () => {
  it("projects hides the archive and floats pinned ones", () => {
    const out = selectProjects(sample, {
      section: "projects",
      query: "",
      sort: "modified",
    });
    expect(names(out)).toEqual(["beta", "Delta", "Alpha"]);
  });

  it("sorts by name (case-insensitively) and by created", () => {
    const base = { section: "projects" as const, query: "" };
    const unpinned = sample.map((e) => ({ ...e, pinned: false }));
    expect(names(selectProjects(unpinned, { ...base, sort: "name" }))).toEqual([
      "Alpha",
      "beta",
      "Delta",
    ]);
    expect(names(selectProjects(unpinned, { ...base, sort: "created" }))).toEqual([
      "beta",
      "Alpha",
      "Delta",
    ]);
  });

  it("pinned and archive show only their own", () => {
    const q = { query: "", sort: "modified" as const };
    expect(names(selectProjects(sample, { ...q, section: "pinned" }))).toEqual([
      "beta",
    ]);
    expect(names(selectProjects(sample, { ...q, section: "archive" }))).toEqual([
      "Gamma",
    ]);
  });

  it("recent skips the archive and missing folders, newest first, capped", () => {
    expect(
      names(selectProjects(sample, { section: "recent", query: "", sort: "name" })),
    ).toEqual(["beta", "Alpha"]);
    const many = Array.from({ length: RECENT_LIMIT + 5 }, (_, i) =>
      entry({ path: `/m/${i}`, last_opened: i }),
    );
    expect(
      selectProjects(many, { section: "recent", query: "", sort: "name" }),
    ).toHaveLength(RECENT_LIMIT);
  });

  it("templates has no projects", () => {
    expect(
      selectProjects(sample, { section: "templates", query: "", sort: "name" }),
    ).toEqual([]);
  });
});

describe("home search", () => {
  it("matches name, folder and tag, ignoring case and padding", () => {
    expect(matchesQuery(alpha, "  ALP ")).toBe(true);
    expect(matchesQuery(alpha, "phd")).toBe(true);
    expect(matchesQuery(delta, "/q/")).toBe(true);
    expect(matchesQuery(beta, "phd")).toBe(false);
    expect(matchesQuery(beta, "")).toBe(true);
  });

  it("narrows a section", () => {
    const out = selectProjects(sample, {
      section: "projects",
      query: "phd",
      sort: "modified",
    });
    expect(names(out)).toEqual(["Alpha"]);
  });
});

describe("pin and archive", () => {
  it("applies a patch only to the named project", () => {
    const pinned = applyPatch(sample, "/p/alpha", { pinned: true, name: " Renamed " });
    expect(pinned[0]).toMatchObject({ pinned: true, name: "Renamed" });
    expect(pinned[1]).toEqual(beta);
    const archived = applyPatch(pinned, "/p/beta", { archived: true });
    const q = { query: "", sort: "name" as const };
    expect(names(selectProjects(archived, { ...q, section: "pinned" }))).toEqual([
      "Renamed",
    ]);
    expect(names(selectProjects(archived, { ...q, section: "archive" }))).toEqual([
      "beta",
      "Gamma",
    ]);
  });

  it("counts for the nav", () => {
    expect(sectionCounts(sample)).toEqual({ projects: 3, pinned: 1, archive: 1 });
  });
});

describe("helpers", () => {
  const now = 10_000_000;
  it("says how long ago in plain words", () => {
    expect(relativeTime(now - 10, now)).toBe("just now");
    expect(relativeTime(now - 90, now)).toBe("1 minute ago");
    expect(relativeTime(now - 3 * 3600, now)).toBe("3 hours ago");
    expect(relativeTime(now - 86_400, now)).toBe("yesterday");
    expect(relativeTime(now - 3 * 86_400, now)).toBe("3 days ago");
    expect(relativeTime(now - 15 * 86_400, now)).toBe("2 weeks ago");
    expect(relativeTime(0, now)).toBe("never");
  });

  it("parses tags, dropping blanks and duplicates", () => {
    expect(parseTags("phd, Draft,, phd\n draft ,thesis")).toEqual([
      "phd",
      "Draft",
      "thesis",
    ]);
  });

  it("slugifies names and suggests a location", () => {
    expect(slugify("  Entropy Paper! ")).toBe("entropy-paper");
    expect(suggestLocation(sample)).toBe("/p");
    expect(suggestLocation([])).toBe("");
  });

  it("invites an action in every empty state", () => {
    expect(emptyState("projects", "", 0).action).toBe("new");
    expect(emptyState("projects", "zz", 3).action).toBe("clear");
    expect(emptyState("pinned", "", 3).title).toBe("Nothing pinned");
    expect(emptyState("recent", "", 3).action).toBe("open");
  });
});
