import { describe, expect, it } from "vitest";

import { updateConsentLines } from "./market";

const none = { filesystem: [], network: [], buffer: [] };

describe("the consent lines for an update", () => {
  it("flags what is new even when everything is new", () => {
    const lines = updateConsentLines(
      { ...none, network: ["fonts.example.org"] },
      { ...none, network: ["fonts.example.org"] },
    );
    expect(lines).toEqual([{ text: "Connects to fonts.example.org", isNew: true }]);
  });

  it("lists what it keeps first, then what it adds", () => {
    const lines = updateConsentLines(
      { ...none, buffer: ["read:active"], network: ["a.org"] },
      { ...none, network: ["a.org"] },
    );
    expect(lines.map((l) => [l.text, l.isNew])).toEqual([
      ["Reads the open document", false],
      ["Connects to a.org", true],
    ]);
  });

  it("flags nothing when the update asks for nothing new", () => {
    const lines = updateConsentLines({ ...none, buffer: ["read:active"] }, none);
    expect(lines.every((l) => !l.isNew)).toBe(true);
  });
});
