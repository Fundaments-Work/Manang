/**
 * Pure settings and keybindings workbench logic.
 */

import type { KeybindingConflict, SettingDef } from "$lib/kernel/api";

export interface SettingsGroup {
  readonly id: string;
  readonly label: string;
  readonly settings: readonly SettingDef[];
}

const CATEGORY_ORDER: readonly string[] = [
  "editor",
  "preview",
  "compile",
  "files",
  "appearance",
];

const CATEGORY_LABELS: Record<string, string> = {
  editor: "Editor",
  preview: "Preview",
  compile: "Compile",
  files: "Files",
  appearance: "Appearance",
  git: "Git",
  search: "Search",
  typst: "Typst",
  citations: "Citations",
  markdown: "Markdown",
  figure: "Figure",
};

/**
 * Capitalizes kebab-case identifier into Title Case.
 * e.g. "font-size" -> "Font Size", "editor.font-size" -> "Font Size".
 */
export function formatSettingTitle(id: string): string {
  const leaf = id.includes(".") ? id.split(".").slice(1).join(" ") : id;
  return leaf
    .split(/[-_ ]+/)
    .filter((w) => w.length > 0)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
    .join(" ");
}

/**
 * Returns human-readable category name.
 */
export function formatCategoryLabel(category: string): string {
  if (category in CATEGORY_LABELS) {
    return CATEGORY_LABELS[category]!;
  }
  return category.charAt(0).toUpperCase() + category.slice(1);
}

/**
 * Groups settings by id prefix / category according to design order:
 * editor, preview, compile, files, appearance, then plugins alphabetically.
 */
export function groupSettings(settings: readonly SettingDef[]): SettingsGroup[] {
  const map = new Map<string, SettingDef[]>();

  for (const s of settings) {
    const cat = s.category || s.id.split(".")[0] || "general";
    const list = map.get(cat) ?? [];
    list.push(s);
    map.set(cat, list);
  }

  const allCategories = [...map.keys()];

  // Sort categories: known order first, then alphabetical
  allCategories.sort((a, b) => {
    const idxA = CATEGORY_ORDER.indexOf(a);
    const idxB = CATEGORY_ORDER.indexOf(b);
    if (idxA !== -1 && idxB !== -1) return idxA - idxB;
    if (idxA !== -1) return -1;
    if (idxB !== -1) return 1;
    return a.localeCompare(b);
  });

  return allCategories.map((cat) => ({
    id: cat,
    label: formatCategoryLabel(cat),
    settings: (map.get(cat) ?? []).sort((a, b) => a.id.localeCompare(b.id)),
  }));
}

/**
 * Filters settings by search query across id, title, and description.
 */
export function filterSettings(
  settings: readonly SettingDef[],
  query: string,
): SettingDef[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [...settings];

  return settings.filter((s) => {
    const title = formatSettingTitle(s.id).toLowerCase();
    const id = s.id.toLowerCase();
    const desc = s.description.toLowerCase();
    const cat = s.category.toLowerCase();
    return id.includes(q) || title.includes(q) || desc.includes(q) || cat.includes(q);
  });
}

/**
 * Determines whether a setting is modified from its default.
 */
export function isSettingModified(
  value: unknown,
  defaultValue: unknown,
  source?: string,
): boolean {
  if (source !== undefined && source !== "default") {
    return true;
  }
  return JSON.stringify(value) !== JSON.stringify(defaultValue);
}

export interface ValidationResult {
  readonly valid: boolean;
  readonly error?: string;
}

/**
 * Validates candidate setting value against definition.
 */
export function validateSettingValue(
  def: SettingDef,
  value: unknown,
): ValidationResult {
  switch (def.type) {
    case "bool":
      if (typeof value !== "boolean") {
        return { valid: false, error: "Must be a boolean (true or false)" };
      }
      return { valid: true };

    case "int": {
      if (typeof value !== "number" || !Number.isInteger(value)) {
        return { valid: false, error: "Must be an integer" };
      }
      if (def.min !== undefined && value < def.min) {
        return { valid: false, error: `Minimum value is ${def.min}` };
      }
      if (def.max !== undefined && value > def.max) {
        return { valid: false, error: `Maximum value is ${def.max}` };
      }
      return { valid: true };
    }

    case "float": {
      if (typeof value !== "number" || !Number.isFinite(value)) {
        return { valid: false, error: "Must be a valid number" };
      }
      if (def.min !== undefined && value < def.min) {
        return { valid: false, error: `Minimum value is ${def.min}` };
      }
      if (def.max !== undefined && value > def.max) {
        return { valid: false, error: `Maximum value is ${def.max}` };
      }
      return { valid: true };
    }

    case "enum": {
      if (typeof value !== "string" || !(def.values ?? []).includes(value)) {
        return {
          valid: false,
          error: `Must be one of: ${(def.values ?? []).join(", ")}`,
        };
      }
      return { valid: true };
    }

    case "string":
    case "path": {
      if (typeof value !== "string") {
        return { valid: false, error: "Must be text" };
      }
      if (def.type === "path" && value.includes("\0")) {
        return { valid: false, error: "Path cannot contain null bytes" };
      }
      return { valid: true };
    }

    case "list": {
      if (!Array.isArray(value)) {
        return { valid: false, error: "Must be a list" };
      }
      return { valid: true };
    }

    default:
      return { valid: true };
  }
}

/**
 * Converts a KeyboardEvent into canonical shortcut string format (e.g. `Mod+Shift+P`).
 * Returns null if only modifier keys were pressed.
 */
export function recordShortcutFromEvent(
  event: KeyboardEvent,
  platform = "linux",
): string | null {
  const key = event.key;
  if (
    key === "Control" ||
    key === "Shift" ||
    key === "Alt" ||
    key === "Meta"
  ) {
    return null;
  }

  const parts: string[] = [];

  // Use Mod+ to represent Command on macOS and Ctrl on Linux/Windows
  const isMac = platform === "macos" || platform === "darwin";
  const modPressed = isMac ? event.metaKey : event.ctrlKey;
  const secondaryCtrl = isMac && event.ctrlKey;

  if (modPressed) parts.push("Mod");
  if (secondaryCtrl) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");

  // Format the key
  let keyName = key;
  if (key === " ") keyName = "Space";
  else if (key.length === 1) keyName = key.toUpperCase();

  parts.push(keyName);
  return parts.join("+");
}

/**
 * Formats a canonical shortcut for human display.
 */
export function formatShortcutDisplay(shortcut: string, platform = "linux"): string {
  const isMac = platform === "macos" || platform === "darwin";
  return shortcut
    .replace(/Mod\+/g, isMac ? "⌘" : "Ctrl+")
    .replace(/Alt\+/g, isMac ? "⌥" : "Alt+")
    .replace(/Shift\+/g, isMac ? "⇧" : "Shift+")
    .replace(/Ctrl\+/g, isMac ? "⌃" : "Ctrl+");
}

/**
 * Finds conflicts involving a specific command.
 */
export function findCommandConflicts(
  command: string,
  conflicts: readonly KeybindingConflict[],
): readonly KeybindingConflict[] {
  return conflicts.filter((c) => c.commands.includes(command));
}
