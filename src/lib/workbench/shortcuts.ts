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
 * Keys that are not commands in the keybindings registry, listed so the
 * reference is complete: Escape dismisses whatever is on top (it is handled
 * where the thing it closes lives), and Go to tab 1-9 is nine commands
 * summarised as one row.
 */
export const GLOBAL_WORKBENCH_SHORTCUTS: readonly {
  readonly id: string;
  readonly title: string;
  readonly category: string;
  readonly shortcut: Shortcut;
  readonly description?: string;
}[] = [
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
    // Keys-only commands (Go to tab 3) are summarised by a global row.
    if (cmd.palette === false) continue;
    if (seen.has(cmd.id)) continue;
    seen.add(cmd.id);

    const shortcut = cmd.shortcuts[0];
    if (!shortcut) continue;

    let category = "General";
    if (
      cmd.id.startsWith("palette.") ||
      cmd.id.startsWith("help.") ||
      cmd.id.startsWith("app.") ||
      cmd.id.startsWith("dev.")
    ) {
      category = "General";
    } else if (cmd.id.startsWith("view.") || cmd.id.startsWith("toggle.")) {
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
