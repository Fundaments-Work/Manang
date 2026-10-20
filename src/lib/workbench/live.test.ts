import { describe, expect, it } from "vitest";

import {
  canShareLive,
  connectionDetail,
  connectionLabel,
  deletedSentence,
  deletedTag,
  PAUSED_DISCARD_NOTE,
  pausedSentence,
  plainLiveError,
  unsyncedWarning,
  editingAllowed,
  initials,
  inviteFileLabel,
  liveBlock,
  liveChipText,
  participantSummary,
  peopleCount,
  versionMessage,
} from "./live";

describe("presence wording", () => {
  const ada = { peer: "a", name: "Ada Lovelace", slot: 1 } as never;
  it("counts people with you", () => {
    expect(peopleCount([])).toBe(1);
    expect(peopleCount([ada])).toBe(2);
  });
  it("words the status chip", () => {
    expect(liveChipText("connected", [ada])).toBe("Live: 2 people");
    expect(liveChipText("connected", [])).toBe("Live: just you");
    expect(liveChipText("reconnecting", [])).toBe("Live: reconnecting");
    expect(liveChipText("error", [])).toBe("Live: stopped");
  });
  it("makes initials", () => {
    expect(initials("Ada Lovelace")).toBe("AL");
    expect(initials("grace brewster murray hopper")).toBe("GH");
    expect(initials("Plato")).toBe("P");
    expect(initials("  ")).toBe("?");
  });
});

describe("live session wording", () => {
  it("says what each state means", () => {
    expect(connectionLabel("connecting", 0)).toBe("Connecting");
    expect(connectionLabel("connected", 0)).toBe("Live");
    expect(connectionLabel("reconnecting", 1)).toBe("Reconnecting");
    expect(connectionLabel("reconnecting", 3)).toBe("Reconnecting (attempt 3)");
    expect(connectionLabel("left", 0)).toBe("Left the session");
    expect(connectionLabel("error", 0)).toBe("Session stopped");
  });

  it("reassures while reconnecting and explains an error", () => {
    expect(connectionDetail("reconnecting", null)).toMatch(/edits are kept/);
    expect(connectionDetail("error", "The copies no longer match.")).toBe(
      "The copies no longer match.",
    );
    expect(connectionDetail("error", null)).toBe("The session failed.");
    expect(connectionDetail("connected", null)).toBeNull();
  });

  it("lists people, you first", () => {
    expect(participantSummary([])).toBe("Only you so far");
    expect(participantSummary([{ peer: "1", name: "Ada", slot: 0 }])).toBe(
      "You and Ada",
    );
    expect(
      participantSummary([
        { peer: "1", name: "Ada", slot: 0 },
        { peer: "2", name: "Grace", slot: 1 },
      ]),
    ).toBe("You, Ada and Grace");
  });

  it("shares text documents only", () => {
    expect(canShareLive("document")).toBe(true);
    expect(canShareLive("figure")).toBe(false);
    expect(canShareLive("viewer")).toBe(false);
    expect(canShareLive(undefined)).toBe(false);
  });

  it("names the file of an invite", () => {
    expect(inviteFileLabel("chapters/intro.typ")).toBe("intro.typ");
    expect(inviteFileLabel(null)).toBe("the shared file");
    expect(inviteFileLabel("")).toBe("the shared file");
  });
});

describe("waiting for a host", () => {
  it("says so plainly and keeps the file read-only", () => {
    expect(connectionLabel("waiting", 0)).toBe("Waiting for host");
    expect(liveChipText("waiting", [])).toBe("Live: waiting for host");
    expect(connectionDetail("waiting", null)).toContain("kept as it is");
    expect(editingAllowed("waiting")).toBe(false);
    expect(editingAllowed("connected")).toBe(true);
    expect(editingAllowed("error")).toBe(false);
  });
});

describe("live guards", () => {
  it("blocks Get updates while a file is live, in plain words", () => {
    const text = liveBlock("get-updates", "chapters/intro.typ");
    expect(text).toContain("intro.typ");
    expect(text).toContain("leave the live session");
    expect(liveBlock("get-updates", null)).toBeNull();
  });
  it("blocks a restore only if it touches the live file", () => {
    expect(liveBlock("restore", "a.typ", ["b.typ"])).toBeNull();
    expect(liveBlock("restore", "a.typ", ["a.typ"])).toContain("a.typ");
    expect(liveBlock("restore", "a.typ", [])).toContain("a.typ"); // whole project
  });
  it("explains a disk change to a live file", () => {
    expect(liveBlock("reload", "a.typ")).toContain("session's version is kept");
  });
  it("names people without an email in the version message", () => {
    expect(versionMessage(undefined, [])).toBeUndefined();
    expect(versionMessage(undefined, ["Ada", "Grace"])).toBe(
      "Also edited by: Ada, Grace",
    );
    expect(versionMessage("Draft.", ["Ada"])).toBe("Draft.\n\nAlso edited by: Ada");
  });
});

describe("plain live errors and the host-leaving warning", () => {
  it("rewrites every code the project driver adds, without jargon", () => {
    for (const code of [
      "needs-confirmation",
      "no-project",
      "bad-path",
      "not-text",
      "paused",
      "waiting",
      "not-project",
    ]) {
      const text = plainLiveError({ code, message: "raw" });
      expect(text).not.toBe("raw");
      expect(text).not.toMatch(/manifest|CRDT|wildcard|`/i);
    }
    expect(plainLiveError({ code: "differs", message: "kept as is" })).toBe(
      "kept as is",
    );
  });

  it("counts files that have not reached anyone", () => {
    expect(unsyncedWarning(1)).toBe("1 file has not reached anyone else yet.");
    expect(unsyncedWarning(7)).toBe("7 files have not reached anyone else yet.");
  });

  it("never says 'by' when the session cannot tell who deleted a file", () => {
    expect(deletedSentence("")).toMatch(/^This file was deleted/);
    expect(deletedSentence("Ada")).toMatch(/^Ada deleted this file/);
    expect(deletedTag("")).toBe("deleted");
    expect(deletedTag("Ada")).toBe("deleted by Ada");
  });

  it("sizes a held change and says where the discarded copies go", () => {
    expect(pausedSentence({ files: 1, changed_lines: 3, total_lines: 9 })).toBe(
      "1 file changed at once (3 of 9 lines).",
    );
    expect(PAUSED_DISCARD_NOTE).toMatch(/trash/);
  });
});
