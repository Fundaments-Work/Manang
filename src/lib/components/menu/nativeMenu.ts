/**
 * macOS: the same menu model as the application menu, through Tauri's menu
 * API (`@tauri-apps/api/menu`; its commands are in `core:default`, so the
 * capability needs nothing new). People expect the menus at the top of the
 * screen there, so the title bar keeps only the mark and the breadcrumb.
 *
 * Undo, Redo, Cut, Copy, Paste and Select all are the system's predefined
 * items: the web view's own editing then works in every field, and the
 * editor's keymap still sees the keys. Everything else runs its command.
 */

import type { Menu as MenuModel, MenuItem } from "$lib/commands/menu";
import type { Shortcut } from "$lib/commands/shortcut";

type Predefined = "Undo" | "Redo" | "Cut" | "Copy" | "Paste" | "SelectAll";

const PREDEFINED: Readonly<Record<string, Predefined>> = {
  "edit.undo": "Undo",
  "edit.redo": "Redo",
  "edit.cut": "Cut",
  "edit.copy": "Copy",
  "edit.paste": "Paste",
  "edit.select-all": "SelectAll",
};

/** Commands the macOS application menu (the "Manang" one) carries instead. */
const APP_MENU: ReadonlySet<string> = new Set([
  "help.about",
  "view.settings",
  "app.quit",
]);

const KEY_NAMES: Readonly<Record<string, string>> = {
  "\\": "Backslash",
  ",": "Comma",
  "/": "Slash",
  ".": "Period",
  " ": "Space",
  "+": "Equal",
};

/** A Tauri accelerator (`CmdOrCtrl+Shift+P`) for a shortcut. */
export function accelerator(shortcut: Shortcut): string {
  const key =
    KEY_NAMES[shortcut.key] ??
    (shortcut.key.length === 1 ? shortcut.key.toUpperCase() : shortcut.key);
  return [
    shortcut.mod === true ? "CmdOrCtrl" : null,
    shortcut.alt === true ? "Alt" : null,
    shortcut.shift === true ? "Shift" : null,
    key,
  ]
    .filter((part): part is string => part !== null)
    .join("+");
}

type ItemOptions =
  | {
      item:
        | Predefined
        | "Separator"
        | "Quit"
        | "Hide"
        | "HideOthers"
        | "ShowAll"
        | "Services";
    }
  | {
      id: string;
      text: string;
      enabled: boolean;
      accelerator?: string;
      action: () => void;
    };

function option(item: MenuItem): ItemOptions {
  const predefined = PREDEFINED[item.id];
  if (predefined !== undefined) return { item: predefined };
  return {
    id: item.id,
    text: item.label,
    enabled: item.enabled,
    ...(item.shortcut === null ? {} : { accelerator: accelerator(item.shortcut) }),
    action: () => item.run(),
  };
}

function sectionsOf(menu: MenuModel, skip: ReadonlySet<string>): ItemOptions[] {
  const out: ItemOptions[] = [];
  for (const section of menu.sections) {
    const items = section.filter((i) => !skip.has(i.id));
    if (items.length === 0) continue;
    if (out.length > 0) out.push({ item: "Separator" });
    out.push(...items.map(option));
  }
  return out;
}

/** Replace the application menu with this model. */
export async function applyNativeMenu(menus: readonly MenuModel[]): Promise<void> {
  const { Menu } = await import("@tauri-apps/api/menu");
  const all = menus.flatMap((m) => m.sections.flat());
  const pick = (id: string): ItemOptions[] => {
    const item = all.find((i) => i.id === id);
    return item === undefined ? [] : [option(item)];
  };
  const app = {
    text: "Manang",
    items: [
      ...pick("help.about"),
      { item: "Separator" as const },
      ...pick("view.settings"),
      { item: "Separator" as const },
      { item: "Services" as const },
      { item: "Separator" as const },
      { item: "Hide" as const },
      { item: "HideOthers" as const },
      { item: "ShowAll" as const },
      { item: "Separator" as const },
      ...pick("app.quit"),
    ],
  };
  const menu = await Menu.new({
    items: [
      app,
      ...menus.map((m) => ({ text: m.label, items: sectionsOf(m, APP_MENU) })),
    ],
  });
  const previous = await menu.setAsAppMenu();
  await previous?.close();
}
