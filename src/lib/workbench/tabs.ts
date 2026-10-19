/**
 * Tab model. Pure and immutable: every function returns a new model, so the
 * runes store can assign the result and tests can compare plain data.
 *
 * A tab holds a text document, a figure, a read-only viewer (an image or a
 * PDF), a plugin's page (path `plugin:<id>`, not a file) or (as a view of a
 * `.typ` document) a slide deck. There is at most one tab per file or plugin;
 * opening one again focuses its tab.
 */

import { isPluginTabPath, pluginIdOf } from "./market";
import { openAsByName, tabKindOf } from "./opener";
import { basename, dirname, isUnder, moved } from "./paths";

export type TabKind = "document" | "figure" | "viewer" | "plugin";
export type DocumentView = "text" | "slides";

export interface Tab {
  readonly id: string;
  readonly kind: TabKind;
  readonly path: string;
  readonly view: DocumentView;
}

export interface TabsModel {
  readonly tabs: readonly Tab[];
  readonly active: string | null;
  /** Most recently active first; used to pick the neighbour after a close. */
  readonly history: readonly string[];
}

export const EMPTY_TABS: TabsModel = { tabs: [], active: null, history: [] };

export const tabId = (path: string): string => path;

/**
 * The tab kind the name gives (`opener.ts`). A name that says nothing (no
 * or an unknown extension) falls back to `fallback`: the kind decided from
 * the file's bytes when it was opened.
 */
export function kindForPath(path: string, fallback: TabKind = "document"): TabKind {
  if (isPluginTabPath(path)) return "plugin";
  const as = openAsByName(path);
  return as === null ? fallback : tabKindOf(as);
}

export function makeTab(
  path: string,
  view: DocumentView = "text",
  kind?: TabKind,
): Tab {
  return { id: tabId(path), kind: kind ?? kindForPath(path), path, view };
}

function withActive(model: TabsModel, id: string | null): TabsModel {
  if (id === null) return { ...model, active: null };
  return {
    ...model,
    active: id,
    history: [id, ...model.history.filter((h) => h !== id)],
  };
}

export function activeTab(model: TabsModel): Tab | undefined {
  return model.tabs.find((tab) => tab.id === model.active);
}

/** Open `path`, or focus its tab. New tabs land right of the active one. */
export function openTab(
  model: TabsModel,
  path: string,
  view?: DocumentView,
  kind?: TabKind,
): TabsModel {
  const id = tabId(path);
  const existing = model.tabs.find((tab) => tab.id === id);
  if (existing !== undefined) {
    const next =
      view === undefined || view === existing.view
        ? model
        : { ...model, tabs: model.tabs.map((t) => (t.id === id ? { ...t, view } : t)) };
    return withActive(next, id);
  }
  const at = model.tabs.findIndex((tab) => tab.id === model.active);
  const index = at === -1 ? model.tabs.length : at + 1;
  const tabs = [
    ...model.tabs.slice(0, index),
    makeTab(path, view, kind),
    ...model.tabs.slice(index),
  ];
  return withActive({ ...model, tabs }, id);
}

export function activateTab(model: TabsModel, id: string): TabsModel {
  return model.tabs.some((tab) => tab.id === id) ? withActive(model, id) : model;
}

export function setView(model: TabsModel, id: string, view: DocumentView): TabsModel {
  return {
    ...model,
    tabs: model.tabs.map((tab) =>
      tab.id === id && tab.kind === "document" ? { ...tab, view } : tab,
    ),
  };
}

/** Show a tab's file in another editor (an SVG as text), keeping its place. */
export function setKind(model: TabsModel, id: string, kind: TabKind): TabsModel {
  return {
    ...model,
    tabs: model.tabs.map((tab) =>
      tab.id === id ? { ...tab, kind, view: "text" as const } : tab,
    ),
  };
}

/** Close a tab. If it was active, focus the most recently used survivor. */
export function closeTab(model: TabsModel, id: string): TabsModel {
  const index = model.tabs.findIndex((tab) => tab.id === id);
  if (index === -1) return model;
  const tabs = model.tabs.filter((tab) => tab.id !== id);
  const history = model.history.filter((h) => h !== id);
  if (model.active !== id) return { tabs, active: model.active, history };
  const survivor =
    history.find((h) => tabs.some((t) => t.id === h)) ??
    tabs[Math.min(index, tabs.length - 1)]?.id ??
    null;
  return withActive({ tabs, active: null, history }, survivor);
}

export function closeOthers(model: TabsModel, id: string): TabsModel {
  const keep = model.tabs.filter((tab) => tab.id === id);
  return keep.length === 0
    ? model
    : withActive({ tabs: keep, active: null, history: [] }, id);
}

export function closeToRight(model: TabsModel, id: string): TabsModel {
  const index = model.tabs.findIndex((tab) => tab.id === id);
  if (index === -1) return model;
  let next = model;
  for (const tab of model.tabs.slice(index + 1)) next = closeTab(next, tab.id);
  return next;
}

/** Step through tabs in strip order, wrapping at the ends. */
export function cycleTab(model: TabsModel, direction: 1 | -1): TabsModel {
  const count = model.tabs.length;
  if (count === 0) return model;
  const at = model.tabs.findIndex((tab) => tab.id === model.active);
  const next = model.tabs[(at + direction + count) % count];
  return next === undefined ? model : withActive(model, next.id);
}

/** Ctrl+1..8 pick that tab; Ctrl+9 picks the last, as in most editors. */
export function tabByShortcut(model: TabsModel, digit: number): TabsModel {
  const target =
    digit === 9 ? model.tabs[model.tabs.length - 1] : model.tabs[digit - 1];
  return target === undefined ? model : withActive(model, target.id);
}

export function moveTab(model: TabsModel, id: string, toIndex: number): TabsModel {
  const from = model.tabs.findIndex((tab) => tab.id === id);
  if (from === -1) return model;
  const to = Math.max(0, Math.min(toIndex, model.tabs.length - 1));
  if (from === to) return model;
  const tabs = [...model.tabs];
  const [tab] = tabs.splice(from, 1);
  if (tab === undefined) return model;
  tabs.splice(to, 0, tab);
  return { ...model, tabs };
}

/**
 * A rename does not change what a file holds: a viewer stays a viewer (a large
 * `.txt`, say), but a file renamed to or from `.figure` changes editor.
 */
function renamedKind(tab: Tab, path: string): TabKind {
  const named = kindForPath(path, tab.kind);
  if (named === "figure" || tab.kind === "figure") return named;
  return tab.kind === "viewer" || tab.kind === "plugin" ? tab.kind : named;
}

/** A file or folder was renamed: keep its tabs, pointing at the new path. */
export function renamePath(model: TabsModel, from: string, to: string): TabsModel {
  const remap = (id: string): string => moved(id, from, to);
  return {
    tabs: model.tabs.map((tab) => {
      const path = moved(tab.path, from, to);
      return path === tab.path
        ? tab
        : { ...tab, path, id: tabId(path), kind: renamedKind(tab, path) };
    }),
    active: model.active === null ? null : remap(model.active),
    history: model.history.map(remap),
  };
}

/** A file or folder is gone: close every tab inside it. */
export function removePath(model: TabsModel, path: string): TabsModel {
  let next = model;
  for (const tab of model.tabs)
    if (isUnder(tab.path, path)) next = closeTab(next, tab.id);
  return next;
}

/**
 * Labels for the strip. A file name that appears twice gets its folder as a
 * suffix so the two tabs can be told apart.
 */
export function tabLabels(
  tabs: readonly Tab[],
): Map<string, { name: string; hint: string }> {
  const counts = new Map<string, number>();
  for (const tab of tabs)
    if (tab.kind !== "plugin")
      counts.set(basename(tab.path), (counts.get(basename(tab.path)) ?? 0) + 1);
  return new Map(
    tabs.map((tab) => {
      const name = tab.kind === "plugin" ? pluginIdOf(tab.path) : basename(tab.path);
      const hint =
        tab.kind !== "plugin" && (counts.get(name) ?? 0) > 1
          ? basename(dirname(tab.path))
          : "";
      return [tab.id, { name, hint }];
    }),
  );
}

/* -------------------------------------------------------------------------- */
/* Persistence                                                                 */
/* -------------------------------------------------------------------------- */

export interface SessionData {
  readonly project: string | null;
  readonly tabs: readonly { path: string; view: DocumentView }[];
  readonly active: string | null;
}

export function toSession(project: string | null, model: TabsModel): SessionData {
  return {
    project,
    // A plugin's page is not a file: it is not brought back next time.
    tabs: model.tabs
      .filter((tab) => tab.kind !== "plugin")
      .map((tab) => ({ path: tab.path, view: tab.view })),
    active: model.tabs.some((t) => t.id === model.active && t.kind === "plugin")
      ? null
      : model.active,
  };
}

/** Validate persisted session data. */
export function sanitizeSession(raw: unknown): SessionData {
  const empty: SessionData = { project: null, tabs: [], active: null };
  if (typeof raw !== "object" || raw === null) return empty;
  const r = raw as Record<string, unknown>;
  const project =
    typeof r["project"] === "string" && r["project"] !== "" ? r["project"] : null;
  const seen = new Set<string>();
  const tabs: { path: string; view: DocumentView }[] = [];
  if (Array.isArray(r["tabs"])) {
    for (const entry of r["tabs"] as unknown[]) {
      if (typeof entry !== "object" || entry === null) continue;
      const e = entry as Record<string, unknown>;
      const path = e["path"];
      if (typeof path !== "string" || path === "" || seen.has(path)) continue;
      seen.add(path);
      tabs.push({ path, view: e["view"] === "slides" ? "slides" : "text" });
    }
  }
  const active =
    typeof r["active"] === "string" && seen.has(r["active"]) ? r["active"] : null;
  return {
    project,
    tabs: project === null ? [] : tabs.slice(0, 24),
    active: project === null ? null : active,
  };
}

export function fromSession(session: SessionData): TabsModel {
  let model = EMPTY_TABS;
  for (const entry of session.tabs) model = openTab(model, entry.path, entry.view);
  return session.active === null ? model : activateTab(model, session.active);
}
