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
  settingTitle,
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
    "requires-restart": false,
    owner: "core",
    category: "editor",
  },
  {
    id: "editor.word-wrap",
    type: "bool",
    default: true,
    description: "Wrap long lines at the editor width.",
    scope: "project",
    "requires-restart": false,
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
    "requires-restart": false,
    owner: "core",
    category: "appearance",
  },
  {
    id: "git.auto-fetch",
    type: "bool",
    default: false,
    description: "Fetch changes automatically.",
    scope: "user",
    "requires-restart": false,
    owner: "git",
    category: "git",
  },
];

describe("settings workbench helpers", () => {
  it("formats setting titles in sentence case", () => {
    expect(formatSettingTitle("editor.font-size")).toBe("Font size");
    expect(formatSettingTitle("files.trash-on-delete")).toBe("Trash on delete");
    expect(formatSettingTitle("word-wrap")).toBe("Word wrap");
    expect(formatSettingTitle("appearance.ui-scale")).toBe("UI scale");
    expect(formatSettingTitle("latex.latexmk")).toBe("latexmk");
  });

  it("formats category labels", () => {
    expect(formatCategoryLabel("editor")).toBe("Editor");
    expect(formatCategoryLabel("git")).toBe("Git");
    expect(formatCategoryLabel("latex")).toBe("LaTeX");
    expect(formatCategoryLabel("custom-plugin")).toBe("Custom plugin");
  });

  it("groups core categories first, then one group per plugin", () => {
    const groups = groupSettings(SAMPLE_SETTINGS);
    expect(groups.map((g) => g.id)).toEqual(["appearance", "editor", "git"]);
    expect(groups.map((g) => g.owner)).toEqual(["core", "core", "git"]);
    expect(groups[1]?.settings.map((s) => s.id)).toEqual([
      "editor.font-size",
      "editor.word-wrap",
    ]);
  });

  it("prefers the manifest title and groups by owning plugin", () => {
    const def: SettingDef = {
      id: "latex.engine",
      title: "Engine",
      type: "enum",
      values: ["pdflatex", "xelatex"],
      default: "pdflatex",
      description: "TeX engine.",
      scope: "project",
      "requires-restart": false,
      owner: "latex",
      category: "latex",
    };
    expect(settingTitle(def)).toBe("Engine");
    expect(settingTitle({ ...def, id: "latex.use-latexmk", title: undefined })).toBe(
      "Use latexmk",
    );
    const groups = groupSettings([def, ...SAMPLE_SETTINGS]);
    expect(groups.map((g) => g.id)).toEqual(["appearance", "editor", "git", "latex"]);
    expect(groups[3]?.label).toBe("LaTeX");
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
