/**
 * What the command palette lists, as pure functions.
 *
 * Two modes, as in other editors: "commands" (Ctrl+K, Ctrl+Shift+P) lists
 * workbench commands and plugin commands, and surfaces matching files once you
 * type; "files" (Ctrl+P) lists project files only.
 */

import type { KernelCommand } from "$lib/kernel/client";
import { basename, dirname } from "$lib/workbench/paths";

import { fuzzyRank, type Ranked } from "./fuzzy";
import { mergeCommands } from "./kernel";
import { searchText, type Command, type CommandGroup } from "./registry";

export type PaletteMode = "commands" | "files";

export const GROUP_ORDER: readonly CommandGroup[] = [
  "Workbench",
  "Files",
  "Plugin commands",
];

export function fileCommand(path: string, open: (path: string) => void): Command {
  return {
    id: `file:${path}`,
    title: basename(path),
    group: "Files",
    keywords: path,
    hint: dirname(path),
    run: () => open(path),
  };
}

export interface PaletteInput {
  readonly mode: PaletteMode;
  readonly query: string;
  readonly workbench: readonly Command[];
  readonly files: readonly string[];
  readonly kernel: readonly KernelCommand[];
  readonly runKernel: (id: string) => void;
  readonly openFile: (path: string) => void;
}

/** Every candidate for the current mode, before ranking. */
export function paletteCandidates(input: PaletteInput): Command[] {
  const files = input.files.map((path) => fileCommand(path, input.openFile));
  if (input.mode === "files") return files;
  const commands = mergeCommands(input.workbench, input.kernel, input.runKernel);
  return input.query.trim() === "" ? commands : [...commands, ...files];
}

export function rankCandidates(
  candidates: readonly Command[],
  query: string,
): Ranked<Command>[] {
  return fuzzyRank(candidates, query, searchText);
}

export interface PaletteGroup {
  readonly group: CommandGroup;
  readonly entries: readonly Ranked<Command>[];
}

/** Ranked results bucketed by group, in display order. Empty groups are dropped. */
export function groupRanked(ranked: readonly Ranked<Command>[]): PaletteGroup[] {
  return GROUP_ORDER.map((group) => ({
    group,
    entries: ranked.filter((entry) => entry.item.group === group),
  })).filter((bucket) => bucket.entries.length > 0);
}

/** The flat order the cursor walks: groups in display order, rank within each. */
export function flatten(groups: readonly PaletteGroup[]): Command[] {
  return groups.flatMap((g) => g.entries.map((entry) => entry.item));
}
