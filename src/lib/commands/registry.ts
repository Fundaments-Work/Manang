/**
 * Command shapes shared by the palette, the keyboard map and the kernel merge.
 *
 * Workbench commands (panels, tabs, files) are built in `workbench.ts` from the
 * live state; kernel commands arrive from the plugin kernel. The palette treats
 * both the same: a titled, searchable thing with an optional shortcut.
 */

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
  readonly run: () => void;
}

/** The string the fuzzy matcher scores against. */
export function searchText(command: Command): string {
  return `${command.title} ${command.keywords ?? ""}`;
}
