/**
 * Project switcher (Ctrl+O): which projects it offers and in what order.
 *
 * Archived projects and folders that have gone are left out; matches rank by
 * fit, then pinned, then most recently opened. The project that is open stays
 * in the list, marked and last, so the list never looks empty and Enter never
 * "switches" to where you already are by accident.
 */

import { fuzzyMatch } from "$lib/commands/fuzzy";
import type { ProjectEntry } from "$lib/kernel/types";

export interface SwitcherItem {
  readonly entry: ProjectEntry;
  readonly current: boolean;
}

export function rankProjects(
  entries: readonly ProjectEntry[],
  query: string,
  currentPath: string | null,
): SwitcherItem[] {
  const trimmed = query.trim();
  const scored: { entry: ProjectEntry; score: number }[] = [];
  for (const entry of entries) {
    if (entry.archived || !entry.exists) continue;
    if (trimmed === "") {
      scored.push({ entry, score: 0 });
      continue;
    }
    const byName = fuzzyMatch(entry.name, trimmed);
    const byPath = fuzzyMatch(entry.path, trimmed);
    const tag = entry.tags.some((t) => t.toLowerCase().includes(trimmed.toLowerCase()));
    if (byName === null && byPath === null && !tag) continue;
    // A name match outweighs a folder match; a tag match is a fallback.
    scored.push({
      entry,
      score: Math.max((byName?.score ?? -1) * 2, byPath?.score ?? -1, tag ? 1 : -1),
    });
  }
  scored.sort(
    (a, b) =>
      b.score - a.score ||
      Number(b.entry.pinned) - Number(a.entry.pinned) ||
      b.entry.last_opened - a.entry.last_opened ||
      a.entry.path.localeCompare(b.entry.path),
  );
  const items = scored.map(({ entry }) => ({
    entry,
    current: entry.path === currentPath,
  }));
  return [...items.filter((i) => !i.current), ...items.filter((i) => i.current)];
}

/** Keep a cursor inside a list of `length` rows. */
export function clampCursor(cursor: number, length: number): number {
  return Math.max(0, Math.min(cursor, length - 1));
}
