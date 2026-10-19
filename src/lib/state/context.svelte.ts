/**
 * The live UI context (see `workbench/context.ts`): one derived value for the
 * whole shell, so the top bar, palette, keyboard map and panels agree on where
 * the person is without each re-deriving it.
 *
 * While Settings is open the surface is `settings`, but the editor and
 * engine are kept: they are those of the place Settings was opened from
 * (`settingsPage.origin`), not dropped.
 */

import { helpPage } from "$lib/state/helpPage.svelte";
import { home } from "$lib/state/home.svelte";
import { layout } from "$lib/state/layout.svelte";
import { settingsPage } from "$lib/state/settingsPage.svelte";
import { tabs } from "$lib/state/tabs.svelte";
import { workspace } from "$lib/state/workspace.svelte";
import {
  contextKeys,
  whenMatches,
  type ContextKey,
  type EditorKind,
  type UiContext,
} from "$lib/workbench/context";
import { engineOf, type Engine } from "$lib/workbench/paths";
import { settingsOriginFor, type SettingsOrigin } from "$lib/workbench/settings";

function originEditor(origin: SettingsOrigin): {
  editor: EditorKind | null;
  engine: Engine | null;
} {
  switch (origin.kind) {
    case "document":
      return { editor: "document", engine: origin.engine };
    case "diagram":
      return { editor: "document", engine: null };
    case "figure":
      return { editor: "figure", engine: null };
    default:
      return { editor: null, engine: null };
  }
}

class ContextState {
  readonly current: UiContext = $derived.by(() => {
    const surface = helpPage.open
      ? "help"
      : settingsPage.open
        ? "settings"
        : home.showing
          ? "home"
          : "workbench";
    const base = {
      surface,
      project: workspace.project !== null,
      zen: layout.zen,
    } as const;
    if (helpPage.open) return { ...base, editor: null, engine: null };
    if (settingsPage.open) return { ...base, ...originEditor(settingsPage.origin) };
    const active = tabs.active;
    const editor: EditorKind | null =
      active === undefined || active.kind === "viewer" || active.kind === "plugin"
        ? null
        : active.kind === "figure"
          ? "figure"
          : active.view === "slides"
            ? "slides"
            : "document";
    return {
      ...base,
      editor,
      engine: active === undefined ? null : engineOf(active.path),
    };
  });

  readonly keys: ReadonlySet<ContextKey> = $derived(contextKeys(this.current));

  get surface(): UiContext["surface"] {
    return this.current.surface;
  }

  /**
   * Where Settings would be opened from right now: the active tab's place
   * on the workbench, the app scope on Home or with no tab. While Settings
   * is open, the place it was opened from.
   */
  get settingsOrigin(): SettingsOrigin {
    if (settingsPage.open) return settingsPage.origin;
    return settingsOriginFor(tabs.active, home.showing);
  }

  /** Whether a `when` clause holds right now. */
  when(clause: string | undefined): boolean {
    return whenMatches(clause, this.keys);
  }
}

export const context = new ContextState();
