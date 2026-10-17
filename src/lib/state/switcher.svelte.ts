/**
 * Project switcher (Ctrl+O): a small palette over the project registry.
 *
 * It only ranks and picks (`workbench/switcher.ts`); opening goes through
 * `openProject`, which asks about unsaved changes first.
 */

import { kernel } from "$lib/kernel";
import { projectApi } from "$lib/kernel/api";
import type { ProjectEntry } from "$lib/kernel/types";
import { clampCursor, rankProjects, type SwitcherItem } from "$lib/workbench/switcher";

import { openProject } from "./actions";
import { workspace } from "./workspace.svelte";

const api = projectApi(kernel);

export class SwitcherState {
  open = $state(false);
  query = $state("");
  cursor = $state(0);
  entries = $state.raw<readonly ProjectEntry[]>([]);

  readonly items: SwitcherItem[] = $derived(
    rankProjects(this.entries, this.query, workspace.project?.root ?? null),
  );
  readonly selected: SwitcherItem | undefined = $derived(
    this.items[clampCursor(this.cursor, this.items.length)],
  );

  async show(): Promise<void> {
    this.query = "";
    this.cursor = 0;
    this.open = true;
    try {
      this.entries = await api.list({});
    } catch {
      this.entries = [];
    }
  }

  hide(): void {
    this.open = false;
    this.query = "";
    this.cursor = 0;
  }

  setQuery(next: string): void {
    this.query = next;
    this.cursor = 0;
  }

  move(delta: number): void {
    this.cursor = clampCursor(this.cursor + delta, this.items.length);
  }

  /** Switch to the highlighted project; choosing the open one just closes. */
  async choose(item: SwitcherItem | undefined = this.selected): Promise<void> {
    if (item === undefined) return;
    this.hide();
    if (item.current) return;
    await openProject(item.entry.path);
  }
}

export const switcher = new SwitcherState();
