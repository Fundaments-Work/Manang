import { readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";

import { describe, expect, it } from "vitest";

import { DEFAULT_BINDINGS } from "./defaults";
import { parseKey } from "./keymap";

/** `CORE_DEFAULTS` from the Rust keybindings registry, as (command, key) rows. */
function rustDefaults(): { command: string; key: string; when: string }[] {
  const path = fileURLToPath(
    new URL("../../../crates/manang-settings/src/keybindings.rs", import.meta.url),
  );
  const source = readFileSync(path, "utf8");
  const start = source.indexOf("pub const CORE_DEFAULTS");
  const end = source.indexOf("];", start);
  const table = source.slice(start, end);
  const rows = [
    ...table.matchAll(/\("([^"]+)",\s*"((?:[^"\\]|\\.)*)",\s*"([^"]*)"\)/g),
  ];
  return rows.map(([, command = "", key = "", when = ""]) => ({
    command,
    // Rust string escapes: only `\\` appears in the table.
    key: key.replace(/\\\\/g, "\\"),
    when,
  }));
}

describe("default keybindings", () => {
  it("match the registry's CORE_DEFAULTS exactly", () => {
    const rust = rustDefaults();
    expect(rust.length).toBeGreaterThan(20);
    expect(rust.every((r) => r.when === "")).toBe(true);
    expect(DEFAULT_BINDINGS.map((b) => `${b.command} ${b.key}`)).toEqual(
      rust.map((r) => `${r.command} ${r.key}`),
    );
  });

  it("are all readable as single-stroke shortcuts", () => {
    for (const b of DEFAULT_BINDINGS) expect(parseKey(b.key), b.key).not.toBeNull();
  });

  it("never put two commands on one key", () => {
    const keys = DEFAULT_BINDINGS.map((b) => b.key.toLowerCase());
    expect(new Set(keys).size).toBe(keys.length);
  });
});
