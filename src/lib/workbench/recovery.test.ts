import { describe, expect, it } from "vitest";

import type { RecoveryEntry } from "$lib/kernel/api";

import { latest, recoveryMessage, recoveryTitle, whenLabel } from "./recovery";

function entry(
  path: string,
  disk: RecoveryEntry["disk"] = "match",
  at = 1000,
): RecoveryEntry {
  return { kind: "text", path, saved_at: at, bytes: 10, disk };
}

const NOW = new Date(2026, 9, 18, 15, 0).getTime();

describe("recovery wording", () => {
  it("names the time of the newest snapshot and the file count", () => {
    const at = new Date(2026, 9, 18, 14, 32).getTime();
    const title = recoveryTitle(
      [entry("a.typ", "match", at - 5000), entry("b.typ", "match", at)],
      NOW,
    );
    expect(title).toMatch(/^Recover unsaved changes from .*32.* in 2 files\?$/);
    expect(recoveryTitle([entry("a.typ", "match", at)], NOW)).toContain("in 1 file?");
  });

  it("adds the date for another day", () => {
    const old = new Date(2026, 9, 12, 9, 5).getTime();
    expect(whenLabel(old, NOW)).toContain("12");
    expect(whenLabel(NOW - 60_000, NOW)).not.toContain(",");
  });

  it("finds the newest snapshot", () => {
    expect(latest([entry("a", "match", 5), entry("b", "match", 9)])).toBe(9);
    expect(latest([])).toBe(0);
  });

  it("says nothing on disk is replaced, lists a few names, flags changed and missing", () => {
    const text = recoveryMessage([
      entry("src/a.typ", "changed"),
      entry("b.typ", "missing"),
      entry("c.typ"),
      entry("d.typ"),
      entry("e.typ"),
      entry("f.typ"),
    ]);
    expect(text).toContain("a.typ, b.typ, c.typ, d.typ and 2 more");
    expect(text).toContain("nothing on disk is replaced until you save");
    expect(text).toContain("One file has also changed on disk");
    expect(text).toContain("One file is not on disk");
  });

  it("stays short when every file is as left", () => {
    expect(recoveryMessage([entry("a.typ")])).not.toContain("changed on disk");
  });
});
