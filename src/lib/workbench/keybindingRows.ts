import type { KeybindingItem } from "$lib/kernel/api";

/** A keybinding as a table row, with an id unique within the table. */
export interface KeybindingRow extends KeybindingItem {
  readonly id: string;
}

/**
 * Table rows for the keybindings list. A command may have several bindings
 * (the palette is Ctrl+K and Ctrl+Shift+P), so the command alone is not a
 * row key; exact duplicates (a binding listed twice) are shown once.
 */
export function keybindingRows(bindings: readonly KeybindingItem[]): KeybindingRow[] {
  const seen = new Set<string>();
  const rows: KeybindingRow[] = [];
  for (const b of bindings) {
    const id = [b.command, b.key, b.when, b.source].join("\u0000");
    if (seen.has(id)) continue;
    seen.add(id);
    rows.push({ ...b, id });
  }
  return rows;
}
