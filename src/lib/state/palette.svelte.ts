/**
 * Command palette state.
 *
 * Keyboard-first, a shortcut rather than the main way in. The result list is
 * derived from the query, and the cursor is clamped to it, so typing can never
 * leave the highlight on a row that has been filtered out.
 */

import {
  flatten,
  groupRanked,
  paletteCandidates,
  rankCandidates,
  type PaletteGroup,
  type PaletteMode,
} from "$lib/commands/palette";
import type { Command } from "$lib/commands/registry";
import { workbenchCommands } from "$lib/commands/workbench";
import { kernel, type KernelCommand } from "$lib/kernel";

import { openFile, runKernelCommand } from "./actions";
import { workspace } from "./workspace.svelte";

export class PaletteState {
  open = $state(false);
  mode = $state<PaletteMode>("commands");
  query = $state("");
  cursor = $state(0);
  kernelCommands = $state.raw<readonly KernelCommand[]>([]);
  workbench = $state.raw<readonly Command[]>([]);

  readonly candidates: Command[] = $derived(
    paletteCandidates({
      mode: this.mode,
      query: this.query,
      workbench: this.workbench,
      files: workspace.paths,
      kernel: this.kernelCommands,
      runKernel: (id) => void runKernelCommand(id),
      openFile: (path) => void openFile(path),
    }),
  );
  readonly groups: PaletteGroup[] = $derived(
    groupRanked(rankCandidates(this.candidates, this.query)),
  );
  readonly flat: Command[] = $derived(flatten(this.groups));
  readonly selected: Command | undefined = $derived(
    this.flat[Math.min(this.cursor, Math.max(0, this.flat.length - 1))],
  );

  show(mode: PaletteMode = "commands"): void {
    this.mode = mode;
    this.query = "";
    this.cursor = 0;
    this.workbench = workbenchCommands((m) => this.show(m));
    this.open = true;
    void workspace.ensureAll();
    void this.#loadKernel();
  }

  async #loadKernel(): Promise<void> {
    if (this.kernelCommands.length > 0) return;
    try {
      this.kernelCommands = await kernel.commands();
    } catch {
      this.kernelCommands = [];
    }
  }

  hide(): void {
    this.open = false;
    this.query = "";
    this.cursor = 0;
  }

  toggle(): void {
    if (this.open) this.hide();
    else this.show();
  }

  setQuery(next: string): void {
    this.query = next;
    this.cursor = 0;
  }

  move(delta: number): void {
    const max = this.flat.length - 1;
    if (max < 0) return;
    this.cursor = Math.max(0, Math.min(this.cursor + delta, max));
  }

  /** Run the selected command and close, unless it is disabled. */
  execute(command: Command | undefined = this.selected): void {
    if (command === undefined || command.enabled === false) return;
    this.hide();
    command.run();
  }
}

export const palette = new PaletteState();
