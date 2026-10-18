import { describe, expect, it } from "vitest";

import type { GitSyncStatus, GitUpdated } from "$lib/kernel/api";

import {
  addressProblem,
  sharedNotice,
  shareSuggestion,
  statusLine,
  unsavedHint,
  updatedWords,
} from "./share";

const base: GitSyncStatus = {
  state: "up-to-date",
  remote: "origin",
  branch: "main",
  ahead: 0,
  behind: 0,
  never_shared: false,
  uncommitted: 0,
};

describe("share status wording", () => {
  it("says what to do without jargon", () => {
    expect(statusLine({ ...base })).toBe("Up to date");
    expect(statusLine({ ...base, state: "ahead", ahead: 2 })).toBe("2 changes to share");
    expect(statusLine({ ...base, state: "ahead", ahead: 1 })).toBe("1 change to share");
    expect(statusLine({ ...base, state: "behind", behind: 1 })).toBe("Updates available");
    expect(statusLine({ ...base, state: "diverged", ahead: 3, behind: 1 })).toBe(
      "Updates available, and 3 changes to share",
    );
    expect(statusLine({ ...base, state: "ahead", ahead: 4, never_shared: true })).toBe(
      "Not shared yet: 4 versions to share",
    );
    expect(statusLine(null)).toBe("");
    for (const state of ["ahead", "behind", "diverged", "up-to-date"] as const) {
      expect(statusLine({ ...base, state, ahead: 1, behind: 1 })).not.toMatch(
        /push|pull|fetch|merge|rebase|upstream|branch/i,
      );
    }
  });

  it("suggests exactly one action", () => {
    expect(shareSuggestion({ ...base, state: "ahead", ahead: 1 })).toBe("share");
    expect(shareSuggestion({ ...base, state: "behind", behind: 1 })).toBe("get-updates");
    expect(shareSuggestion({ ...base, state: "diverged" })).toBe("get-updates");
    expect(shareSuggestion(base)).toBe("none");
    expect(shareSuggestion(null)).toBe("none");
  });

  it("mentions unsaved edits only when there are some", () => {
    expect(unsavedHint(base)).toBe("");
    expect(unsavedHint({ ...base, uncommitted: 1 })).toContain("1 file has unsaved");
    expect(unsavedHint({ ...base, uncommitted: 3 })).toContain("3 files have unsaved");
  });

  it("words the share notice", () => {
    expect(sharedNotice(2)).toBe("Shared 2 changes");
    expect(sharedNotice(0)).toBe("Everything is already shared");
  });
});

describe("get updates wording", () => {
  const got = (over: Partial<GitUpdated>): GitUpdated => ({
    result: "merged",
    files: [],
    received: 1,
    safety_version: null,
    ...over,
  });

  it("reports what arrived and any safety version", () => {
    expect(updatedWords(got({})).text).toBe("Got 1 update.");
    expect(updatedWords(got({ received: 3, safety_version: "abc" })).text).toContain(
      "kept as a version first",
    );
    expect(updatedWords(got({ result: "up-to-date" })).kind).toBe("notice");
  });

  it("names conflicting files and promises nothing changed", () => {
    const words = updatedWords(got({ result: "conflicts", files: ["a.typ", "b.typ"] }));
    expect(words.kind).toBe("conflicts");
    expect(words.text).toContain("a.typ, b.typ");
    expect(words.text).toContain("Nothing was changed");
  });
});

describe("address check", () => {
  it("asks for an address and refuses spaces", () => {
    expect(addressProblem("  ")).not.toBe("");
    expect(addressProblem("a b")).not.toBe("");
    expect(addressProblem("https://example.org/x.git")).toBe("");
  });
});
