/**
 * Open tabs. The model and its rules are pure (`workbench/tabs.ts`); this holds
 * the reactive copy and remembers it between runs.
 */

import { SESSION_KEY, saveJson } from "$lib/workbench/persist";
import {
  EMPTY_TABS,
  activateTab,
  activeTab,
  closeOthers,
  closeTab,
  closeToRight,
  cycleTab,
  moveTab,
  openTab,
  removePath,
  renamePath,
  setKind,
  setView,
  tabByShortcut,
  toSession,
  type DocumentView,
  type Tab,
  type TabKind,
  type TabsModel,
} from "$lib/workbench/tabs";

import { workspace } from "./workspace.svelte";

export class TabsState {
  model = $state.raw<TabsModel>(EMPTY_TABS);

  readonly tabs: readonly Tab[] = $derived(this.model.tabs);
  readonly active: Tab | undefined = $derived(activeTab(this.model));

  #set(next: TabsModel): void {
    this.model = next;
    this.remember();
  }

  /** Remember the project and its tabs for the next start. */
  remember(): void {
    saveJson(SESSION_KEY, toSession(workspace.project?.root ?? null, this.model));
  }

  replace(model: TabsModel): void {
    this.#set(model);
  }
  open(path: string, view?: DocumentView, kind?: TabKind): void {
    this.#set(openTab(this.model, path, view, kind));
  }
  activate(id: string): void {
    this.#set(activateTab(this.model, id));
  }
  close(id: string): void {
    this.#set(closeTab(this.model, id));
  }
  closeOthers(id: string): void {
    this.#set(closeOthers(this.model, id));
  }
  closeToRight(id: string): void {
    this.#set(closeToRight(this.model, id));
  }
  cycle(direction: 1 | -1): void {
    this.#set(cycleTab(this.model, direction));
  }
  byShortcut(digit: number): void {
    this.#set(tabByShortcut(this.model, digit));
  }
  move(id: string, toIndex: number): void {
    this.#set(moveTab(this.model, id, toIndex));
  }
  setView(id: string, view: DocumentView): void {
    this.#set(setView(this.model, id, view));
  }
  /** Show the tab for `path` in another editor, in the same place. */
  reopenAs(path: string, kind: TabKind): void {
    this.#set(activateTab(setKind(this.model, path, kind), path));
  }
  renamed(from: string, to: string): void {
    this.#set(renamePath(this.model, from, to));
  }
  removed(path: string): void {
    this.#set(removePath(this.model, path));
  }
  clear(): void {
    this.#set(EMPTY_TABS);
  }
}

export const tabs = new TabsState();
