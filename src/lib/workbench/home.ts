/**
 * Home screen rules: which projects a section shows, how a search narrows
 * them, and how they are ordered. Pure data in, data out, so the rules are
 * unit-tested and the store (`state/home.svelte.ts`) only holds the result.
 *
 * The kernel returns every project once (`project.list`, archived included);
 * filtering and sorting happen here so typing in the search field never waits
 * on IPC.
 */

import type {
  Engine,
  ProjectEntry,
  ProjectPatch,
  ProjectSort,
} from "$lib/kernel/types";

export type HomeSection = "projects" | "recent" | "pinned" | "archive" | "templates";
export type HomeView = "grid" | "list";

export const SECTIONS: readonly { id: HomeSection; label: string }[] = [
  { id: "projects", label: "Projects" },
  { id: "recent", label: "Recent" },
  { id: "pinned", label: "Pinned" },
  { id: "archive", label: "Archive" },
  { id: "templates", label: "Templates" },
];

export const SORTS: readonly { id: ProjectSort; label: string }[] = [
  { id: "modified", label: "Last opened" },
  { id: "name", label: "Name" },
  { id: "created", label: "Created" },
];

/** How many projects the Recent section shows. */
export const RECENT_LIMIT = 12;

export interface HomeQuery {
  readonly section: HomeSection;
  readonly query: string;
  readonly sort: ProjectSort;
}

function sortBy(entries: readonly ProjectEntry[], sort: ProjectSort): ProjectEntry[] {
  const out = [...entries];
  switch (sort) {
    case "name":
      out.sort(
        (a, b) =>
          a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) ||
          a.path.localeCompare(b.path),
      );
      break;
    case "created":
      out.sort((a, b) => b.created - a.created || a.path.localeCompare(b.path));
      break;
    default:
      out.sort((a, b) => b.last_opened - a.last_opened || a.path.localeCompare(b.path));
  }
  return out;
}

/** Case-insensitive match on name, folder or tag. An empty query matches all. */
export function matchesQuery(entry: ProjectEntry, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === "") return true;
  return (
    entry.name.toLowerCase().includes(needle) ||
    entry.path.toLowerCase().includes(needle) ||
    entry.tags.some((tag) => tag.toLowerCase().includes(needle))
  );
}

/** What a section shows for a query and sort. Templates has no projects. */
export function selectProjects(
  entries: readonly ProjectEntry[],
  { section, query, sort }: HomeQuery,
): ProjectEntry[] {
  const found = entries.filter((e) => matchesQuery(e, query));
  switch (section) {
    case "archive":
      return sortBy(
        found.filter((e) => e.archived),
        sort,
      );
    case "pinned":
      return sortBy(
        found.filter((e) => e.pinned && !e.archived),
        sort,
      );
    case "recent":
      return sortBy(
        found.filter((e) => !e.archived && e.exists),
        "modified",
      ).slice(0, RECENT_LIMIT);
    case "projects": {
      // Pinned projects lead; the chosen order applies within each group.
      const live = sortBy(
        found.filter((e) => !e.archived),
        sort,
      );
      return [...live.filter((e) => e.pinned), ...live.filter((e) => !e.pinned)];
    }
    default:
      return [];
  }
}

/** Apply a metadata change locally, so the card responds before the kernel answers. */
export function applyPatch(
  entries: readonly ProjectEntry[],
  path: string,
  patch: ProjectPatch,
): ProjectEntry[] {
  return entries.map((e) =>
    e.path === path
      ? {
          ...e,
          ...(patch.name !== undefined ? { name: patch.name.trim() } : {}),
          ...(patch.pinned !== undefined ? { pinned: patch.pinned } : {}),
          ...(patch.archived !== undefined ? { archived: patch.archived } : {}),
          ...(patch.tags !== undefined ? { tags: [...patch.tags] } : {}),
        }
      : e,
  );
}

/** Counts for the left nav. */
export function sectionCounts(
  entries: readonly ProjectEntry[],
): Record<string, number> {
  return {
    projects: entries.filter((e) => !e.archived).length,
    pinned: entries.filter((e) => e.pinned && !e.archived).length,
    archive: entries.filter((e) => e.archived).length,
  };
}

/** "just now", "5 minutes ago", "yesterday", "3 weeks ago", then a date. */
export function relativeTime(seconds: number, nowSeconds: number): string {
  if (seconds <= 0) return "never";
  const delta = Math.max(0, nowSeconds - seconds);
  if (delta < 60) return "just now";
  const minutes = Math.floor(delta / 60);
  if (minutes < 60) return `${minutes} ${minutes === 1 ? "minute" : "minutes"} ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} ${hours === 1 ? "hour" : "hours"} ago`;
  const days = Math.floor(hours / 24);
  if (days === 1) return "yesterday";
  if (days < 7) return `${days} days ago`;
  if (days < 30) {
    const weeks = Math.floor(days / 7);
    return `${weeks} ${weeks === 1 ? "week" : "weeks"} ago`;
  }
  return new Date(seconds * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

/** Split "a, b" style input into clean, unique tags. */
export function parseTags(text: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of text.split(/[,\n]/)) {
    const tag = raw.trim();
    if (tag === "" || seen.has(tag.toLowerCase())) continue;
    seen.add(tag.toLowerCase());
    out.push(tag);
  }
  return out;
}

export interface EmptyState {
  readonly title: string;
  readonly hint: string;
  /** The one action worth offering. */
  readonly action: "new" | "open" | "clear" | null;
}

/** What to say when a section has nothing to show, and what to offer. */
export function emptyState(
  section: HomeSection,
  query: string,
  total: number,
): EmptyState {
  if (query.trim() !== "") {
    return {
      title: `No projects match "${query.trim()}"`,
      hint: "Try fewer letters, or search by folder or tag.",
      action: "clear",
    };
  }
  if (total === 0) {
    return {
      title: "No projects yet",
      hint: "Create a project, or open a folder that already has a manang.toml.",
      action: "new",
    };
  }
  switch (section) {
    case "pinned":
      return {
        title: "Nothing pinned",
        hint: "Pin a project from its menu to keep it at the top.",
        action: null,
      };
    case "archive":
      return {
        title: "The archive is empty",
        hint: "Archive projects you are done with. They stay on disk.",
        action: null,
      };
    case "recent":
      return {
        title: "Nothing opened recently",
        hint: "Open a folder to see it here.",
        action: "open",
      };
    default:
      return {
        title: "No projects here",
        hint: "Create a project or open a folder.",
        action: "new",
      };
  }
}

/** A folder-safe name for a new project. */
export function slugify(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/** Where new projects go by default: next to the newest existing one. */
export function suggestLocation(entries: readonly ProjectEntry[]): string {
  const newest = [...entries]
    .filter((e) => e.exists)
    .sort((a, b) => b.last_opened - a.last_opened)[0];
  if (newest === undefined) return "";
  const cut = newest.path.lastIndexOf("/");
  return cut <= 0 ? "/" : newest.path.slice(0, cut);
}

export interface Starter {
  readonly engine: Engine;
  readonly title: string;
  readonly note: string;
}

/** The built-in starters (`project.create` scaffolds one per engine). */
export const STARTERS: readonly Starter[] = [
  {
    engine: "typst",
    title: "Typst article",
    note: "Title, abstract, sections and a bibliography, with an instant preview.",
  },
  {
    engine: "latex",
    title: "LaTeX article",
    note: "A standard article for journal templates and existing .tex files.",
  },
  {
    engine: "markdown",
    title: "Markdown notes",
    note: "Plain notes and simple documents, with a references file.",
  },
];

export const ENGINE_LABEL: Record<Engine, string> = {
  typst: "Typst",
  latex: "LaTeX",
  markdown: "Markdown",
};
