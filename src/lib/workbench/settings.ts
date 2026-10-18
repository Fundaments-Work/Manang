/**
 * Pure settings and keybindings workbench logic.
 */

import type { KeybindingConflict, SettingDef, SettingScope } from "$lib/kernel/api";

import { diagramLanguageOf } from "./diagram";
import { basename, engineOf, extension, type Engine } from "./paths";

export interface SettingsGroup {
  readonly id: string;
  readonly label: string;
  /** `core` for the app's own settings, otherwise the plugin that declared them. */
  readonly owner: string;
  readonly settings: readonly SettingDef[];
}

const CATEGORY_ORDER: readonly string[] = [
  "appearance",
  "editor",
  "preview",
  "compile",
  "files",
];

/** The app's own settings in reading order; others follow by id. */
const SETTING_ORDER: readonly string[] = [
  "appearance.theme",
  "appearance.ui-scale",
  "appearance.reduced-motion",
  "appearance.custom-theme",
  "appearance.coach-lines",
  "editor.font-size",
  "editor.line-height",
  "editor.word-wrap",
  "editor.line-numbers",
  "editor.tab-size",
  "preview.auto-compile",
  "preview.debounce-ms",
  "preview.double-click-source",
  "preview.dark-pages",
];

function settingRank(id: string): number {
  const at = SETTING_ORDER.indexOf(id);
  return at === -1 ? SETTING_ORDER.length : at;
}

/** Names whose capitalisation a plain rule gets wrong. */
const PROPER_NAMES: Record<string, string> = {
  latex: "LaTeX",
  bibtex: "BibTeX",
  typst: "Typst",
  git: "Git",
  ui: "UI",
  pdf: "PDF",
  csl: "CSL",
  tex: "TeX",
  pdflatex: "pdfLaTeX",
  xelatex: "XeLaTeX",
  lualatex: "LuaLaTeX",
  latexmk: "latexmk",
};

function word(w: string, first: boolean): string {
  const lower = w.toLowerCase();
  const proper = PROPER_NAMES[lower];
  if (proper !== undefined) return proper;
  return first ? lower.charAt(0).toUpperCase() + lower.slice(1) : lower;
}

/** Sentence case from kebab-case: "font-size" -> "Font size". */
function sentence(text: string): string {
  return text
    .split(/[-_ ]+/)
    .filter((w) => w.length > 0)
    .map((w, i) => word(w, i === 0))
    .join(" ");
}

/**
 * A setting's label from its id, in sentence case.
 * e.g. "editor.font-size" -> "Font size", "latex.latexmk" -> "latexmk".
 */
export function formatSettingTitle(id: string): string {
  const leaf = id.includes(".") ? id.split(".").slice(1).join(" ") : id;
  return sentence(leaf);
}

/** The label a row shows: the manifest's title when it has one. */
export function settingTitle(def: SettingDef): string {
  return def.title !== undefined && def.title !== ""
    ? def.title
    : formatSettingTitle(def.id);
}

/** Human-readable group name: "editor" -> "Editor", "latex" -> "LaTeX". */
export function formatCategoryLabel(category: string): string {
  return sentence(category);
}

/** The group a setting belongs to: its plugin, or its category for core settings. */
function groupOf(def: SettingDef): { id: string; owner: string } {
  const plugin = def.owner !== "" && def.owner !== "core";
  if (plugin) return { id: def.owner, owner: def.owner };
  return { id: def.category || def.id.split(".")[0] || "general", owner: "core" };
}

/**
 * Groups settings for display: the app's own categories in a fixed order
 * (appearance, editor, preview, compile, files, then any other), followed by
 * one group per plugin in alphabetical order.
 */
export function groupSettings(settings: readonly SettingDef[]): SettingsGroup[] {
  const map = new Map<string, { owner: string; list: SettingDef[] }>();
  for (const s of settings) {
    const { id, owner } = groupOf(s);
    const entry = map.get(id) ?? { owner, list: [] };
    entry.list.push(s);
    map.set(id, entry);
  }

  const rank = (id: string, owner: string): [number, number] => {
    const core = owner === "core" ? 0 : 1;
    const at = CATEGORY_ORDER.indexOf(id);
    return [core, at === -1 ? CATEGORY_ORDER.length : at];
  };

  return [...map.entries()]
    .sort(([a, ea], [b, eb]) => {
      const [ca, ia] = rank(a, ea.owner);
      const [cb, ib] = rank(b, eb.owner);
      return ca - cb || ia - ib || a.localeCompare(b);
    })
    .map(([id, entry]) => ({
      id,
      label: formatCategoryLabel(id),
      owner: entry.owner,
      settings: entry.list.sort(
        (a, b) => settingRank(a.id) - settingRank(b.id) || a.id.localeCompare(b.id),
      ),
    }));
}

/**
 * Filters settings by search query across id, title, and description.
 */
export function filterSettings(
  settings: readonly SettingDef[],
  query: string,
): SettingDef[] {
  const q = query.trim().toLowerCase();
  if (q === "") return [...settings];

  return settings.filter((s) => {
    const title = settingTitle(s).toLowerCase();
    const id = s.id.toLowerCase();
    const desc = s.description.toLowerCase();
    const cat = s.category.toLowerCase();
    const owner = s.owner.toLowerCase();
    return (
      id.includes(q) ||
      title.includes(q) ||
      desc.includes(q) ||
      cat.includes(q) ||
      owner.includes(q)
    );
  });
}

/**
 * Where the Settings page was opened from. It decides which groups the page
 * shows: a document shows the document's settings, Home or the palette's
 * "Open all settings" shows everything (`app`).
 */
export type SettingsOrigin =
  | { readonly kind: "app" }
  | { readonly kind: "document"; readonly path: string; readonly engine: Engine | null }
  | { readonly kind: "diagram"; readonly path: string }
  | { readonly kind: "figure"; readonly path: string }
  | { readonly kind: "pdf"; readonly path: string | null };

export const APP_ORIGIN: SettingsOrigin = { kind: "app" };

/** The context of a group that declares none (`DEFAULT_SETTINGS_CONTEXT`). */
const DEFAULT_CONTEXTS: readonly string[] = ["app"];

/** The contexts a setting's group belongs to; `["app"]` when it declares none. */
export function contextsOf(item: {
  readonly contexts?: readonly string[] | undefined;
}) {
  return item.contexts !== undefined && item.contexts.length > 0
    ? item.contexts
    : DEFAULT_CONTEXTS;
}

/** The contexts an origin shows, or null for the app scope (everything). */
export function originContexts(origin: SettingsOrigin): readonly string[] | null {
  switch (origin.kind) {
    case "app":
      return null;
    case "document":
      return origin.engine === null
        ? ["document"]
        : ["document", `engine:${origin.engine}`];
    case "diagram":
      return ["diagram"];
    case "figure":
      return ["figure"];
    case "pdf":
      return ["pdf"];
  }
}

/** Whether a group declaring `contexts` shows for `origin`. */
export function inContext(
  contexts: readonly string[],
  origin: SettingsOrigin,
): boolean {
  const wanted = originContexts(origin);
  return wanted === null || contexts.some((c) => wanted.includes(c));
}

/**
 * A plugin section's contexts: its manifest's `settings_contexts` (from the
 * plugin list), else those its settings carry, else `["app"]`.
 */
export function pluginContexts(
  name: string,
  info:
    readonly { readonly name: string; readonly contexts?: readonly string[] }[] | null,
  defs: readonly SettingDef[],
): readonly string[] {
  const declared = info?.find((p) => p.name === name)?.contexts;
  if (declared !== undefined && declared.length > 0) return declared;
  const own = defs.find((d) => d.owner === name);
  return own === undefined ? DEFAULT_CONTEXTS : contextsOf(own);
}

/** The settings groups shown for `origin`, in display order. */
export function groupsForContext(
  defs: readonly SettingDef[],
  origin: SettingsOrigin,
): SettingsGroup[] {
  return groupSettings(defs.filter((d) => inContext(contextsOf(d), origin)));
}

/** A search split into what the scope shows and what only "all settings" would. */
export interface ScopedMatches {
  readonly here: SettingDef[];
  readonly elsewhere: SettingDef[];
}

export function splitMatches(
  defs: readonly SettingDef[],
  query: string,
  origin: SettingsOrigin,
): ScopedMatches {
  const here: SettingDef[] = [];
  const elsewhere: SettingDef[] = [];
  for (const def of filterSettings(defs, query)) {
    (inContext(contextsOf(def), origin) ? here : elsewhere).push(def);
  }
  return { here, elsewhere };
}

/**
 * Which file the page writes to when it opens: in a document scope with a
 * project, "This project" when the project overrides any shown setting (so
 * what you see is what you change), otherwise User.
 */
export function defaultSettingsScope(
  origin: SettingsOrigin,
  shown: readonly SettingDef[],
  sourceOf: (id: string) => string,
  hasProject: boolean,
): SettingScope {
  if (!hasProject || origin.kind !== "document") return "user";
  return shown.some((d) => d.scope === "project" && sourceOf(d.id) === "project")
    ? "project"
    : "user";
}

const ENGINE_LABELS: Record<Engine, string> = {
  typst: "Typst",
  latex: "LaTeX",
  markdown: "Markdown",
};

/** The one line naming the scope, e.g. "Settings for main.typ · Typst". */
export function scopeLabel(origin: SettingsOrigin): string {
  switch (origin.kind) {
    case "app":
      return "All settings";
    case "document":
      return origin.engine === null
        ? `Settings for ${basename(origin.path)}`
        : `Settings for ${basename(origin.path)} · ${ENGINE_LABELS[origin.engine]}`;
    case "diagram":
      return `Settings for ${basename(origin.path)} · Diagram`;
    case "figure":
      return `Settings for ${basename(origin.path)} · Figure`;
    case "pdf":
      return origin.path === null
        ? "Settings for the PDF viewer"
        : `Settings for ${basename(origin.path)} · PDF`;
  }
}

/** What an origin needs to know about the tab Settings opens over. */
export interface OriginTab {
  readonly kind: "document" | "figure" | "viewer";
  readonly path: string;
}

/**
 * The origin for Settings opened over `tab`. Home (or no tab) is the app
 * scope; a figure tab the figure scope; a PDF viewer tab the PDF scope; a
 * diagram source the diagram scope; any other document its engine's scope.
 */
export function settingsOriginFor(
  tab: OriginTab | undefined,
  onHome: boolean,
): SettingsOrigin {
  if (onHome || tab === undefined) return APP_ORIGIN;
  switch (tab.kind) {
    case "figure":
      return { kind: "figure", path: tab.path };
    case "viewer":
      return extension(tab.path) === "pdf"
        ? { kind: "pdf", path: tab.path }
        : APP_ORIGIN;
    case "document":
      if (diagramLanguageOf(tab.path) !== null)
        return { kind: "diagram", path: tab.path };
      return { kind: "document", path: tab.path, engine: engineOf(tab.path) };
  }
}

/**
 * Determines whether a setting is modified from its default.
 */
export function isSettingModified(
  value: unknown,
  defaultValue: unknown,
  source?: string,
): boolean {
  if (source !== undefined && source !== "default") {
    return true;
  }
  return JSON.stringify(value) !== JSON.stringify(defaultValue);
}

export interface ValidationResult {
  readonly valid: boolean;
  readonly error?: string;
}

/**
 * Validates candidate setting value against definition.
 */
export function validateSettingValue(
  def: SettingDef,
  value: unknown,
): ValidationResult {
  switch (def.type) {
    case "bool":
      if (typeof value !== "boolean") {
        return { valid: false, error: "Must be a boolean (true or false)" };
      }
      return { valid: true };

    case "int": {
      if (typeof value !== "number" || !Number.isInteger(value)) {
        return { valid: false, error: "Must be an integer" };
      }
      if (def.min !== undefined && value < def.min) {
        return { valid: false, error: `Minimum value is ${def.min}` };
      }
      if (def.max !== undefined && value > def.max) {
        return { valid: false, error: `Maximum value is ${def.max}` };
      }
      return { valid: true };
    }

    case "float": {
      if (typeof value !== "number" || !Number.isFinite(value)) {
        return { valid: false, error: "Must be a valid number" };
      }
      if (def.min !== undefined && value < def.min) {
        return { valid: false, error: `Minimum value is ${def.min}` };
      }
      if (def.max !== undefined && value > def.max) {
        return { valid: false, error: `Maximum value is ${def.max}` };
      }
      return { valid: true };
    }

    case "enum": {
      if (typeof value !== "string" || !(def.values ?? []).includes(value)) {
        return {
          valid: false,
          error: `Must be one of: ${(def.values ?? []).join(", ")}`,
        };
      }
      return { valid: true };
    }

    case "string":
    case "path": {
      if (typeof value !== "string") {
        return { valid: false, error: "Must be text" };
      }
      if (def.type === "path" && value.includes("\0")) {
        return { valid: false, error: "Path cannot contain null bytes" };
      }
      return { valid: true };
    }

    case "list": {
      if (!Array.isArray(value)) {
        return { valid: false, error: "Must be a list" };
      }
      return { valid: true };
    }

    default:
      return { valid: true };
  }
}

/**
 * Converts a KeyboardEvent into canonical shortcut string format (e.g. `Mod+Shift+P`).
 * Returns null if only modifier keys were pressed.
 */
export function recordShortcutFromEvent(
  event: KeyboardEvent,
  platform = "linux",
): string | null {
  const key = event.key;
  if (key === "Control" || key === "Shift" || key === "Alt" || key === "Meta") {
    return null;
  }

  const parts: string[] = [];

  // Use Mod+ to represent Command on macOS and Ctrl on Linux/Windows
  const isMac = platform === "macos" || platform === "darwin";
  const modPressed = isMac ? event.metaKey : event.ctrlKey;
  const secondaryCtrl = isMac && event.ctrlKey;

  if (modPressed) parts.push("Mod");
  if (secondaryCtrl) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");

  // Format the key
  let keyName = key;
  if (key === " ") keyName = "Space";
  else if (key.length === 1) keyName = key.toUpperCase();

  parts.push(keyName);
  return parts.join("+");
}

/**
 * Formats a canonical shortcut for human display.
 */
export function formatShortcutDisplay(shortcut: string, platform = "linux"): string {
  const isMac = platform === "macos" || platform === "darwin";
  return shortcut
    .replace(/Mod\+/g, isMac ? "⌘" : "Ctrl+")
    .replace(/Alt\+/g, isMac ? "⌥" : "Alt+")
    .replace(/Shift\+/g, isMac ? "⇧" : "Shift+")
    .replace(/Ctrl\+/g, isMac ? "⌃" : "Ctrl+");
}

/**
 * Finds conflicts involving a specific command.
 */
export function findCommandConflicts(
  command: string,
  conflicts: readonly KeybindingConflict[],
): readonly KeybindingConflict[] {
  return conflicts.filter((c) => c.commands.includes(command));
}
