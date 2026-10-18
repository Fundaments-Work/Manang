import { describe, expect, it } from "vitest";

import type { KeybindingItem } from "$lib/kernel/api";

import { keybindingRows } from "./keybindingRows";

const binding = (
  command: string,
  key: string,
  source: "default" | "user" = "default",
) => ({ command, key, display: key, when: "", source }) satisfies KeybindingItem;

describe("keybindingRows", () => {
  it("keeps every binding of a command, each with its own id", () => {
    const rows = keybindingRows([
      binding("palette.commands", "ctrl+k"),
      binding("palette.commands", "ctrl+shift+p"),
    ]);
    expect(rows).toHaveLength(2);
    expect(new Set(rows.map((r) => r.id)).size).toBe(2);
  });

  it("shows an exact duplicate once", () => {
    const twice = binding("file.save", "ctrl+s");
    expect(keybindingRows([twice, twice])).toHaveLength(1);
  });

  it("tells a user override from the default on the same key", () => {
    const rows = keybindingRows([
      binding("file.save", "ctrl+s"),
      binding("file.save", "ctrl+s", "user"),
    ]);
    expect(rows).toHaveLength(2);
  });
});
