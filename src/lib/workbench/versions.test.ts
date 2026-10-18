import { describe, expect, it } from "vitest";

import type { GitStatusFile } from "$lib/kernel/api";
import {
  changeSummary,
  changedPaths,
  defaultVersionName,
  fileCount,
  planRestore,
  restoredNotice,
  restoreWords,
  versionTitle,
} from "./versions";

function file(path: string, over: Partial<GitStatusFile> = {}): GitStatusFile {
  return {
    path,
    staged: null,
    unstaged: null,
    untracked: false,
    conflicted: false,
    ...over,
  };
}

describe("changedPaths", () => {
  it("counts staged, unstaged, untracked and conflicted files", () => {
    const files = [
      file("a.typ", { unstaged: "modified" }),
      file("b.typ", { staged: "added" }),
      file("c.typ", { untracked: true }),
      file("d.typ", { conflicted: true }),
      file("e.typ"),
    ];
    expect(changedPaths(files)).toEqual(["a.typ", "b.typ", "c.typ", "d.typ"]);
  });
});

describe("wording", () => {
  it("summarises changes", () => {
    expect(changeSummary(0)).toBe("Nothing has changed since the last version.");
    expect(changeSummary(1)).toBe("1 file has changed since the last version.");
    expect(changeSummary(3)).toBe("3 files have changed since the last version.");
    expect(fileCount(2)).toBe("2 files");
  });

  it("titles a version, never blank", () => {
    expect(versionTitle({ subject: " First draft " })).toBe("First draft");
    expect(versionTitle({ subject: "  " })).toBe("(no description)");
  });

  it("makes a dated default name in local time", () => {
    expect(defaultVersionName(new Date(2026, 9, 18, 14, 5))).toBe(
      "Version of 2026-10-18 14:05",
    );
  });
});

describe("planRestore", () => {
  it("is exact for a file restore", () => {
    expect(planRestore({ kind: "file", paths: ["a.typ"] }, ["b.typ"])).toEqual({
      dirty: [],
      saveFirst: false,
    });
    expect(planRestore({ kind: "file", paths: ["a.typ"] }, ["a.typ", "b.typ"])).toEqual(
      {
        dirty: ["a.typ"],
        saveFirst: true,
      },
    );
  });

  it("treats any change as at risk for a whole restore", () => {
    expect(planRestore({ kind: "whole", paths: [] }, ["b.typ"]).saveFirst).toBe(true);
    expect(planRestore({ kind: "whole", paths: [] }, []).saveFirst).toBe(false);
  });
});

describe("restoreWords", () => {
  const version = { short: "d3b8902", subject: "First draft" };

  it("says plainly what happens, and offers the safety save when dirty", () => {
    const scope = { kind: "file", paths: ["a.typ"] } as const;
    const words = restoreWords(scope, version, planRestore(scope, ["a.typ"]));
    expect(words.title).toBe("Restore this file?");
    expect(words.message).toContain("a.typ will be replaced by its content");
    expect(words.message).toContain("saved first");
    expect(words.message).toContain("Before restoring d3b8902");
    expect(words.message).toContain("Nothing is deleted");
    expect(words.action).toBe("Save my changes first, then restore");
  });

  it("is a plain Restore when nothing is at risk", () => {
    const scope = { kind: "whole", paths: ["a.typ", "b.typ"] } as const;
    const words = restoreWords(scope, version, planRestore(scope, []));
    expect(words.title).toBe("Restore the whole project?");
    expect(words.message).not.toContain("saved first");
    expect(words.action).toBe("Restore");
  });
});

describe("restoredNotice", () => {
  it("reports what came back and what was left alone", () => {
    expect(restoredNotice([], [])).toContain("already match");
    expect(restoredNotice(["a.typ"], [])).toBe("Restored a.typ.");
    expect(restoredNotice(["a.typ", "b.typ"], ["n.typ"])).toBe(
      "Restored 2 files. 1 file added since then was left as they are.",
    );
  });
});
