/**
 * Keyboard shortcuts from the keybindings registry, as pure functions.
 *
 * The registry (the settings plugin's `keybindings.list`) holds every binding
 * in force: the defaults and the person's overrides, by command id. The global
 * key handler and the menus read it through these functions, so a shortcut
 * changed in Settings is the one that runs and the one shown.
 *
 * Precedence follows the registry's: a user binding beats a default, then the
 * binding with the more specific `when` wins; a tie between two commands runs
 * neither and is reported as a conflict.
 */

import type { Command } from "./registry";
import { matches, type KeyLike, type Shortcut } from "./shortcut";

/** One binding as the registry lists it (`KeybindingItem` minus display). */
export interface RegistryBinding {
  readonly command: string;
  readonly key: string;
  readonly when?: string | undefined;
  readonly source?: "default" | "user" | undefined;
}

export interface KeyBinding {
  readonly command: string;
  readonly shortcut: Shortcut;
  readonly when: string;
  readonly source: "default" | "user";
}

const MODIFIERS: Readonly<Record<string, "mod" | "alt" | "shift">> = {
  mod: "mod",
  primary: "mod",
  ctrl: "mod",
  control: "mod",
  meta: "mod",
  cmd: "mod",
  command: "mod",
  super: "mod",
  alt: "alt",
  option: "alt",
  opt: "alt",
  shift: "shift",
};

/** Registry key names that differ from `KeyboardEvent.key`. */
const KEY_NAMES: Readonly<Record<string, string>> = {
  space: " ",
  plus: "+",
  esc: "Escape",
  del: "Delete",
  up: "ArrowUp",
  down: "ArrowDown",
  left: "ArrowLeft",
  right: "ArrowRight",
};

/**
 * A registry key (`Mod+Shift+P`, `F5`, `Alt+Shift+F`) as a `Shortcut`, or
 * null for a chord (`Ctrl+K Ctrl+S`) or something unreadable. The shell has
 * one modifier for Ctrl/Cmd, so `Ctrl`, `Meta` and `Mod` all read as `mod`.
 */
export function parseKey(text: string): Shortcut | null {
  const trimmed = text.trim();
  if (trimmed === "" || /\s/.test(trimmed)) return null;
  // A trailing "+" is the key itself (`Mod++`), not a separator.
  const parts = trimmed.endsWith("++")
    ? [...trimmed.slice(0, -2).split("+"), "+"]
    : trimmed.split("+");
  const raw = parts.pop();
  if (raw === undefined || raw === "") return null;
  let mod = false;
  let alt = false;
  let shift = false;
  for (const part of parts) {
    const which = MODIFIERS[part.toLowerCase()];
    if (which === undefined) return null;
    if (which === "mod") mod = true;
    else if (which === "alt") alt = true;
    else shift = true;
  }
  if (MODIFIERS[raw.toLowerCase()] !== undefined) return null;
  const key = KEY_NAMES[raw.toLowerCase()] ?? raw;
  return {
    key,
    ...(mod ? { mod } : {}),
    ...(alt ? { alt } : {}),
    ...(shift ? { shift } : {}),
  };
}

/** The registry's list as shortcuts; chords and unreadable keys are left out. */
export function toBindings(items: readonly RegistryBinding[]): KeyBinding[] {
  const out: KeyBinding[] = [];
  for (const item of items) {
    const shortcut = parseKey(item.key);
    if (shortcut === null) continue;
    out.push({
      command: item.command,
      shortcut,
      when: item.when ?? "",
      source: item.source ?? "default",
    });
  }
  return out;
}

/** Each command's shortcuts, in listed order (the first is the one shown). */
export function shortcutsByCommand(
  bindings: readonly KeyBinding[],
): ReadonlyMap<string, readonly Shortcut[]> {
  const map = new Map<string, Shortcut[]>();
  for (const b of bindings) {
    const list = map.get(b.command);
    if (list === undefined) map.set(b.command, [b.shortcut]);
    else list.push(b.shortcut);
  }
  return map;
}

/** Number of clauses in a `when`; more is more specific. */
function specificity(when: string): number {
  return when.trim() === "" ? 0 : when.split(/&&|\|\|/).length;
}

export type KeyResolution =
  | { readonly kind: "run"; readonly command: Command }
  /** The key belongs to a command that does not apply right now (disabled). */
  | { readonly kind: "disabled"; readonly command: Command }
  | { readonly kind: "conflict"; readonly commands: readonly Command[] };

/**
 * What a key press does: the command it runs, a disabled command it belongs
 * to (so the press is swallowed rather than reaching the browser), a conflict
 * between commands, or null when no binding applies.
 *
 * Commands whose keys belong to the focused surface (`keys: "surface"`) are
 * never run from here, and a binding applies only when both its own `when`
 * and its command's `when` hold.
 */
export function resolveKey(
  event: KeyLike,
  bindings: readonly KeyBinding[],
  commands: readonly Command[],
  applies: (when: string | undefined) => boolean = () => true,
): KeyResolution | null {
  const byId = new Map(commands.map((c) => [c.id, c]));
  const live: { binding: KeyBinding; command: Command }[] = [];
  let disabled: Command | undefined;
  for (const binding of bindings) {
    if (!matches(event, binding.shortcut)) continue;
    const command = byId.get(binding.command);
    if (command === undefined || command.keys === "surface") continue;
    if (!applies(binding.when)) continue;
    if (command.enabled === false || !applies(command.when)) {
      disabled ??= command;
      continue;
    }
    live.push({ binding, command });
  }
  if (live.length === 0)
    return disabled === undefined ? null : { kind: "disabled", command: disabled };
  const rank = (b: KeyBinding): number =>
    (b.source === "user" ? 1000 : 0) + specificity(b.when);
  const top = Math.max(...live.map((l) => rank(l.binding)));
  const winners: Command[] = [];
  for (const l of live)
    if (rank(l.binding) === top && !winners.includes(l.command))
      winners.push(l.command);
  const [only] = winners;
  if (winners.length === 1 && only !== undefined) return { kind: "run", command: only };
  return { kind: "conflict", commands: winners };
}

/**
 * Commands with their shortcuts taken from the registry. Commands whose keys
 * belong to the focused surface keep the shortcuts they declare.
 */
export function withShortcuts(
  commands: readonly Command[],
  shortcuts: ReadonlyMap<string, readonly Shortcut[]>,
): Command[] {
  return commands.map((command) =>
    command.keys === "surface"
      ? command
      : { ...command, shortcuts: shortcuts.get(command.id) ?? [] },
  );
}

/** Whether a key press would type text into a field (a bare letter, digit or space). */
export function typesText(event: KeyLike): boolean {
  return !event.ctrlKey && !event.metaKey && !event.altKey && event.key.length === 1;
}
