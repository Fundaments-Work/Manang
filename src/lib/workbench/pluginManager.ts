/**
 * The Plugins view's rules, free of Svelte and the kernel so they can be unit
 * tested: how a plugin is labelled, which filter it belongs to, what the
 * search field matches, and how permission sets compare (an update that asks
 * for more than was approved is shown with the new access highlighted).
 */

import type { PluginAccess, PluginState } from "$lib/kernel/pluginsApi";

import { describeAccess } from "./plugins";

export const EMPTY_ACCESS: PluginAccess = { filesystem: [], network: [], buffer: [] };

const lower = (list: readonly string[], network: boolean): string[] =>
  network ? list.map((s) => s.toLowerCase()) : [...list];

const within = (
  a: readonly string[],
  b: readonly string[],
  network: boolean,
): boolean => {
  const set = new Set(lower(b, network));
  return lower(a, network).every((s) => set.has(s));
};

const without = (a: readonly string[], b: readonly string[], network: boolean) => {
  const set = new Set(lower(b, network));
  const seen = new Set<string>();
  return a.filter((s) => {
    const key = network ? s.toLowerCase() : s;
    if (set.has(key) || seen.has(key)) return false;
    seen.add(key);
    return true;
  });
};

/** Whether everything in `a` is also in `b` (network hosts compare case-insensitively). */
export function accessWithin(a: PluginAccess, b: PluginAccess): boolean {
  return (
    within(a.filesystem, b.filesystem, false) &&
    within(a.network, b.network, true) &&
    within(a.buffer, b.buffer, false)
  );
}

/** What `a` asks for beyond `b`: the new access an update must be approved for. */
export function accessDifference(a: PluginAccess, b: PluginAccess): PluginAccess {
  return {
    filesystem: without(a.filesystem, b.filesystem, false),
    network: without(a.network, b.network, true),
    buffer: without(a.buffer, b.buffer, false),
  };
}

export function accessIsEmpty(a: PluginAccess): boolean {
  return a.filesystem.length + a.network.length + a.buffer.length === 0;
}

/** One line of the consent dialog and the detail pane. */
export interface AccessLine {
  readonly text: string;
  /** Not approved before: an update asks for it for the first time. */
  readonly isNew: boolean;
}

/**
 * The plain-language lines for `requested`, those in `added` flagged as new
 * and listed last. With nothing flagged (a fresh install) every line is plain:
 * there is nothing to compare with.
 */
export function accessLines(
  requested: PluginAccess,
  added?: PluginAccess,
): AccessLine[] {
  // Nothing to compare with, or everything is new (a first install).
  if (added === undefined || accessIsEmpty(added) || accessWithin(requested, added)) {
    return describeAccess(requested).map((text) => ({ text, isNew: false }));
  }
  const kept = accessDifference(requested, added);
  return [
    ...describeAccess(kept).map((text) => ({ text, isNew: false })),
    ...describeAccess(added).map((text) => ({ text, isNew: true })),
  ];
}

/** "Built in" for first-party plugins, "Sandboxed" for installed WebAssembly ones. */
export function tierLabel(plugin: Pick<PluginState, "tier">): string {
  return plugin.tier === "native" ? "Built in" : "Sandboxed";
}

export type PluginStateLabel = "Enabled" | "Disabled" | "Needs approval" | "Failed";

export function stateLabel(plugin: Pick<PluginState, "status">): PluginStateLabel {
  switch (plugin.status) {
    case "loaded":
      return "Enabled";
    case "disabled":
      return "Disabled";
    case "needs_approval":
      return "Needs approval";
    case "failed":
      return "Failed";
  }
}

/** Failed or waiting for consent: what the "Needs attention" filter lists. */
export function needsAttention(plugin: Pick<PluginState, "status">): boolean {
  return plugin.status === "failed" || plugin.status === "needs_approval";
}

export type PluginFilter = "all" | "installed" | "builtin" | "attention";

export const PLUGIN_FILTERS: readonly {
  readonly id: PluginFilter;
  readonly label: string;
}[] = [
  { id: "all", label: "All" },
  { id: "installed", label: "Installed" },
  { id: "builtin", label: "Built in" },
  { id: "attention", label: "Needs attention" },
];

export function matchesFilter(plugin: PluginState, filter: PluginFilter): boolean {
  switch (filter) {
    case "all":
      return true;
    case "installed":
      return plugin.tier === "wasm";
    case "builtin":
      return plugin.tier === "native";
    case "attention":
      return needsAttention(plugin);
  }
}

/** The search field matches the name, description, publisher and command ids. */
export function matchesQuery(plugin: PluginState, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === "") return true;
  const haystack = [
    plugin.name,
    plugin.description,
    plugin.publisher ?? "",
    ...plugin.commands.flatMap((c) => [c.id, c.title]),
  ];
  return haystack.some((s) => s.toLowerCase().includes(q));
}

/**
 * The plugins to show: filtered, then those needing attention first, then by
 * name (a broken plugin should not hide at the bottom of a long list).
 */
export function visiblePlugins(
  plugins: readonly PluginState[],
  filter: PluginFilter,
  query: string,
): PluginState[] {
  return plugins
    .filter((p) => matchesFilter(p, filter) && matchesQuery(p, query))
    .sort(
      (a, b) =>
        Number(needsAttention(b)) - Number(needsAttention(a)) ||
        a.name.localeCompare(b.name),
    );
}

/** How many plugins each filter would list (for the chips). */
export function filterCounts(
  plugins: readonly PluginState[],
): Record<PluginFilter, number> {
  return {
    all: plugins.length,
    installed: plugins.filter((p) => matchesFilter(p, "installed")).length,
    builtin: plugins.filter((p) => matchesFilter(p, "builtin")).length,
    attention: plugins.filter(needsAttention).length,
  };
}

/** Whether the enable/disable toggle applies: not for the core of the app. */
export function canToggle(plugin: Pick<PluginState, "core">): boolean {
  return !plugin.core;
}

/** Whether the plugin can be uninstalled: installed ones only, never built-ins. */
export function canUninstall(plugin: Pick<PluginState, "core" | "tier">): boolean {
  return !plugin.core && plugin.tier === "wasm";
}

/** The sentence under a disabled plugin's name. */
export function disabledReason(plugin: Pick<PluginState, "disabled_by">): string {
  return plugin.disabled_by === "project"
    ? "Switched off by this project"
    : "Switched off by you";
}

/** `2026-10-18` for a Unix timestamp in seconds. */
export function installedDate(seconds: number): string {
  return new Date(seconds * 1000).toISOString().slice(0, 10);
}

/** The plugin with `name` replaced by `next`, or appended; sorted by name. */
export function withPlugin(
  plugins: readonly PluginState[],
  next: PluginState,
): PluginState[] {
  const rest = plugins.filter((p) => p.name !== next.name);
  return [...rest, next].sort((a, b) => a.name.localeCompare(b.name));
}
