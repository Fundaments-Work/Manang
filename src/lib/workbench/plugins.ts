/**
 * Plugins, derived from what the kernel actually offers.
 *
 * `kernel.commands()` lists each command with the plugin that contributes it;
 * grouping by that name gives the real plugin list. The kernel does not yet
 * report a plugin's tier or permissions, so none are shown rather than invented.
 * If it starts to (an optional `tier` / `permissions` on the command, or a
 * `kernel.plugins` command), extend `PluginGroup` here and the panel follows.
 */

import type { KernelCommand } from "$lib/kernel/client";

export interface PluginGroup {
  readonly name: string;
  readonly commands: readonly KernelCommand[];
}

export function groupPlugins(commands: readonly KernelCommand[]): PluginGroup[] {
  const byName = new Map<string, KernelCommand[]>();
  for (const command of commands) {
    const list = byName.get(command.plugin) ?? [];
    list.push(command);
    byName.set(command.plugin, list);
  }
  return [...byName.entries()]
    .map(([name, list]) => ({
      name,
      commands: [...list].sort((a, b) => a.id.localeCompare(b.id)),
    }))
    .sort((a, b) => a.name.localeCompare(b.name));
}
