/**
 * The live keymap: what the keybindings registry says is bound, kept current.
 *
 * Loaded once the workbench mounts and again on every `keybindings.changed`
 * (a change in Settings > Keyboard shortcuts, or an edit to
 * `keybindings.toml`), so a new shortcut works the moment it is saved. Until
 * the registry answers, and if it cannot, the shipped defaults apply.
 */

import { kernel, reportError } from "$lib/kernel";
import { keybindingsApi, type KeybindingConflict } from "$lib/kernel/api";

import { DEFAULT_BINDINGS } from "./defaults";
import {
  shortcutsByCommand,
  toBindings,
  type KeyBinding,
  type RegistryBinding,
} from "./keymap";
import type { Shortcut } from "./shortcut";

class KeymapState {
  items = $state<readonly RegistryBinding[]>(DEFAULT_BINDINGS);
  conflicts = $state<readonly KeybindingConflict[]>([]);
  readonly bindings: readonly KeyBinding[] = $derived(toBindings(this.items));
  readonly #byCommand: ReadonlyMap<string, readonly Shortcut[]> = $derived(
    shortcutsByCommand(this.bindings),
  );
  #started = false;

  /** A command's shortcuts, the shown one first. */
  shortcuts(command: string): readonly Shortcut[] {
    return this.#byCommand.get(command) ?? [];
  }

  /** The shortcuts by command id, for building commands. */
  get byCommand(): ReadonlyMap<string, readonly Shortcut[]> {
    return this.#byCommand;
  }

  async load(): Promise<void> {
    try {
      const listed = await keybindingsApi(kernel).list();
      this.items = listed.bindings;
      this.conflicts = listed.conflicts;
    } catch (error) {
      reportError("keyboard shortcuts could not be loaded; using the defaults", error);
    }
  }

  /** Load now and follow changes. Safe to call more than once. */
  start(): void {
    if (this.#started) return;
    this.#started = true;
    void this.load();
    kernel
      .onEvent((event) => {
        if (event.topic === "keybindings.changed") void this.load();
      })
      .catch((error: unknown) =>
        reportError("keyboard shortcut changes will not be followed", error),
      );
  }
}

export const keymap = new KeymapState();
