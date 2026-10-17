/**
 * Shortcut reference model and grouping for workbench shortcuts.
 *
 * Pure data transforms mapping registered commands and global workbench keys
 * into organized, searchable cheat-sheet groups.
 */

import type { Command } from "$lib/commands/registry";
import { formatShortcut, type Platform, type Shortcut } from "$lib/commands/shortcut";

export interface ShortcutEntry {
  readonly id: string;
  readonly title: string;
  readonly category: string;
  readonly formatted: string;
  readonly description?: string | undefined;
}

export interface ShortcutGroup {
  readonly name: string;
  readonly entries: readonly ShortcutEntry[];
}

/**
 * Built-in global shortcuts that might not be in the dynamic command registry
 * (e.g. tab numeric switching or palette accelerators).
 */
export const GLOBAL_WORKBENCH_SHORTCUTS: readonly {
  readonly id: string;
  readonly title: string;
  readonly category: string;
  readonly shortcut: Shortcut;
  readonly description?: string;
}[] = [
  {
    id: "workbench.shortcuts",
    title: "Keyboard shortcuts cheat sheet",
    category: "General",
    shortcut: { key: "/", mod: true },
    description: "Show this reference overlay",
  },
  {
    id: "workbench.palette.commands",
    title: "Command palette",
    category: "General",
    shortcut: { key: "k", mod: true },
    description: "Search and run any command",
  },
  {
    id: "workbench.palette.files",
    title: "Quick open file",
    category: "General",
    shortcut: { key: "p", mod: true },
    description: "Fuzzy find and switch files",
  },
  {
    id: "workbench.tabs.cycle",
    title: "Next tab",
    category: "Tabs & Navigation",
    shortcut: { key: "Tab", mod: true },
  },
  {
    id: "workbench.tabs.prev",
    title: "Previous tab",
    category: "Tabs & Navigation",
    shortcut: { key: "PageUp", mod: true },
  },
  {
    id: "workbench.tabs.direct",
    title: "Switch to tab 1-9",
    category: "Tabs & Navigation",
    shortcut: { key: "1-9", mod: true },
    description: "Jump directly to tab index",
  },
  {
    id: "workbench.zen.exit",
    title: "Exit Zen mode",
    category: "Views & Panels",
    shortcut: { key: "Escape" },
    description: "Restore normal workbench chrome",
  },
];

/**
 * Maps commands with defined shortcuts into organized groups for the shortcut overlay.
 */
export function extractShortcutGroups(
  commands: readonly Command[],
  platform: Platform,
): ShortcutGroup[] {
  const seen = new Set<string>();
  const entries: ShortcutEntry[] = [];

  // 1. Add global built-in shortcuts
  for (const g of GLOBAL_WORKBENCH_SHORTCUTS) {
    seen.add(g.id);
    entries.push({
      id: g.id,
      title: g.title,
      category: g.category,
      formatted: formatShortcut(g.shortcut, platform),
      description: g.description,
    });
  }

  // 2. Add commands from the registry that have shortcuts
  for (const cmd of commands) {
    if (!cmd.shortcuts || cmd.shortcuts.length === 0) continue;
    if (seen.has(cmd.id)) continue;
    seen.add(cmd.id);

    const shortcut = cmd.shortcuts[0];
    if (!shortcut) continue;

    let category = "General";
    if (cmd.id.startsWith("view.") || cmd.id.startsWith("toggle.")) {
      category = "Views & Panels";
    } else if (
      cmd.id.startsWith("file.") ||
      cmd.id.startsWith("doc.") ||
      cmd.id.startsWith("project.")
    ) {
      category = "Document & Files";
    } else if (cmd.id.startsWith("tab.")) {
      category = "Tabs & Navigation";
    } else if (cmd.group) {
      category = cmd.group;
    }

    entries.push({
      id: cmd.id,
      title: cmd.title,
      category,
      formatted: formatShortcut(shortcut, platform),
    });
  }

  // Group by category with stable ordering
  const order = ["General", "Document & Files", "Views & Panels", "Tabs & Navigation"];
  const grouped = new Map<string, ShortcutEntry[]>();

  for (const entry of entries) {
    const list = grouped.get(entry.category) ?? [];
    list.push(entry);
    grouped.set(entry.category, list);
  }

  const result: ShortcutGroup[] = [];

  for (const name of order) {
    const list = grouped.get(name);
    if (list && list.length > 0) {
      result.push({ name, entries: list });
      grouped.delete(name);
    }
  }

  // Any remaining categories
  for (const [name, list] of grouped) {
    if (list.length > 0) {
      result.push({ name, entries: list });
    }
  }

  return result;
}

/**
 * Filters shortcut groups matching a query across title, shortcut keys, or category.
 */
export function filterShortcutGroups(
  groups: readonly ShortcutGroup[],
  query: string,
): ShortcutGroup[] {
  const trimmed = query.trim().toLowerCase();
  if (!trimmed) return groups as ShortcutGroup[];

  const result: ShortcutGroup[] = [];

  for (const group of groups) {
    const matchesGroup = group.name.toLowerCase().includes(trimmed);
    const filteredEntries = group.entries.filter(
      (e) =>
        matchesGroup ||
        e.title.toLowerCase().includes(trimmed) ||
        e.formatted.toLowerCase().includes(trimmed) ||
        (e.description && e.description.toLowerCase().includes(trimmed)),
    );

    if (filteredEntries.length > 0) {
      result.push({
        name: group.name,
        entries: filteredEntries,
      });
    }
  }

  return result;
}
