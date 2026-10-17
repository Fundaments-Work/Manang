import { describe, expect, it } from "vitest";
import type { KeybindingConflict, SettingDef } from "$lib/kernel/api";
import {
  filterSettings,
  findCommandConflicts,
  formatCategoryLabel,
  formatSettingTitle,
  formatShortcutDisplay,
  groupSettings,
  isSettingModified,
  recordShortcutFromEvent,
  validateSettingValue,
} from "./settings";

const SAMPLE_SETTINGS: SettingDef[] = [
  {
    id: "editor.font-size",
    type: "int",
    min: 8,
    max: 48,
    default: 14,
    description: "Editor font size in pixels.",
    scope: "project",
    requires_restart: false,
    owner: "core",
    category: "editor",
  },
  {
    id: "editor.word-wrap",
    type: "bool",
    default: true,
    description: "Wrap long lines at the editor width.",
    scope: "project",
    requires_restart: false,
    owner: "core",
    category: "editor",
  },
  {
    id: "appearance.theme",
    type: "enum",
    values: ["dark", "light", "system"],
    default: "dark",
    description: "Colour theme.",
    scope: "user",
    requires_restart: false,
    owner: "core",
    category: "appearance",
  },
  {
    id: "git.auto-fetch",
    type: "bool",
    default: false,
    description: "Fetch changes automatically.",
    scope: "user",
    requires_restart: false,
    owner: "git",
    category: "git",
  },
];

describe("settings workbench helpers", () => {
  it("formats setting titles into plain title case", () => {
    expect(formatSettingTitle("editor.font-size")).toBe("Font Size");
    expect(formatSettingTitle("files.trash-on-delete")).toBe("Trash On Delete");
    expect(formatSettingTitle("word-wrap")).toBe("Word Wrap");
  });

  it("formats category labels", () => {
    expect(formatCategoryLabel("editor")).toBe("Editor");
    expect(formatCategoryLabel("git")).toBe("Git");
    expect(formatCategoryLabel("custom-plugin")).toBe("Custom-plugin");
  });

  it("groups settings in specified category order", () => {
    const groups = groupSettings(SAMPLE_SETTINGS);
    expect(groups.map((g) => g.id)).toEqual(["editor", "appearance", "git"]);
    expect(groups[0]?.settings.map((s) => s.id)).toEqual([
      "editor.font-size",
      "editor.word-wrap",
    ]);
  });

  it("filters settings by search query", () => {
    const matched = filterSettings(SAMPLE_SETTINGS, "font");
    expect(matched.length).toBe(1);
    expect(matched[0]?.id).toBe("editor.font-size");

    const byDesc = filterSettings(SAMPLE_SETTINGS, "pixels");
    expect(byDesc.length).toBe(1);
    expect(byDesc[0]?.id).toBe("editor.font-size");

    const byCat = filterSettings(SAMPLE_SETTINGS, "git");
    expect(byCat.length).toBe(1);
    expect(byCat[0]?.id).toBe("git.auto-fetch");
  });

  it("identifies modified settings correctly", () => {
    expect(isSettingModified(14, 14, "default")).toBe(false);
    expect(isSettingModified(16, 14, "user")).toBe(true);
    expect(isSettingModified(14, 14, "user")).toBe(true); // user layer set explicitly
    expect(isSettingModified(16, 14)).toBe(true);
  });

  it("validates setting values against type constraints", () => {
    const intDef = SAMPLE_SETTINGS[0]!;
    expect(validateSettingValue(intDef, 14).valid).toBe(true);
    expect(validateSettingValue(intDef, 4).valid).toBe(false);
    expect(validateSettingValue(intDef, 100).valid).toBe(false);
    expect(validateSettingValue(intDef, "14").valid).toBe(false);

    const enumDef = SAMPLE_SETTINGS[2]!;
    expect(validateSettingValue(enumDef, "dark").valid).toBe(true);
    expect(validateSettingValue(enumDef, "neon").valid).toBe(false);
  });

  it("records keyboard shortcuts from event", () => {
    const ev1 = {
      key: "s",
      ctrlKey: true,
      metaKey: false,
      altKey: false,
      shiftKey: false,
    } as KeyboardEvent;
    expect(recordShortcutFromEvent(ev1, "linux")).toBe("Mod+S");

    const loneCtrl = {
      key: "Control",
      ctrlKey: true,
      metaKey: false,
      altKey: false,
      shiftKey: false,
    } as KeyboardEvent;
    expect(recordShortcutFromEvent(loneCtrl, "linux")).toBeNull();
  });

  it("formats shortcuts for display on linux and mac", () => {
    expect(formatShortcutDisplay("Mod+S", "linux")).toBe("Ctrl+S");
    expect(formatShortcutDisplay("Mod+Shift+P", "linux")).toBe("Ctrl+Shift+P");
    expect(formatShortcutDisplay("Mod+S", "macos")).toBe("⌘S");
  });

  it("finds conflicts for a command", () => {
    const conflicts: KeybindingConflict[] = [
      { key: "Ctrl+S", kind: "conflict", commands: ["editor.save", "custom.save"] },
      { key: "Ctrl+P", kind: "shadow", commands: ["palette.open", "custom.print"] },
    ];
    expect(findCommandConflicts("editor.save", conflicts).length).toBe(1);
    expect(findCommandConflicts("other.cmd", conflicts).length).toBe(0);
  });
});
