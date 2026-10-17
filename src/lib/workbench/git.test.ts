import { describe, expect, it } from "vitest";
import {
  formatRelativeTime,
  gitActionableError,
  groupGitFiles,
  parseGitError,
  statusBadge,
} from "./git";

describe("workbench git utilities", () => {
  it("groups files by staged, unstaged, untracked, and conflicts", () => {
    const files = [
      {
        path: "a.typ",
        staged: "modified" as const,
        unstaged: null,
        untracked: false,
        conflicted: false,
      },
      {
        path: "b.typ",
        staged: null,
        unstaged: "modified" as const,
        untracked: false,
        conflicted: false,
      },
      {
        path: "c.md",
        staged: null,
        unstaged: null,
        untracked: true,
        conflicted: false,
      },
      {
        path: "d.tex",
        staged: null,
        unstaged: null,
        untracked: false,
        conflicted: true,
      },
    ];

    const groups = groupGitFiles(files);
    expect(groups.staged.map((f) => f.path)).toEqual(["a.typ"]);
    expect(groups.changes.map((f) => f.path)).toEqual(["b.typ"]);
    expect(groups.untracked.map((f) => f.path)).toEqual(["c.md"]);
    expect(groups.conflicted.map((f) => f.path)).toEqual(["d.tex"]);
  });

  it("formats relative timestamps", () => {
    const now = 1_000_000;
    expect(formatRelativeTime(now - 10, now)).toBe("just now");
    expect(formatRelativeTime(now - 120, now)).toBe("2m ago");
    expect(formatRelativeTime(now - 7200, now)).toBe("2h ago");
    expect(formatRelativeTime(now - 86400, now)).toBe("yesterday");
    expect(formatRelativeTime(now - 172800, now)).toBe("2d ago");
  });

  it("parses structured Git error JSON", () => {
    const err = { message: JSON.stringify({ code: "missing-identity", message: "no identity" }) };
    const parsed = parseGitError(err);
    expect(parsed.code).toBe("missing-identity");
    expect(parsed.message).toBe("no identity");
  });

  it("returns actionable guidance for known error codes", () => {
    expect(gitActionableError("missing-identity", "some message")).toContain("git config --global user.name");
    expect(gitActionableError("nothing-to-commit", "some message")).toContain("Nothing staged");
    expect(gitActionableError("other", "plain fallback")).toBe("plain fallback");
  });

  it("formats status badges", () => {
    expect(statusBadge("modified").label).toBe("M");
    expect(statusBadge("added").label).toBe("A");
    expect(statusBadge(null, true).label).toBe("U");
  });
});
