/**
 * The plugin marketplace's rules, free of Svelte and the kernel so they can be
 * unit tested: comparing versions, searching and sorting the registry list,
 * what a permission means in plain words, how a listing's freshness and an
 * error are worded, and the little formatting a plugin page needs.
 */

import type {
  BrowseEntry,
  BrowseSort,
  Freshness,
  RegistryAccess,
} from "$lib/kernel/marketApi";
import type { PluginAccess } from "$lib/kernel/pluginsApi";

import { accessDifference, type AccessLine } from "./pluginManager";
import { describeAccess } from "./plugins";

/* -------------------------------------------------------------------------- */
/* Versions                                                                    */
/* -------------------------------------------------------------------------- */

interface Parsed {
  readonly core: readonly [number, number, number];
  readonly pre: readonly string[];
}

function parseVersion(version: string): Parsed | null {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?(?:\+[0-9A-Za-z.-]+)?$/.exec(
    version.trim(),
  );
  if (match === null) return null;
  return {
    core: [Number(match[1]), Number(match[2]), Number(match[3])],
    pre: match[4] === undefined ? [] : match[4].split("."),
  };
}

function comparePre(a: readonly string[], b: readonly string[]): number {
  // A release is newer than any of its pre-releases.
  if (a.length === 0 || b.length === 0) return b.length - a.length;
  for (let i = 0; i < Math.max(a.length, b.length); i += 1) {
    const x = a[i];
    const y = b[i];
    if (x === undefined) return -1;
    if (y === undefined) return 1;
    const nx = /^\d+$/.test(x);
    const ny = /^\d+$/.test(y);
    if (nx && ny) {
      if (Number(x) !== Number(y)) return Number(x) < Number(y) ? -1 : 1;
    } else if (nx !== ny) {
      return nx ? -1 : 1;
    } else if (x !== y) {
      return x < y ? -1 : 1;
    }
  }
  return 0;
}

/**
 * Semantic-version order: negative when `a` is older than `b`. Versions that
 * are not versions compare as equal (nothing is offered on a guess).
 */
export function compareVersions(a: string, b: string): number {
  const x = parseVersion(a);
  const y = parseVersion(b);
  if (x === null || y === null) return 0;
  for (let i = 0; i < 3; i += 1) {
    const dx = x.core[i] ?? 0;
    const dy = y.core[i] ?? 0;
    if (dx !== dy) return dx < dy ? -1 : 1;
  }
  return comparePre(x.pre, y.pre);
}

/** Whether `latest` is a newer version than `installed`. */
export function isNewer(latest: string, installed: string): boolean {
  return compareVersions(latest, installed) > 0;
}

/* -------------------------------------------------------------------------- */
/* Search, filter, sort                                                        */
/* -------------------------------------------------------------------------- */

export const BROWSE_SORTS: readonly {
  readonly id: BrowseSort;
  readonly label: string;
}[] = [
  { id: "name", label: "Name" },
  { id: "newest", label: "Newest" },
  { id: "downloads", label: "Downloads" },
];

function matches(
  entry: BrowseEntry,
  words: readonly string[],
  tag: string | null,
): boolean {
  if (tag !== null && !entry.tags.some((t) => t.toLowerCase() === tag.toLowerCase()))
    return false;
  if (words.length === 0) return true;
  const haystack = [
    entry.name,
    entry.id,
    entry.author,
    entry.description,
    ...entry.tags,
  ]
    .join("\n")
    .toLowerCase();
  return words.every((w) => haystack.includes(w));
}

/**
 * The registry list as the Browse tab shows it: every word of `query` must
 * appear in the name, id, author, description or a tag; `tag` narrows to one
 * tag; then ordered by name, newest release or downloads (ties by name).
 */
export function visibleBrowse(
  entries: readonly BrowseEntry[],
  query: string,
  tag: string | null,
  sort: BrowseSort,
): BrowseEntry[] {
  const words = query
    .toLowerCase()
    .split(/\s+/)
    .filter((w) => w !== "");
  const byName = (a: BrowseEntry, b: BrowseEntry): number =>
    a.name.toLowerCase().localeCompare(b.name.toLowerCase()) ||
    a.id.localeCompare(b.id);
  return entries
    .filter((e) => matches(e, words, tag))
    .sort((a, b) => {
      if (sort === "newest")
        return (b.published ?? "").localeCompare(a.published ?? "") || byName(a, b);
      if (sort === "downloads")
        return (b.downloads ?? 0) - (a.downloads ?? 0) || byName(a, b);
      return byName(a, b);
    });
}

/** Count of plugins with a newer version than the installed one. */
export function updateCount(entries: readonly BrowseEntry[]): number {
  return entries.filter((e) => e.installed?.update_available === true).length;
}

/* -------------------------------------------------------------------------- */
/* Permissions in plain words                                                  */
/* -------------------------------------------------------------------------- */

/** One capability, said so a person can decide. */
export interface PermissionItem {
  /** What it can do: "Read files in your project". */
  readonly text: string;
  /** What that means and its limit, one sentence. */
  readonly meaning: string;
  /** Not approved before: an update asks for it for the first time. */
  readonly isNew: boolean;
}

const FS_SCOPES: Record<string, string> = {
  project: "files in your project",
  config: "its own settings folder",
  cache: "its own cache folder",
};

const BUFFER_SCOPES: Record<string, string> = {
  active: "the open document",
};

function fsItem(entry: string): { text: string; meaning: string } {
  const [mode, scope] = entry.split(":");
  const where = scope === undefined ? undefined : FS_SCOPES[scope];
  if (where === undefined || (mode !== "read" && mode !== "write"))
    return {
      text: `Access to ${entry}`,
      meaning: "A kind of access this version of Manang cannot describe.",
    };
  if (scope === "project")
    return mode === "read"
      ? {
          text: "Read files in your project",
          meaning: "It can open and read any file in the folder you have open.",
        }
      : {
          text: "Read and change files in your project",
          meaning:
            "It can read, create and overwrite files in the folder you have open. Use Git to undo.",
        };
  return mode === "read"
    ? {
        text: `Read ${where}`,
        meaning: "Only a folder Manang keeps for this plugin, not your documents.",
      }
    : {
        text: `Read and change ${where}`,
        meaning: "Only a folder Manang keeps for this plugin, not your documents.",
      };
}

function bufferItem(entry: string): { text: string; meaning: string } {
  const [mode, scope] = entry.split(":");
  const where = scope === undefined ? undefined : BUFFER_SCOPES[scope];
  if (where === undefined || (mode !== "read" && mode !== "write"))
    return {
      text: `Access to ${entry}`,
      meaning: "A kind of access this version of Manang cannot describe.",
    };
  return mode === "read"
    ? {
        text: `Read ${where}`,
        meaning: "It can see the text you are writing while it is open.",
      }
    : {
        text: `Read and change ${where}`,
        meaning: "It can see and edit the text you are writing. Undo works as usual.",
      };
}

/**
 * The plain-language list for `access`: one line per capability, with what it
 * means. Changes come before reads; nothing is dropped, an entry that cannot
 * be described is shown verbatim. `added` marks capabilities an update asks
 * for beyond what was approved.
 */
export function permissionItems(
  access: RegistryAccess,
  added?: RegistryAccess,
): PermissionItem[] {
  const newFs = new Set(added?.filesystem ?? []);
  const newNet = new Set((added?.network ?? []).map((h) => h.toLowerCase()));
  const newBuf = new Set(added?.buffer ?? []);
  const items: PermissionItem[] = [];
  for (const entry of access.filesystem)
    items.push({ ...fsItem(entry), isNew: newFs.has(entry) });
  for (const entry of access.buffer)
    items.push({ ...bufferItem(entry), isNew: newBuf.has(entry) });
  for (const host of access.network)
    items.push({
      text: `Connect to ${host}`,
      meaning: `It can send and receive data with ${host} and nowhere else.`,
      isNew: newNet.has(host.toLowerCase()),
    });
  const rank = (i: PermissionItem): number =>
    (i.text.startsWith("Read and change") ? 0 : 1) + (i.isNew ? 10 : 0);
  return items.sort((a, b) => rank(a) - rank(b));
}

/** The sentence under an empty permissions list. */
export const NO_PERMISSIONS =
  "It asks for no access to your files, documents or network. It can only run its own commands.";

/* -------------------------------------------------------------------------- */
/* Listing state and errors                                                    */
/* -------------------------------------------------------------------------- */

/** `2026-10-19` for a Unix timestamp in seconds. */
export function isoDate(seconds: number): string {
  return new Date(seconds * 1000).toISOString().slice(0, 10);
}

/** The date part of an ISO timestamp (`2026-10-10T12:00:00Z`), or "". */
export function publishedDate(iso: string | null | undefined): string {
  return iso === null || iso === undefined ? "" : iso.slice(0, 10);
}

/** What the Browse tab says about how current its list is; null when nothing needs saying. */
export function freshnessNotice(freshness: Freshness): string | null {
  const date = isoDate(freshness.fetched_at);
  if (freshness.state === "offline")
    return `You are offline or the registry is not answering. Showing the list from ${date}.`;
  if (freshness.state === "cached") return null;
  return null;
}

export type BrowseProblem =
  "off" | "not_configured" | "signature" | "schema" | "unreachable" | "other";

/** The `[code]` of a kernel error message, if it has one. */
export function errorCode(message: string): string | null {
  const m = /^\s*\[([a-z_]+)\]/.exec(message);
  return m?.[1] ?? null;
}

/** A kernel error text without its `[code]` prefix. */
export function errorText(message: string): string {
  return message.replace(/^\s*\[[a-z_]+\]\s*/, "");
}

export function browseProblem(message: string): BrowseProblem {
  switch (errorCode(message)) {
    case "registry_off":
      return "off";
    case "registry_not_configured":
      return "not_configured";
    case "registry_signature":
    case "registry_stale":
    case "registry_bad_index":
      return "signature";
    case "registry_schema":
      return "schema";
    case "registry_unreachable":
      return "unreachable";
    default:
      return "other";
  }
}

/** The headline and detail for a Browse problem. */
export function problemWording(
  problem: BrowseProblem,
  message: string,
): { readonly title: string; readonly detail: string } {
  switch (problem) {
    case "off":
      return {
        title: "Browsing plugins is turned off",
        detail:
          "Manang does not contact the plugin registry. Turn it on in Settings, under Plugins.",
      };
    case "not_configured":
      return {
        title: "The plugin registry is not available yet",
        detail:
          "This build of Manang has no registry address or key. You can still install a plugin from a folder or file.",
      };
    case "signature":
      return {
        title: "The plugin list could not be verified",
        detail:
          "Nothing was installed. Try again later; installed plugins are not affected.",
      };
    case "schema":
      return {
        title: "The plugin list needs a newer Manang",
        detail: errorText(message),
      };
    case "unreachable":
      return {
        title: "The plugin registry could not be reached",
        detail: "Check your connection, then try again.",
      };
    case "other":
      return { title: "Plugins could not be listed", detail: errorText(message) };
  }
}

/* -------------------------------------------------------------------------- */
/* Small formatting                                                            */
/* -------------------------------------------------------------------------- */

/** `48 KB`, `1.2 MB`. */
export function formatSize(bytes: number | null | undefined): string {
  if (bytes === null || bytes === undefined) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** `1.2k`, `13k`, `2.1M` for a download count. */
export function formatCount(n: number | null | undefined): string {
  if (n === null || n === undefined) return "";
  if (n < 1000) return String(n);
  if (n < 10_000) return `${(n / 1000).toFixed(1).replace(/\.0$/, "")}k`;
  if (n < 1_000_000) return `${Math.round(n / 1000)}k`;
  return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
}

/** Up to two initials for a plugin without an icon: "Word count" gives "WC". */
export function initials(name: string): string {
  const words = name
    .replace(/[-_]+/g, " ")
    .split(/\s+/)
    .filter((w) => w !== "");
  const first = words[0]?.[0] ?? "?";
  const second = words.length > 1 ? (words[1]?.[0] ?? "") : (words[0]?.[1] ?? "");
  return (first + second).toUpperCase();
}

/** The virtual path of a plugin page tab. */
export const PLUGIN_TAB_PREFIX = "plugin:";

export const pluginTabPath = (id: string): string => `${PLUGIN_TAB_PREFIX}${id}`;

export const isPluginTabPath = (path: string): boolean =>
  path.startsWith(PLUGIN_TAB_PREFIX);

export const pluginIdOf = (path: string): string =>
  path.slice(PLUGIN_TAB_PREFIX.length);

/**
 * The consent dialog's lines for an update: what it keeps, then what it adds,
 * flagged New. Unlike `accessLines`, an update whose whole ask is new (the
 * plugin had no access before) still flags every line.
 */
export function updateConsentLines(
  requested: PluginAccess,
  added: PluginAccess,
): AccessLine[] {
  return [
    ...describeAccess(accessDifference(requested, added)).map((text) => ({
      text,
      isNew: false,
    })),
    ...describeAccess(added).map((text) => ({ text, isNew: true })),
  ];
}
