import { describe, expect, it } from "vitest";

import {
  canShareLive,
  connectionDetail,
  connectionLabel,
  initials,
  inviteFileLabel,
  liveChipText,
  participantSummary,
  peopleCount,
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
