import { describe, expect, it } from "vitest";

import type { GitStatus, TreeEntry } from "$lib/kernel/types";

import { basename, dirname, isUnder, moved, nameProblem, uniqueName } from "./paths";
import {
  branchLabel,
  filterRows,
  gitMarks,
  isHiddenEntry,
  navigate,
  sortEntries,
  typeAhead,
  visibleRows,
} from "./tree";

const file = (path: string): TreeEntry => ({
  path,
  name: basename(path),
  kind: "file",
});
const dir = (path: string): TreeEntry => ({ path, name: basename(path), kind: "dir" });

const listings = new Map<string, TreeEntry[]>([
  [
    "",
    [
      file("main.typ"),
      dir("figures"),
      dir("sections"),
      file(".gitkeep"),
      file("10.typ"),
      file("2.typ"),
    ],
  ],
  ["figures", [file("figures/b.svg"), file("figures/a.svg")]],
]);

describe("sorting", () => {
  it("puts folders first, sorts naturally and hides the keep file", () => {
    expect(sortEntries(listings.get("") ?? []).map((e) => e.name)).toEqual([
      "figures",
      "sections",
      "2.typ",
      "10.typ",
      "main.typ",
    ]);
  });
});

describe("visible rows", () => {
  it("shows only the root until a folder is expanded", () => {
    const rows = visibleRows(listings, new Set());
    expect(rows.map((r) => r.path)).toEqual([
      "figures",
      "sections",
      "2.typ",
      "10.typ",
      "main.typ",
    ]);
    expect(rows[0]).toMatchObject({
      depth: 0,
      expanded: false,
      position: 1,
      setSize: 5,
    });
  });

  it("inlines expanded folders with depth and ARIA counts", () => {
    const rows = visibleRows(listings, new Set(["figures"]));
    expect(
      rows.slice(0, 3).map((r) => [r.path, r.depth, r.position, r.setSize]),
    ).toEqual([
      ["figures", 0, 1, 5],
      ["figures/a.svg", 1, 1, 2],
      ["figures/b.svg", 1, 2, 2],
    ]);
  });

  it("marks an expanded folder whose listing has not arrived as loading", () => {
    const rows = visibleRows(listings, new Set(["sections"]));
    expect(rows.find((r) => r.path === "sections")).toMatchObject({
      expanded: true,
      loading: true,
    });
  });
});

describe("filter", () => {
  const all = [
    file("main.typ"),
    file("sections/results.typ"),
    file("sections/background.typ"),
    dir("sections"),
  ];

  it("is fuzzy over the whole path, files only, with the folder as a hint", () => {
    const rows = filterRows(all, "resu");
    expect(rows.map((r) => r.path)).toEqual(["sections/results.typ"]);
    expect(rows[0]?.hint).toBe("sections");
  });
});

describe("git", () => {
  const status: GitStatus = {
    is_repo: true,
    branch: "main",
    detached: false,
    head: "abcdef0123",
    truncated: false,
    changes: [
      { path: "sections/results.typ", staged: null, unstaged: "modified" },
      { path: "notes.md", staged: null, unstaged: "untracked" },
      { path: "a.typ", staged: "added", unstaged: null },
    ],
  };

  it("maps changes to badges and flags their folders", () => {
    const marks = gitMarks(status);
    expect(marks.files.get("sections/results.typ")).toEqual({
      letter: "M",
      label: "Modified",
    });
    expect(marks.files.get("notes.md")?.letter).toBe("U");
    expect(marks.files.get("a.typ")?.letter).toBe("A");
    expect([...marks.folders]).toEqual(["sections"]);
  });

  it("shows nothing outside a repository", () => {
    expect(gitMarks(null).files.size).toBe(0);
    expect(gitMarks({ ...status, is_repo: false }).files.size).toBe(0);
  });

  it("labels the branch, starring it when the work tree is dirty", () => {
    expect(branchLabel(status)).toBe("main*");
    expect(branchLabel({ ...status, changes: [] })).toBe("main");
    expect(branchLabel({ ...status, branch: null, changes: [] })).toBe("abcdef0");
    expect(branchLabel(null)).toBeNull();
  });
});

describe("keyboard navigation (APG tree)", () => {
  const rows = visibleRows(listings, new Set(["figures"]));

  it("moves with arrows, Home and End", () => {
    expect(navigate(rows, "figures", "ArrowDown")).toEqual({
      action: "focus",
      path: "figures/a.svg",
    });
    expect(navigate(rows, "figures", "ArrowUp")).toEqual({ action: "none" });
    expect(navigate(rows, "sections", "Home")).toEqual({
      action: "focus",
      path: "figures",
    });
    expect(navigate(rows, "figures", "End")).toEqual({
      action: "focus",
      path: "main.typ",
    });
  });

  it("Right opens a closed folder, then enters it; Left closes, then goes to the parent", () => {
    expect(navigate(rows, "sections", "ArrowRight")).toEqual({
      action: "expand",
      path: "sections",
    });
    expect(navigate(rows, "figures", "ArrowRight")).toEqual({
      action: "focus",
      path: "figures/a.svg",
    });
    expect(navigate(rows, "figures", "ArrowLeft")).toEqual({
      action: "collapse",
      path: "figures",
    });
    expect(navigate(rows, "figures/b.svg", "ArrowLeft")).toEqual({
      action: "focus",
      path: "figures",
    });
    expect(navigate(rows, "main.typ", "ArrowRight")).toEqual({ action: "none" });
  });

  it("starts at the first row when nothing is focused", () => {
    expect(navigate(rows, "gone", "ArrowDown")).toEqual({
      action: "focus",
      path: "figures",
    });
  });

  it("type-ahead finds the next row by prefix, wrapping", () => {
    expect(typeAhead(rows, "figures", "m")).toBe("main.typ");
    expect(typeAhead(rows, "main.typ", "f")).toBe("figures");
    expect(typeAhead(rows, "figures", "zzz")).toBeNull();
  });
});

describe("paths", () => {
  it("splits and tests ancestry", () => {
    expect(dirname("a/b/c.typ")).toBe("a/b");
    expect(dirname("c.typ")).toBe("");
    expect(isUnder("a/b/c", "a/b")).toBe(true);
    expect(isUnder("a/bc", "a/b")).toBe(false);
    expect(moved("a/b/c", "a/b", "x")).toBe("x/c");
    expect(moved("a/bc", "a/b", "x")).toBe("a/bc");
  });

  it("explains why a name is refused", () => {
    const siblings = new Set(["a.typ"]);
    expect(nameProblem("", siblings)).toBe("Enter a name.");
    expect(nameProblem("x/y", siblings)).toBe("Names cannot contain slashes.");
    expect(nameProblem("a.typ", siblings)).toBe(
      '"a.typ" already exists in this folder.',
    );
    expect(nameProblem("a.typ", siblings, "a.typ")).toBeNull();
    expect(nameProblem("b.typ", siblings)).toBeNull();
  });

  it("finds the next free untitled name", () => {
    expect(uniqueName("untitled.typ", new Set())).toBe("untitled.typ");
    expect(
      uniqueName("untitled.typ", new Set(["untitled.typ", "untitled 2.typ"])),
    ).toBe("untitled 3.typ");
  });
});

describe("hidden entries", () => {
  const withHidden = new Map<string, readonly TreeEntry[]>([
    ["", [file(".gitignore"), file("manang.toml"), file("main.typ"), dir(".manang")]],
    ["figures", [file("figures/manang.toml")]],
  ]);

  it("keeps dotfiles and the project config out of the tree", () => {
    expect(visibleRows(withHidden, new Set()).map((r) => r.path)).toEqual(["main.typ"]);
  });

  it("lists them when asked, and only the root manang.toml counts as the config", () => {
    expect(visibleRows(withHidden, new Set(), true).map((r) => r.path)).toContain(
      "manang.toml",
    );
    expect(isHiddenEntry(file("figures/manang.toml"))).toBe(false);
  });
});
