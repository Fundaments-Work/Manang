import { describe, expect, it, vi } from "vitest";

import { ariaShortcut, buildMenus, menuItems, mnemonicIndex, MENU_BARS } from "./menu";
import type { Command } from "./registry";

const linux = { isMac: false };
const mac = { isMac: true };
const noop = (): void => undefined;

const commands: Command[] = [
  {
    id: "file.save",
    title: "Save",
    group: "Workbench",
    shortcuts: [{ key: "s", mod: true }],
    menu: { bar: "file", group: "3_save", order: 0 },
    run: noop,
  },
  {
    id: "file.save-all",
    title: "Save all",
    group: "Workbench",
    enabled: false,
    menu: { bar: "file", group: "3_save", order: 1 },
    run: noop,
  },
  {
    id: "file.new",
    title: "New file",
    group: "Workbench",
    menu: { bar: "file", group: "1_new", order: 1 },
    run: noop,
  },
  {
    id: "home.new-project",
    title: "New project",
    group: "Workbench",
    menu: { bar: "file", group: "1_new", order: 0 },
    run: noop,
  },
  {
    id: "doc.export-pdf",
    title: "Export PDF",
    group: "Workbench",
    when: "document",
    shortcuts: [{ key: "e", mod: true }],
    menu: [
      { bar: "file", group: "4_export", order: 0 },
      { bar: "build", group: "1_compile", order: 1 },
    ],
    run: noop,
  },
  {
    id: "view.preview-editor",
    title: "Preview: editor only",
    menuTitle: "Editor only",
    group: "Workbench",
    menu: { bar: "view", group: "4_preview", order: 0 },
    run: noop,
  },
  // Not in any menu.
  { id: "tab.go-1", title: "Go to tab 1", group: "Workbench", run: noop },
];

describe("buildMenus", () => {
  it("returns the six menus in bar order with their mnemonics", () => {
    const menus = buildMenus({ commands: [], platform: linux });
    expect(menus.map((m) => m.label)).toEqual([
      "File",
      "Edit",
      "View",
      "Go",
      "Build",
      "Help",
    ]);
    expect(menus.map((m) => m.label[m.mnemonicIndex])).toEqual([
      "F",
      "E",
      "V",
      "G",
      "B",
      "H",
    ]);
    expect(menus.every((m) => m.sections.length === 0)).toBe(true);
  });

  it("groups by group name, orders within a group, separates groups", () => {
    const [file] = buildMenus({ commands, platform: linux });
    expect(file?.sections.map((s) => s.map((i) => i.label))).toEqual([
      ["New project", "New file"],
      ["Save", "Save all"],
      ["Export PDF"],
    ]);
  });

  it("places a command in every menu it names, with its menu label", () => {
    const menus = buildMenus({ commands, platform: linux });
    const build = menus.find((m) => m.id === "build");
    expect(build && menuItems(build).map((i) => i.id)).toEqual(["doc.export-pdf"]);
    const view = menus.find((m) => m.id === "view");
    expect(view && menuItems(view)[0]?.label).toBe("Editor only");
  });

  it("disables items by enabled and by their when clause", () => {
    const applies = (when: string | undefined): boolean => when !== "document";
    const [file] = buildMenus({ commands, platform: linux, applies });
    const items = file ? menuItems(file) : [];
    const enabled = Object.fromEntries(items.map((i) => [i.id, i.enabled]));
    expect(enabled).toEqual({
      "home.new-project": true,
      "file.new": true,
      "file.save": true,
      "file.save-all": false,
      "doc.export-pdf": false,
    });
  });

  it("shows each item's current shortcut for the platform", () => {
    const [file] = buildMenus({ commands, platform: linux });
    const save = file && menuItems(file).find((i) => i.id === "file.save");
    expect(save?.shortcutText).toBe("Ctrl+S");
    expect(save?.ariaKeys).toBe("Control+S");
    const [macFile] = buildMenus({ commands, platform: mac });
    const macSave = macFile && menuItems(macFile).find((i) => i.id === "file.save");
    expect(macSave?.shortcutText).toBe("⌘S");
    expect(macSave?.ariaKeys).toBe("Meta+S");
    const newFile = file && menuItems(file).find((i) => i.id === "file.new");
    expect(newFile?.shortcutText).toBeNull();
  });

  it("adds plugin commands placed by their manifest, after the shell's groups", () => {
    const runKernel = vi.fn();
    const menus = buildMenus({
      commands,
      platform: linux,
      kernel: [
        {
          id: "marp.export",
          title: "Export slides",
          plugin: "marp",
          menus: [{ bar: "file", group: "plugins", order: 0 }],
        },
        { id: "marp.other", title: "Not placed", plugin: "marp" },
      ],
      runKernel,
    });
    const file = menus[0];
    const last = file?.sections[file.sections.length - 1];
    expect(last?.map((i) => i.label)).toEqual(["Export slides"]);
    last?.[0]?.run();
    expect(runKernel).toHaveBeenCalledWith("marp.export");
    expect(menus.flatMap(menuItems).some((i) => i.id === "marp.other")).toBe(false);
  });

  it("finds a menu by its mnemonic letter", () => {
    expect(mnemonicIndex(MENU_BARS, "F")).toBe(0);
    expect(mnemonicIndex(MENU_BARS, "v")).toBe(2);
    expect(mnemonicIndex(MENU_BARS, "x")).toBe(-1);
  });

  it("names keys for aria-keyshortcuts", () => {
    expect(ariaShortcut({ key: "p", mod: true, shift: true }, linux)).toBe(
      "Control+Shift+P",
    );
    expect(ariaShortcut({ key: "F5" }, linux)).toBe("F5");
    expect(ariaShortcut({ key: "f", alt: true, shift: true }, mac)).toBe("Alt+Shift+F");
  });
});
