/**
 * The live UI context (see `workbench/context.ts`): one derived value for the
 * whole shell, so the top bar, palette, keyboard map and panels agree on where
 * the person is without each re-deriving it.
 */

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
import { engineOf } from "$lib/workbench/paths";

class ContextState {
  readonly current: UiContext = $derived.by(() => {
    const active = tabs.active;
    const editor: EditorKind | null =
      active === undefined || active.kind === "viewer"
        ? null
        : active.kind === "figure"
          ? "figure"
          : active.view === "slides"
            ? "slides"
            : "document";
    return {
      surface: settingsPage.open ? "settings" : home.showing ? "home" : "workbench",
      project: workspace.project !== null,
      editor,
      engine: active === undefined ? null : engineOf(active.path),
      zen: layout.zen,
    };
  });

  readonly keys: ReadonlySet<ContextKey> = $derived(contextKeys(this.current));

  get surface(): UiContext["surface"] {
    return this.current.surface;
  }

  /** Whether a `when` clause holds right now. */
  when(clause: string | undefined): boolean {
    return whenMatches(clause, this.keys);
  }
}

export const context = new ContextState();
