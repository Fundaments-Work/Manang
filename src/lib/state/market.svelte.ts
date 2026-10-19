/**
 * The plugin marketplace's state: which tab of the Plugins view is open, the
 * registry list and why it may be missing, the updates found, and the install
 * and update flows (read the access out, ask, download, verify, install). The
 * rules are in `workbench/market.ts`; the work is the kernel's
 * (`plugins.browse`, `plugins.install-from-registry`, ...). Nothing is
 * installed without the consent dialog being answered yes.
 */

import { errorMessage, kernel, reportError } from "$lib/kernel";
import {
  marketApi,
  type BrowseEntry,
  type BrowseResult,
  type BrowseSort,
  type InstallProgress,
  type RegistryAccess,
  type RegistryStatus,
  type UpdateInfo,
} from "$lib/kernel/marketApi";
import type { PluginAccess } from "$lib/kernel/pluginsApi";
import { accessLines } from "$lib/workbench/pluginManager";
import {
  browseProblem,
  pluginTabPath,
  updateConsentLines,
  type BrowseProblem,
} from "$lib/workbench/market";

import { notice } from "./notice.svelte";
import { plugins } from "./plugins.svelte";
import { tabs } from "./tabs.svelte";

export type PluginsTab = "installed" | "browse";

/** Why the list is missing, in the words of `workbench/market.ts`. */
export interface BrowseFailure {
  readonly problem: BrowseProblem;
  readonly message: string;
}

const HOUR_MS = 60 * 60 * 1000;
/** The first background update check waits for the app to settle. */
const FIRST_CHECK_MS = 8000;

const asAccess = (a: RegistryAccess): PluginAccess => ({
  filesystem: a.filesystem,
  network: a.network,
  buffer: a.buffer,
});

export class MarketState {
  tab = $state<PluginsTab>("installed");
  sort = $state<BrowseSort>("name");
  /** The tag the Browse list is narrowed to. */
  tag = $state<string | null>(null);

  result = $state.raw<BrowseResult | null>(null);
  failure = $state.raw<BrowseFailure | null>(null);
  loading = $state(false);
  status = $state.raw<RegistryStatus | null>(null);
  updates = $state.raw<readonly UpdateInfo[]>([]);
  /** Registry installs under way, by plugin id. */
  progress = $state.raw<Readonly<Record<string, InstallProgress>>>({});
  /** Ids with an install or update running. */
  working = $state.raw<readonly string[]>([]);

  readonly updateCount = $derived(this.updates.length);

  #api = marketApi(kernel);
  #stop: (() => void) | undefined;
  #timers: ReturnType<typeof setTimeout>[] = [];
  #interval: ReturnType<typeof setInterval> | undefined;

  /** Start listening and schedule the background update check. Safe to call twice. */
  start(): void {
    if (this.#stop !== undefined) return;
    this.#stop = () => undefined;
    void kernel
      .onEvent((event) => {
        if (event.topic === "plugins.install-progress") {
          const p = event.payload as InstallProgress;
          // Events arrive a moment after the call: ignore a late one.
          if (this.working.includes(p.id))
            this.progress = { ...this.progress, [p.id]: p };
        }
      })
      .then((stop) => {
        if (this.#stop === undefined) stop();
        else this.#stop = stop;
      });
    this.#timers.push(setTimeout(() => void this.checkUpdates(true), FIRST_CHECK_MS));
    // The backend answers a background check from its cache within a day, so
    // an hourly tick costs nothing and catches a long-running session.
    this.#interval = setInterval(() => void this.checkUpdates(true), HOUR_MS);
  }

  stop(): void {
    this.#stop?.();
    this.#stop = undefined;
    for (const t of this.#timers) clearTimeout(t);
    this.#timers = [];
    clearInterval(this.#interval);
    this.#interval = undefined;
  }

  /** Read the registry list (`refresh` asks the registry; otherwise the cache may answer). */
  async loadBrowse(refresh: boolean): Promise<void> {
    this.loading = true;
    try {
      const result = await this.#api.browse(refresh);
      this.result = result;
      this.failure = null;
    } catch (e) {
      const message = errorMessage(e);
      this.failure = { problem: browseProblem(message), message };
      // An old list under a failure would look current: show the failure alone.
      this.result = null;
      reportError("the plugin list could not be loaded", e);
    } finally {
      this.loading = false;
    }
  }

  async loadStatus(): Promise<void> {
    try {
      this.status = await this.#api.status();
    } catch (e) {
      reportError("the plugin registry status could not be read", e);
    }
  }

  /** Look for updates; never installs. Silent on failure (a quiet background job). */
  async checkUpdates(background: boolean): Promise<void> {
    try {
      const check = await this.#api.checkUpdates(background);
      this.updates = check.updates;
    } catch (e) {
      reportError("the plugin update check failed", e);
    }
  }

  updateFor(id: string): UpdateInfo | undefined {
    return this.updates.find((u) => u.id === id);
  }

  /** Open (or focus) a plugin's page in the editor area. */
  openPage(id: string): void {
    tabs.open(pluginTabPath(id));
  }

  #mark(id: string, on: boolean): void {
    const rest = this.working.filter((n) => n !== id);
    this.working = on ? [...rest, id] : rest;
    if (!on) {
      const left = { ...this.progress };
      delete left[id];
      this.progress = left;
    }
  }

  /**
   * Install or update `id` from the registry: show the access the verified
   * list gives for `version`, ask, then download and install. Resolves true
   * when the plugin is installed. The kernel checks the plugin's own manifest
   * asks for exactly what was shown.
   */
  async install(
    id: string,
    options: {
      readonly name: string;
      readonly version: string;
      readonly description: string;
      readonly author: string;
      readonly permissions: RegistryAccess;
      /** Set for an update: the version running now and what is new. */
      readonly update?: { readonly running: string; readonly added: RegistryAccess };
      readonly pinVersion?: boolean;
    },
  ): Promise<boolean> {
    if (this.working.includes(id)) return false;
    const update = options.update;
    const approved = await plugins.askConsent({
      name: options.name,
      version: options.version,
      description: options.description,
      publisher: options.author,
      mode: update === undefined ? "install" : "update",
      lines:
        update === undefined
          ? accessLines(asAccess(options.permissions))
          : updateConsentLines(asAccess(options.permissions), asAccess(update.added)),
      running: update?.running ?? null,
    });
    if (!approved) return false;
    this.#mark(id, true);
    try {
      await this.#api.install(
        id,
        options.pinVersion === true ? options.version : undefined,
      );
      notice.show(
        update === undefined
          ? `Installed ${options.name} ${options.version}`
          : `Updated ${options.name} to ${options.version}`,
      );
      await plugins.refresh();
      void this.checkUpdates(true);
      if (this.result !== null) void this.loadBrowse(false);
      return true;
    } catch (e) {
      reportError(`installing ${id} failed`, e);
      notice.error(`Could not install ${options.name}: ${errorMessage(e)}`);
      void plugins.refresh();
      return false;
    } finally {
      this.#mark(id, false);
    }
  }

  /** Install the latest version of a Browse entry (its access is read from the registry first). */
  async installLatest(entry: BrowseEntry): Promise<boolean> {
    let detail;
    try {
      detail = await this.#api.detail(entry.id);
    } catch (e) {
      reportError(`reading ${entry.id} failed`, e);
      notice.error(`Could not read ${entry.name}: ${errorMessage(e)}`);
      return false;
    }
    const version = detail.versions.find((v) => v.version === entry.latest);
    if (version === undefined) {
      this.openPage(entry.id);
      return false;
    }
    return this.install(entry.id, {
      name: entry.name,
      version: entry.latest,
      description: entry.description,
      author: entry.author,
      permissions: version.permissions,
    });
  }

  /** Update every plugin with an update, one consent each. A refusal skips that one. */
  async updateAll(): Promise<void> {
    for (const u of [...this.updates]) {
      if (!u.compatible) continue;
      let detail;
      try {
        detail = await this.#api.detail(u.id);
      } catch (e) {
        notice.error(`Could not update ${u.name}: ${errorMessage(e)}`);
        continue;
      }
      const version = detail.versions.find((v) => v.version === u.latest);
      if (version === undefined) continue;
      await this.install(u.id, {
        name: u.name,
        version: u.latest,
        description: detail.plugin.description,
        author: detail.plugin.author,
        permissions: version.permissions,
        update: { running: u.installed, added: u.new_permissions },
      });
    }
  }
}

export const market = new MarketState();
