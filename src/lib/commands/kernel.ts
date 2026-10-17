/**
 * Kernel commands in the palette.
 *
 * Pure so it can be tested without the Svelte runtime: the palette store feeds
 * it the plugin commands and a runner, and gets back registry-shaped entries
 * that merge with the static list under "Plugin commands".
 */

import type { KernelCommand } from "$lib/kernel/client";

import type { Command } from "./registry";

/** Prefix keeping kernel ids from ever colliding with a static command id. */
export const KERNEL_ID_PREFIX = "kernel:";

export function toPaletteCommand(
  command: KernelCommand,
  run: (id: string) => void,
): Command {
  return {
    id: `${KERNEL_ID_PREFIX}${command.id}`,
    title: command.title,
    group: "Plugin commands",
    // Searchable by plugin name and raw id ("editor", "typst.render").
    keywords: `${command.plugin} ${command.id}`,
    hint: command.id,
    run: () => run(command.id),
  };
}

/**
 * Static commands first, in their authored order, then kernel commands in the
 * order the kernel listed them (it sorts by id). Duplicate kernel ids collapse.
 */
export function mergeCommands(
  statics: readonly Command[],
  kernelCommands: readonly KernelCommand[],
  run: (id: string) => void,
): Command[] {
  const seen = new Set(statics.map((c) => c.id));
  const merged = [...statics];
  for (const command of kernelCommands) {
    const entry = toPaletteCommand(command, run);
    if (seen.has(entry.id)) continue;
    seen.add(entry.id);
    merged.push(entry);
  }
  return merged;
}
