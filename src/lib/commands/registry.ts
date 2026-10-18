/**
 * Command shapes shared by the palette, the keyboard map and the kernel merge.
 *
 * Workbench commands (panels, tabs, files) are built in `workbench.ts` from the
 * live state; kernel commands arrive from the plugin kernel. The palette treats
 * both the same: a titled, searchable thing with an optional shortcut.
 */

import type { MenuPlacement } from "./menu";
import type { Shortcut } from "./shortcut";

export type CommandGroup = "Workbench" | "Files" | "Plugin commands";

export interface Command {
  readonly id: string;
  readonly title: string;
  readonly group: CommandGroup;
  /** Extra searchable terms that do not appear in the title. */
  readonly keywords?: string;
  /** Shortcuts that run it; the first is the one shown. */
  readonly shortcuts?: readonly Shortcut[];
  /** Quiet secondary text: a kernel command id, or a file's folder. */
  readonly hint?: string;
  /** False greys the entry out and stops it running. */
  readonly enabled?: boolean;
  /**
   * Where the command applies, as a `when` clause over the UI context keys
   * (`workbench && document`, `home`, `!settings`). Outside it the command is
   * left out of the palette and its shortcut does nothing.
   */
  readonly when?: string | undefined;
  /** Where it sits in the menu bar; a list for a command in two menus. */
  readonly menu?: MenuPlacement | readonly MenuPlacement[] | undefined;
  /** A shorter label for the menu, where the menu already gives context. */
  readonly menuTitle?: string | undefined;
  /**
   * `surface`: the focused editor, canvas or field handles the keys itself
   * (Undo, Copy, Find); the shortcut is shown but the shell never binds it.
   */
  readonly keys?: "surface" | undefined;
  /** False keeps it out of the palette (Go to tab 3 is a key, not a search). */
  readonly palette?: boolean | undefined;
  readonly run: () => void;
}

/** The string the fuzzy matcher scores against. */
export function searchText(command: Command): string {
  return `${command.title} ${command.keywords ?? ""}`;
}
