/**
 * Plugins, derived from what the kernel actually offers.
 *
 * `kernel.commands()` lists each command with the plugin that contributes it;
 * grouping by that name gives the real plugin list. `kernel_plugins` adds
 * each manifest's version, description, tier, permissions and settings.
 */

import type {
  Kernel,
  KernelCommand,
  PluginInfo,
  PluginPermissions,
} from "$lib/kernel/client";

export type { PluginInfo, PluginPermissions };

/** The manifest summaries, or null when this backend cannot list them. */
export async function listPlugins(kernel: Kernel): Promise<PluginInfo[] | null> {
  if (kernel.plugins === undefined) return null;
  try {
    return await kernel.plugins();
  } catch {
    return null;
  }
}

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

/** What the Plugins view shows for one plugin. */
export interface PluginEntry {
  readonly name: string;
  readonly version: string;
  readonly description: string;
  readonly tier: "native" | "wasm" | null;
  readonly access: readonly string[];
  readonly settings: readonly string[];
  readonly commands: readonly KernelCommand[];
}

const SCOPE_WORDS: Record<string, string> = {
  project: "project files",
  config: "its settings folder",
  cache: "its cache",
  active: "the open document",
};

/**
 * Plain sentences for a manifest's permissions, e.g. "Reads project files",
 * "Connects to api.crossref.org". Unknown entries are shown verbatim rather
 * than dropped: a permission the user cannot see is worse than an awkward one.
 */
export function describeAccess(permissions: PluginPermissions): string[] {
  const out: string[] = [];
  const fs = permissions.filesystem ?? [];
  const writes = new Set<string>();
  const reads = new Set<string>();
  for (const entry of [...fs, ...(permissions.buffer ?? [])]) {
    const [mode, scope] = entry.split(":");
    const words = scope !== undefined ? SCOPE_WORDS[scope] : undefined;
    if (words === undefined || (mode !== "read" && mode !== "write")) {
      out.push(entry);
      continue;
    }
    (mode === "write" ? writes : reads).add(words);
  }
  for (const words of writes) out.push(`Reads and changes ${words}`);
  for (const words of reads) if (!writes.has(words)) out.push(`Reads ${words}`);
  const hosts = permissions.network ?? [];
  if (hosts.length > 0) out.push(`Connects to ${hosts.join(", ")}`);
  return out;
}

/**
 * Join the manifest summaries with the live command list. Without summaries
 * (a shell that predates `kernel_plugins`) the list falls back to grouping
 * the commands, showing no tier or access rather than inventing them.
 */
export function pluginEntries(
  info: readonly PluginInfo[] | null,
  commands: readonly KernelCommand[],
): PluginEntry[] {
  const groups = new Map(groupPlugins(commands).map((g) => [g.name, g.commands]));
  if (info === null) {
    return [...groups.entries()].map(([name, list]) => ({
      name,
      version: "",
      description: "",
      tier: null,
      access: [],
      settings: [],
      commands: list,
    }));
  }
  return [...info]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((p) => ({
      name: p.name,
      version: p.version,
      description: p.description,
      tier: p.tier,
      access: describeAccess(p.permissions),
      settings: p.settings,
      commands: groups.get(p.name) ?? [],
    }));
}
