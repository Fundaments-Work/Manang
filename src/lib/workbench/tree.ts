/**
 * File tree model.
 *
 * The tree is loaded lazily: `listings` maps a directory path to the entries
 * the kernel returned for it, and only expanded directories are ever asked for.
 * `visibleRows` turns that plus the expansion set into the flat, ordered list
 * the `tree` widget renders, with the ARIA numbers (level, set size, position)
 * already computed so a virtualised or partial DOM stays accessible.
 */

import { fuzzyRank } from "$lib/commands/fuzzy";
import type { ChangeState, GitStatus, TreeEntry } from "$lib/kernel/types";

import { basename, dirname } from "./paths";

/** Entries the UI never shows: the placeholder that keeps empty folders alive. */
export const KEEP_FILE = ".gitkeep";

export interface TreeRow {
  readonly path: string;
  readonly name: string;
  readonly kind: "file" | "dir";
  readonly depth: number;
  readonly expanded: boolean;
  /** 1-based position among siblings, for `aria-posinset`. */
  readonly position: number;
  readonly setSize: number;
  /** Directory whose listing has not arrived yet. */
  readonly loading: boolean;
  /** Folder shown in a filtered list, to tell same-named files apart. */
  readonly hint: string;
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** Folders first, then names in natural order (`2.typ` before `10.typ`). */
export function sortEntries(entries: readonly TreeEntry[]): TreeEntry[] {
  return entries
    .filter((entry) => entry.name !== KEEP_FILE)
    .sort((a, b) => {
      const dirA = a.kind === "dir" ? 0 : 1;
      const dirB = b.kind === "dir" ? 0 : 1;
      return dirA - dirB || collator.compare(a.name, b.name);
    });
}

export function visibleRows(
  listings: ReadonlyMap<string, readonly TreeEntry[]>,
  expanded: ReadonlySet<string>,
): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (dir: string, depth: number): void => {
    const entries = sortEntries(listings.get(dir) ?? []);
    entries.forEach((entry, index) => {
      const isDir = entry.kind === "dir";
      const open = isDir && expanded.has(entry.path);
      rows.push({
        path: entry.path,
        name: entry.name,
        kind: isDir ? "dir" : "file",
        depth,
        expanded: open,
        position: index + 1,
        setSize: entries.length,
        loading: open && !listings.has(entry.path),
        hint: "",
      });
      if (open) walk(entry.path, depth + 1);
    });
  };
  walk("", 0);
  return rows;
}

/** Fuzzy filter over every file in the project, as one flat list. */
export function filterRows(all: readonly TreeEntry[], query: string, limit = 200): TreeRow[] {
  const files = all.filter((e) => e.kind !== "dir" && e.name !== KEEP_FILE);
  const ranked = fuzzyRank(files, query, (entry) => entry.path).slice(0, limit);
  return ranked.map(({ item }, index) => ({
    path: item.path,
    name: item.name,
    kind: "file",
    depth: 0,
    expanded: false,
    position: index + 1,
    setSize: ranked.length,
    loading: false,
    hint: dirname(item.path),
  }));
}

/** Directories to ask the kernel for before `path` can be shown. */
export function ancestors(path: string): string[] {
  const parts = path.split("/");
  const out: string[] = [];
  for (let i = 1; i < parts.length; i += 1) out.push(parts.slice(0, i).join("/"));
  return out;
}

/* -------------------------------------------------------------------------- */
/* Git                                                                         */
/* -------------------------------------------------------------------------- */

export interface Badge {
  /** One letter, as in `git status --short`. */
  readonly letter: string;
  /** Plain-language state, for the tooltip and the accessible name. */
  readonly label: string;
}

const BADGES: Record<ChangeState, Badge> = {
  added: { letter: "A", label: "Added" },
  modified: { letter: "M", label: "Modified" },
  deleted: { letter: "D", label: "Deleted" },
  renamed: { letter: "R", label: "Renamed" },
  copied: { letter: "C", label: "Copied" },
  typechange: { letter: "T", label: "Type changed" },
  conflicted: { letter: "!", label: "Has conflicts" },
  "intent-to-add": { letter: "A", label: "Added" },
  untracked: { letter: "U", label: "Untracked" },
};

export interface GitMarks {
  readonly files: ReadonlyMap<string, Badge>;
  /** Folders containing at least one changed file. */
  readonly folders: ReadonlySet<string>;
}

export function gitMarks(status: GitStatus | null): GitMarks {
  const files = new Map<string, Badge>();
  const folders = new Set<string>();
  if (status === null || !status.is_repo) return { files, folders };
  for (const change of status.changes) {
    const state = change.unstaged ?? change.staged;
    if (state === null) continue;
    files.set(change.path, BADGES[state]);
    for (const dir of ancestors(change.path)) folders.add(dir);
  }
  return { files, folders };
}

/** `main`, or `main*` when the work tree has changes. */
export function branchLabel(status: GitStatus | null): string | null {
  if (status === null || !status.is_repo) return null;
  const name = status.branch ?? (status.head === null ? "no commits" : status.head.slice(0, 7));
  return status.changes.length > 0 ? `${name}*` : name;
}

/* -------------------------------------------------------------------------- */
/* Keyboard navigation (WAI-ARIA tree pattern)                                 */
/* -------------------------------------------------------------------------- */

export type TreeKey = "ArrowDown" | "ArrowUp" | "ArrowRight" | "ArrowLeft" | "Home" | "End";

export type TreeMove =
  | { readonly action: "focus"; readonly path: string }
  | { readonly action: "expand"; readonly path: string }
  | { readonly action: "collapse"; readonly path: string }
  | { readonly action: "none" };

/** What a navigation key does from `current`, per the APG tree view pattern. */
export function navigate(rows: readonly TreeRow[], current: string, key: TreeKey): TreeMove {
  const index = rows.findIndex((row) => row.path === current);
  const row = rows[index];
  if (row === undefined) {
    const first = rows[0];
    return first === undefined ? { action: "none" } : { action: "focus", path: first.path };
  }
  const focus = (target: TreeRow | undefined): TreeMove =>
    target === undefined ? { action: "none" } : { action: "focus", path: target.path };
  switch (key) {
    case "ArrowDown":
      return focus(rows[index + 1]);
    case "ArrowUp":
      return focus(rows[index - 1]);
    case "Home":
      return focus(rows[0]);
    case "End":
      return focus(rows[rows.length - 1]);
    case "ArrowRight":
      if (row.kind === "dir") {
        return row.expanded ? focus(rows[index + 1]?.depth === row.depth + 1 ? rows[index + 1] : undefined) : { action: "expand", path: row.path };
      }
      return { action: "none" };
    case "ArrowLeft":
      if (row.kind === "dir" && row.expanded) return { action: "collapse", path: row.path };
      return focus([...rows.slice(0, index)].reverse().find((r) => r.depth === row.depth - 1));
  }
}

/** Type-ahead: the next row after `current` whose name starts with `typed`. */
export function typeAhead(rows: readonly TreeRow[], current: string, typed: string): string | null {
  if (typed === "") return null;
  const lower = typed.toLowerCase();
  const start = rows.findIndex((row) => row.path === current);
  const order = [...rows.slice(start + 1), ...rows.slice(0, start + 1)];
  return order.find((row) => row.name.toLowerCase().startsWith(lower))?.path ?? null;
}

export { basename };
