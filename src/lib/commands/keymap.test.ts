import { describe, expect, it, vi } from "vitest";

import { DEFAULT_BINDINGS } from "./defaults";
import {
  parseKey,
  resolveKey,
  shortcutsByCommand,
  toBindings,
  typesText,
  withShortcuts,
  type RegistryBinding,
} from "./keymap";
import type { Command } from "./registry";
import type { KeyLike } from "./shortcut";

const key = (k: string, mods: Partial<Omit<KeyLike, "key">> = {}): KeyLike => ({
  key: k,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  altKey: false,
  ...mods,
});

function command(
  id: string,
  extra: Partial<Command> = {},
): Command & { run: ReturnType<typeof vi.fn> } {
  return { id, title: id, group: "Workbench", run: vi.fn(), ...extra } as Command & {
    run: ReturnType<typeof vi.fn>;
  };
}

describe("parseKey", () => {
  it("reads the registry's canonical keys", () => {
    expect(parseKey("Mod+Shift+P")).toEqual({ key: "P", mod: true, shift: true });
    expect(parseKey("F5")).toEqual({ key: "F5" });
    expect(parseKey("Alt+Shift+F")).toEqual({ key: "F", alt: true, shift: true });
    expect(parseKey("Mod+\\")).toEqual({ key: "\\", mod: true });
    expect(parseKey("Mod+,")).toEqual({ key: ",", mod: true });
    expect(parseKey("Mod+PageDown")).toEqual({ key: "PageDown", mod: true });
    expect(parseKey("Ctrl+Space")).toEqual({ key: " ", mod: true });
    expect(parseKey("Mod+Plus")).toEqual({ key: "+", mod: true });
    expect(parseKey("Mod++")).toEqual({ key: "+", mod: true });
    expect(parseKey("cmd+k")).toEqual({ key: "k", mod: true });
  });

  it("leaves out chords and nonsense", () => {
    expect(parseKey("Ctrl+K Ctrl+S")).toBeNull();
    expect(parseKey("")).toBeNull();
    expect(parseKey("Hyper+K")).toBeNull();
    expect(parseKey("Mod+Shift")).toBeNull();
  });
});

describe("resolveKey", () => {
  const save = command("file.save");
  const palette = command("palette.commands");
  const commands = [save, palette, command("view.zen", { when: "document" })];
  const bindings = toBindings(DEFAULT_BINDINGS);

  it("finds the command a default binding runs", () => {
    expect(resolveKey(key("s", { ctrlKey: true }), bindings, commands)).toEqual({
      kind: "run",
      command: save,
    });
    expect(resolveKey(key("k", { metaKey: true }), bindings, commands)?.kind).toBe(
      "run",
    );
    expect(
      resolveKey(key("P", { ctrlKey: true, shiftKey: true }), bindings, commands),
    ).toEqual({
      kind: "run",
      command: palette,
    });
    expect(resolveKey(key("x"), bindings, commands)).toBeNull();
  });

  it("applies a user override: the new key runs, the old one is free", () => {
    const items: RegistryBinding[] = [
      ...DEFAULT_BINDINGS.filter((b) => b.command !== "file.save"),
      { command: "file.save", key: "Mod+Alt+S", source: "user" },
    ];
    const user = toBindings(items);
    expect(resolveKey(key("s", { ctrlKey: true }), user, commands)).toBeNull();
    expect(
      resolveKey(key("s", { ctrlKey: true, altKey: true }), user, commands),
    ).toEqual({
      kind: "run",
      command: save,
    });
  });

  it("runs the command an old id was aliased to (the registry lists the new id)", () => {
    // The user's file says `palette.open = F1`; the registry reads it as
    // `palette.commands` and lists that, so F1 opens the palette.
    const listed = toBindings([
      { command: "palette.commands", key: "F1", source: "user" },
    ]);
    expect(resolveKey(key("F1"), listed, commands)).toEqual({
      kind: "run",
      command: palette,
    });
  });

  it("prefers user bindings, then the more specific when", () => {
    const other = command("a.other");
    const items: RegistryBinding[] = [
      { command: "file.save", key: "Mod+J" },
      { command: "a.other", key: "Mod+J", source: "user" },
    ];
    expect(
      resolveKey(key("j", { ctrlKey: true }), toBindings(items), [save, other]),
    ).toEqual({
      kind: "run",
      command: other,
    });
    const scoped: RegistryBinding[] = [
      { command: "file.save", key: "Mod+J" },
      { command: "a.other", key: "Mod+J", when: "document" },
    ];
    expect(
      resolveKey(key("j", { ctrlKey: true }), toBindings(scoped), [save, other]),
    ).toEqual({
      kind: "run",
      command: other,
    });
  });

  it("reports a tie between two commands instead of picking one", () => {
    const other = command("a.other");
    const tied = toBindings([
      { command: "file.save", key: "Mod+J", source: "user" },
      { command: "a.other", key: "Mod+J", source: "user" },
    ]);
    expect(resolveKey(key("j", { ctrlKey: true }), tied, [save, other])).toEqual({
      kind: "conflict",
      commands: [save, other],
    });
  });

  it("respects the command's and the binding's when clauses", () => {
    const applies = (when: string | undefined): boolean =>
      when === undefined || when === "";
    expect(
      resolveKey(key("\\", { ctrlKey: true }), bindings, commands, applies),
    ).toEqual({
      kind: "disabled",
      command: commands[2],
    });
    const scoped = toBindings([{ command: "file.save", key: "F2", when: "figure" }]);
    expect(resolveKey(key("F2"), scoped, commands, applies)).toBeNull();
  });

  it("swallows the key of a disabled command, and never binds surface keys", () => {
    const off = command("file.save", { enabled: false });
    expect(resolveKey(key("s", { ctrlKey: true }), bindings, [off])).toEqual({
      kind: "disabled",
      command: off,
    });
    const undo = command("edit.undo", { keys: "surface" });
    const b = toBindings([{ command: "edit.undo", key: "Mod+Z" }]);
    expect(resolveKey(key("z", { ctrlKey: true }), b, [undo])).toBeNull();
  });
});

describe("shortcuts on commands", () => {
  it("come from the registry, except for surface keys", () => {
    const map = shortcutsByCommand(toBindings(DEFAULT_BINDINGS));
    expect(map.get("palette.commands")).toEqual([
      { key: "K", mod: true },
      { key: "P", mod: true, shift: true },
    ]);
    const undo = command("edit.undo", {
      keys: "surface",
      shortcuts: [{ key: "z", mod: true }],
    });
    const [save, edit] = withShortcuts(
      [command("file.save", { shortcuts: [{ key: "x" }] }), undo],
      map,
    );
    expect(save?.shortcuts).toEqual([{ key: "S", mod: true }]);
    expect(edit).toBe(undo);
  });

  it("knows which presses type text", () => {
    expect(typesText(key("a"))).toBe(true);
    expect(typesText(key("a", { ctrlKey: true }))).toBe(false);
    expect(typesText(key("F5"))).toBe(false);
  });
});
