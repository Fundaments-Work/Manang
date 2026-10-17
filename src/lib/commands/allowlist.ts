/**
 * Which plugin commands the palette offers.
 *
 * The palette runs a command with no arguments. Most plugin commands need some
 * (`typst.compile` wants a main file, `project.rename` two paths) and many are
 * internals the shell calls itself (`editor.*`, `*.cancel`, `*.get`), so they
 * would only fail or confuse. Plugin commands are therefore opt-in: only ids
 * listed here appear, each with the way it is run.
 *
 * - `kernel`: `kernel.run(id)` with no arguments is complete and safe.
 * - `action`: the command needs the editor (its text, its caret), so the shell
 *   runs its own action; the palette entry is the plugin's title with that
 *   action behind it.
 *
 * `coveredBy` names a workbench command that already offers the same thing
 * (with its shortcut), so the plugin entry is not listed twice.
 */

import type { KernelCommand } from "$lib/kernel/client";

export type PaletteRun =
  | { readonly kind: "kernel" }
  | { readonly kind: "action"; readonly action: PaletteAction };

export type PaletteAction = "format" | "spell" | "stats" | "export-pdf";

export interface PaletteEntry {
  readonly run: PaletteRun;
  readonly coveredBy?: string;
}

export const PALETTE_ALLOWLIST: Readonly<Record<string, PaletteEntry>> = {
  "typst.export-pdf": {
    run: { kind: "action", action: "export-pdf" },
    coveredBy: "doc.export-pdf",
  },
  "writing.format.document": {
    run: { kind: "action", action: "format" },
    coveredBy: "doc.format",
  },
  "writing.spell.check": { run: { kind: "action", action: "spell" } },
  "writing.stats.count": { run: { kind: "action", action: "stats" } },
};

/** True for plugin commands that are safe to run from the palette. */
export function isPaletteSafe(id: string): boolean {
  return Object.hasOwn(PALETTE_ALLOWLIST, id);
}

/** The action a palette id maps to, or `null` for a plain kernel call. */
export function paletteAction(id: string): PaletteAction | null {
  const entry = Object.hasOwn(PALETTE_ALLOWLIST, id)
    ? PALETTE_ALLOWLIST[id]
    : undefined;
  return entry?.run.kind === "action" ? entry.run.action : null;
}

/**
 * The kernel's commands narrowed to what the palette should list: allowlisted
 * ids, minus those a workbench command (by id, in `workbenchIds`) already covers.
 */
export function paletteKernelCommands(
  commands: readonly KernelCommand[],
  workbenchIds: ReadonlySet<string> = new Set(),
): KernelCommand[] {
  return commands.filter((command) => {
    const entry = Object.hasOwn(PALETTE_ALLOWLIST, command.id)
      ? PALETTE_ALLOWLIST[command.id]
      : undefined;
    if (entry === undefined) return false;
    return entry.coveredBy === undefined || !workbenchIds.has(entry.coveredBy);
  });
}
