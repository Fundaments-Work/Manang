/**
 * The menu bar's model, built from the command registry.
 *
 * Menus never have handlers of their own: every item is a command with a
 * `menu` placement (or a plugin command placed by its manifest's
 * `[[contributes.menus]]`), so the palette, the keyboard and the menus run the
 * same thing and show the same shortcut. Pure, so grouping, ordering and the
 * enabled state are unit-tested; the title bar draws the result and macOS
 * hands it to the native application menu.
 */

import type { KernelCommand } from "$lib/kernel/client";

import type { Command } from "./registry";
import { formatShortcut, type Platform, type Shortcut } from "./shortcut";

export type MenuBarId = "file" | "edit" | "view" | "go" | "build" | "help";

/**
 * Where a command sits: groups are separated by a rule and sorted by name
 * (the shell's are `1_new`, `2_open`, ...; plugins default to `plugins`, after
 * them), items within a group by `order`, then label.
 */
export interface MenuPlacement {
  readonly bar: MenuBarId;
  readonly group: string;
  readonly order: number;
}

export interface MenuBarSpec {
  readonly id: MenuBarId;
  readonly label: string;
  /** The underlined letter; Alt plus it opens the menu. */
  readonly mnemonic: string;
}

export const MENU_BARS: readonly MenuBarSpec[] = [
  { id: "file", label: "File", mnemonic: "f" },
  { id: "edit", label: "Edit", mnemonic: "e" },
  { id: "view", label: "View", mnemonic: "v" },
  { id: "go", label: "Go", mnemonic: "g" },
  { id: "build", label: "Build", mnemonic: "b" },
  { id: "help", label: "Help", mnemonic: "h" },
];

export interface MenuItem {
  /** The command id (plugin commands keep their kernel id). */
  readonly id: string;
  readonly label: string;
  /** The current shortcut as shown, or null. */
  readonly shortcut: Shortcut | null;
  readonly shortcutText: string | null;
  /** `aria-keyshortcuts` value (`Control+Shift+P`), or null. */
  readonly ariaKeys: string | null;
  readonly enabled: boolean;
  readonly run: () => void;
}

export interface Menu extends MenuBarSpec {
  /** Index of the mnemonic letter in the label. */
  readonly mnemonicIndex: number;
  /** Groups in order; the menu draws a separator between two. */
  readonly sections: readonly (readonly MenuItem[])[];
}

export interface MenuInput {
  readonly commands: readonly Command[];
  readonly kernel?: readonly KernelCommand[] | undefined;
  readonly runKernel?: ((id: string) => void) | undefined;
  /** Whether a `when` clause holds; everything applies when absent. */
  readonly applies?: ((when: string | undefined) => boolean) | undefined;
  readonly platform: Platform;
}

/** `aria-keyshortcuts` names: `Control+Shift+P`, `Meta+K` on macOS. */
export function ariaShortcut(shortcut: Shortcut, platform: Platform): string {
  const key =
    shortcut.key === " "
      ? "Space"
      : shortcut.key.length === 1
        ? shortcut.key.toUpperCase()
        : shortcut.key;
  return [
    shortcut.mod === true ? (platform.isMac ? "Meta" : "Control") : null,
    shortcut.alt === true ? "Alt" : null,
    shortcut.shift === true ? "Shift" : null,
    key,
  ]
    .filter((part): part is string => part !== null)
    .join("+");
}

function placements(command: Command): readonly MenuPlacement[] {
  const menu = command.menu;
  if (menu === undefined) return [];
  return "bar" in menu ? [menu as MenuPlacement] : (menu as readonly MenuPlacement[]);
}

interface Placed {
  readonly placement: MenuPlacement;
  readonly item: MenuItem;
}

/** Every menu, in bar order, each with its items grouped and ordered. */
export function buildMenus(input: MenuInput): Menu[] {
  const applies = input.applies ?? (() => true);
  const placed: Placed[] = [];
  const seen = new Set<string>();
  const add = (placement: MenuPlacement, item: MenuItem): void => {
    const key = `${placement.bar} ${item.id}`;
    if (seen.has(key)) return;
    seen.add(key);
    placed.push({ placement, item });
  };

  for (const command of input.commands) {
    const shortcut = command.shortcuts?.[0] ?? null;
    for (const placement of placements(command)) {
      add(placement, {
        id: command.id,
        label: command.menuTitle ?? command.title,
        shortcut,
        shortcutText:
          shortcut === null ? null : formatShortcut(shortcut, input.platform),
        ariaKeys: shortcut === null ? null : ariaShortcut(shortcut, input.platform),
        enabled: command.enabled !== false && applies(command.when),
        run: command.run,
      });
    }
  }

  const runKernel = input.runKernel;
  for (const command of input.kernel ?? []) {
    for (const placement of command.menus ?? []) {
      add(placement, {
        id: command.id,
        label: command.title,
        shortcut: null,
        shortcutText: null,
        ariaKeys: null,
        enabled: runKernel !== undefined,
        run: () => runKernel?.(command.id),
      });
    }
  }

  return MENU_BARS.map((bar) => {
    const mine = placed
      .filter((p) => p.placement.bar === bar.id)
      .sort(
        (a, b) =>
          (a.placement.group < b.placement.group
            ? -1
            : a.placement.group > b.placement.group
              ? 1
              : 0) ||
          a.placement.order - b.placement.order ||
          a.item.label.localeCompare(b.item.label),
      );
    const sections: MenuItem[][] = [];
    let group: string | null = null;
    for (const p of mine) {
      if (p.placement.group !== group || sections.length === 0) {
        sections.push([]);
        group = p.placement.group;
      }
      sections[sections.length - 1]?.push(p.item);
    }
    return {
      ...bar,
      mnemonicIndex: bar.label.toLowerCase().indexOf(bar.mnemonic),
      sections,
    };
  });
}

/** A menu's items in the order the keyboard walks them. */
export function menuItems(menu: Menu): MenuItem[] {
  return menu.sections.flat();
}

/** The menu a mnemonic letter opens, by index, or -1. */
export function mnemonicIndex(menus: readonly MenuBarSpec[], key: string): number {
  const lower = key.toLowerCase();
  return menus.findIndex((m) => m.mnemonic === lower);
}
