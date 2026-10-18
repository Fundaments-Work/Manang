/**
 * Home: the project list screen. It is the surface when no project is open,
 * and can be shown over an open project without closing it.
 *
 * The registry comes from `project.list` (everything, archived included); the
 * rules for sections, search and order are pure (`workbench/home.ts`). Changes
 * (pin, archive, rename) show at once and are sent to the kernel; a failure
 * puts the old value back and says why.
 */

import { SvelteDate, SvelteMap } from "svelte/reactivity";

import { errorMessage, kernel } from "$lib/kernel";
import { projectApi } from "$lib/kernel/api";
import type {
  Engine,
  ProjectEntry,
  ProjectPatch,
  ProjectSort,
} from "$lib/kernel/types";
import {
  applyPatch,
  sectionCounts,
  selectProjects,
  type HomeSection,
  type HomeView,
} from "$lib/workbench/home";
import { clearKey, loadJson, saveJson } from "$lib/workbench/persist";

import { notice } from "./notice.svelte";
import { workspace } from "./workspace.svelte";

const PREFS_KEY = "manang.home.v1";
const api = projectApi(kernel);

function loadPrefs(): { view: HomeView; sort: ProjectSort } {
  const raw = loadJson(PREFS_KEY) as { view?: unknown; sort?: unknown } | null;
  return {
    view: raw?.view === "list" ? "list" : "grid",
    sort: raw?.sort === "name" || raw?.sort === "created" ? raw.sort : "modified",
  };
}

export class HomeState {
  /** Home shown over an open project. With no project it is always shown. */
  visible = $state(false);
  section = $state<HomeSection>("projects");
  query = $state("");
  sort = $state<ProjectSort>("modified");
  view = $state<HomeView>("grid");
  entries = $state.raw<readonly ProjectEntry[]>([]);
  loaded = $state(false);
  /** The New project dialog, with the language it starts on. */
  creating = $state<{ engine: Engine } | null>(null);
  /** The typed-path form, when there is no native folder picker. */
  typingPath = $state(false);
  /** Bumped per project when its thumbnail changes, so cards reload it. */
  readonly thumbVersions = new SvelteMap<string, number>();

  readonly shown = $derived(
    selectProjects(this.entries, {
      section: this.section,
      query: this.query,
      sort: this.sort,
    }),
  );
  readonly counts = $derived(sectionCounts(this.entries));

  /** Whether the Home surface is what the main area shows. */
  get showing(): boolean {
    return workspace.project === null || this.visible;
  }

  /** Restore the saved view and sort. Call once at start-up. */
  restore(): void {
    const prefs = loadPrefs();
    this.view = prefs.view;
    this.sort = prefs.sort;
  }

  #savePrefs(): void {
    saveJson(PREFS_KEY, { view: this.view, sort: this.sort });
  }

  setView(view: HomeView): void {
    this.view = view;
    this.#savePrefs();
  }

  setSort(sort: ProjectSort): void {
    this.sort = sort;
    this.#savePrefs();
  }

  setSection(section: HomeSection): void {
    this.section = section;
  }

  /** Forget saved preferences (used by tests). */
  resetPrefs(): void {
    clearKey(PREFS_KEY);
  }

  async load(): Promise<void> {
    try {
      this.entries = await api.list({
        include_archived: true,
        include_trashed: true,
      });
    } catch (error) {
      notice.error(`Could not read your projects: ${errorMessage(error)}`);
    } finally {
      this.loaded = true;
    }
  }

  /** Show Home over the open project (or as the only surface). */
  show(): void {
    this.visible = true;
    void this.load();
  }

  hide(): void {
    this.visible = false;
    this.query = "";
  }

  toggle(): void {
    if (workspace.project === null) return;
    if (this.visible) this.hide();
    else this.show();
  }

  openNew(engine: Engine = "typst"): void {
    this.creating = { engine };
  }

  /** Change a project's metadata: shown at once, undone if the kernel refuses. */
  async patch(path: string, patch: ProjectPatch): Promise<boolean> {
    const before = this.entries;
    this.entries = applyPatch(before, path, patch);
    try {
      const saved = await api.update(path, patch);
      this.entries = this.entries.map((e) => (e.path === path ? saved : e));
      const project = workspace.project;
      if (patch.name !== undefined && project?.root === path) {
        workspace.project = { ...project, name: saved.name };
      }
      return true;
    } catch (error) {
      this.entries = before;
      notice.error(`Could not update the project: ${errorMessage(error)}`);
      return false;
    }
  }

  /** Move to trash. The project is marked trashed with the current timestamp. */
  async trash(path: string): Promise<boolean> {
    const before = this.entries;
    const nowStamp = new SvelteDate().toISOString();
    this.entries = this.entries.map((e) =>
      e.path === path ? { ...e, trashed_at: nowStamp } : e,
    );
    try {
      const saved = await api.trashProject(path);
      this.entries = this.entries.map((e) => (e.path === path ? saved : e));
      return true;
    } catch (error) {
      this.entries = before;
      notice.error(`Could not move project to trash: ${errorMessage(error)}`);
      return false;
    }
  }

  /** Restore a trashed project. */
  async restoreProject(path: string): Promise<boolean> {
    const before = this.entries;
    this.entries = this.entries.map((e) =>
      e.path === path ? { ...e, trashed_at: null } : e,
    );
    try {
      const saved = await api.restoreProject(path);
      this.entries = this.entries.map((e) => (e.path === path ? saved : e));
      return true;
    } catch (error) {
      this.entries = before;
      notice.error(`Could not restore project: ${errorMessage(error)}`);
      return false;
    }
  }

  /** Purge a project from the registry, optionally deleting files to OS trash. */
  async purge(path: string, deleteFiles: boolean): Promise<boolean> {
    const before = this.entries;
    this.entries = before.filter((e) => e.path !== path);
    try {
      await api.purgeProject(path, deleteFiles);
      this.thumbVersions.delete(path);
      return true;
    } catch (error) {
      this.entries = before;
      notice.error(`Could not delete project: ${errorMessage(error)}`);
      return false;
    }
  }

  /** Empty trash (all trashed projects, or older than given days). */
  async emptyTrash(olderThanDays?: number): Promise<boolean> {
    try {
      await api.emptyTrash(olderThanDays);
      await this.load();
      return true;
    } catch (error) {
      notice.error(`Could not empty trash: ${errorMessage(error)}`);
      return false;
    }
  }

  /** Remove from the list. The folder and its files stay where they are. */
  async forget(path: string): Promise<void> {
    const before = this.entries;
    this.entries = before.filter((e) => e.path !== path);
    try {
      await api.forget(path);
      this.thumbVersions.delete(path);
    } catch (error) {
      this.entries = before;
      notice.error(`Could not remove the project: ${errorMessage(error)}`);
    }
  }

  /** A new thumbnail was stored for `path`. */
  noteThumbnail(path: string): void {
    this.thumbVersions.set(path, (this.thumbVersions.get(path) ?? 0) + 1);
    this.entries = this.entries.map((e) =>
      e.path === path && !e.has_thumbnail ? { ...e, has_thumbnail: true } : e,
    );
  }
}

export const home = new HomeState();
