import { describe, expect, it } from "vitest";

import type { Command } from "$lib/commands/registry";
import type { Platform } from "$lib/commands/shortcut";

import {
  extractShortcutGroups,
  filterShortcutGroups,
  GLOBAL_WORKBENCH_SHORTCUTS,
} from "./shortcuts";

describe("workbench shortcuts model", () => {
  const linuxPlatform: Platform = { isMac: false };
  const macPlatform: Platform = { isMac: true };

  const mockCommands: readonly Command[] = [
    {
      id: "view.files",
      title: "Show files",
      group: "Workbench",
      shortcuts: [{ key: "e", mod: true, shift: true }],
      run: () => {},
    },
    {
      id: "file.save",
      title: "Save document",
      group: "Workbench",
      shortcuts: [{ key: "s", mod: true }],
      run: () => {},
    },
    {
      id: "tab.close",
      title: "Close tab",
      group: "Workbench",
      shortcuts: [{ key: "w", mod: true }],
      run: () => {},
    },
    {
      id: "no.shortcut",
      title: "Command without shortcut",
      group: "Workbench",
      run: () => {},
    },
  ];

  it("extracts and formats shortcuts across categories on Linux", () => {
    const groups = extractShortcutGroups(mockCommands, linuxPlatform);

    // Verify categories present
    const categoryNames = groups.map((g) => g.name);
    expect(categoryNames).toContain("General");
    expect(categoryNames).toContain("Document & Files");
    expect(categoryNames).toContain("Views & Panels");
    expect(categoryNames).toContain("Tabs & Navigation");

    // Verify formatting
    const general = groups.find((g) => g.name === "General");
    expect(general?.entries.find((e) => e.id === "workbench.shortcuts")?.formatted).toBe("Ctrl+/");

    const views = groups.find((g) => g.name === "Views & Panels");
    expect(views?.entries.find((e) => e.id === "view.files")?.formatted).toBe("Ctrl+Shift+E");

    const docs = groups.find((g) => g.name === "Document & Files");
    expect(docs?.entries.find((e) => e.id === "file.save")?.formatted).toBe("Ctrl+S");
  });

  it("formats modifier symbols on macOS", () => {
    const groups = extractShortcutGroups(mockCommands, macPlatform);
    const general = groups.find((g) => g.name === "General");
    expect(general?.entries.find((e) => e.id === "workbench.shortcuts")?.formatted).toBe("⌘/");

    const views = groups.find((g) => g.name === "Views & Panels");
    expect(views?.entries.find((e) => e.id === "view.files")?.formatted).toBe("⇧⌘E");
  });

  it("filters shortcuts by command title, keys, or category", () => {
    const groups = extractShortcutGroups(mockCommands, linuxPlatform);

    // Search by title
    const searchByTitle = filterShortcutGroups(groups, "save");
    expect(searchByTitle).toHaveLength(1);
    expect(searchByTitle[0]?.entries[0]?.title).toBe("Save document");

    // Search by key
    const searchByKey = filterShortcutGroups(groups, "shift+e");
    expect(searchByKey).toHaveLength(1);
    expect(searchByKey[0]?.entries[0]?.title).toBe("Show files");

    // Search by category
    const searchByCat = filterShortcutGroups(groups, "tabs");
    expect(searchByCat.length).toBeGreaterThanOrEqual(1);

    // Non-matching query returns empty
    const nonMatching = filterShortcutGroups(groups, "xyz-unmatched");
    expect(nonMatching).toHaveLength(0);
  });

  it("includes all global shortcuts without duplicates", () => {
    const groups = extractShortcutGroups([], linuxPlatform);
    const totalEntries = groups.reduce((acc, g) => acc + g.entries.length, 0);
    expect(totalEntries).toBe(GLOBAL_WORKBENCH_SHORTCUTS.length);
  });
});
