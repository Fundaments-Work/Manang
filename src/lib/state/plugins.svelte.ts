/**
 * The Plugins view's state: the plugin list from the manager (`plugins.list`),
 * the filter and search, which plugin is open, and the install flow (pick,
 * inspect, consent, install, approve). All rules are in
 * `workbench/pluginManager.ts`; this holds the reactive copy and calls the
 * kernel. The list is refreshed from `plugins.*` events, so a plugin that
 * fails, appears or changes on disk shows without a reload.
 */

import { errorMessage, kernel, reportError } from "$lib/kernel";
import { hasNativeDialogs, pickFolder, pickPluginArchive } from "$lib/kernel/native";
import { MOCK_PLUGIN_SOURCES } from "$lib/kernel/mockPlugins";
import {
  pluginsApi,
  type PluginInspect,
  type PluginState,
} from "$lib/kernel/pluginsApi";
import {
  accessLines,
  canToggle,
  canUninstall,
  withPlugin,
  type AccessLine,
  type PluginFilter,
} from "$lib/workbench/pluginManager";

import { dialog } from "./dialog.svelte";
import { notice } from "./notice.svelte";

/** What the consent dialog asks the person to approve. */
export interface ConsentRequest {
  readonly name: string;
  readonly version: string;
  readonly description: string;
  readonly publisher: string | null;
  /** `install` of something new, `update` that asks for more, `review` of one already installed. */
  readonly mode: "install" | "update" | "review";
  readonly lines: readonly AccessLine[];
  readonly running: string | null;
}

/** Coalesces the burst of events one change produces into one refresh. */
const REFRESH_DELAY_MS = 80;

export class PluginsState {
  list = $state.raw<readonly PluginState[]>([]);
  warnings = $state.raw<readonly string[]>([]);
  loaded = $state(false);
  error = $state("");
  filter = $state<PluginFilter>("all");
  query = $state("");
  /** The plugin whose details are open. */
  selected = $state<string | null>(null);
  /** Plugins with a request in flight, so their controls rest meanwhile. */
  busy = $state.raw<readonly string[]>([]);
  consent = $state.raw<ConsentRequest | null>(null);
  installing = $state(false);

  #api = pluginsApi(kernel);
  #timer: ReturnType<typeof setTimeout> | undefined;
  #consentResolve: ((approved: boolean) => void) | undefined;

  async refresh(): Promise<void> {
    try {
      const result = await this.#api.list();
      this.list = result.plugins;
      this.warnings = result.warnings;
      this.error = "";
    } catch (e) {
      this.error = errorMessage(e);
      reportError("plugins could not be listed", e);
    } finally {
      this.loaded = true;
    }
  }

  /** Refresh soon: one event usually arrives with a few siblings. */
  scheduleRefresh(): void {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => void this.refresh(), REFRESH_DELAY_MS);
  }

  stop(): void {
    clearTimeout(this.#timer);
  }

  get(name: string): PluginState | undefined {
    return this.list.find((p) => p.name === name);
  }

  select(name: string | null): void {
    this.selected = this.selected === name ? null : name;
  }

  #apply(next: PluginState): void {
    this.list = withPlugin(this.list, next);
  }

  #mark(name: string, on: boolean): void {
    const rest = this.busy.filter((n) => n !== name);
    this.busy = on ? [...rest, name] : rest;
  }

  /** Run one request on a plugin; failures are logged and shown. */
  async #act(
    plugin: PluginState,
    what: string,
    run: () => Promise<PluginState | null>,
    done?: (next: PluginState | null) => string,
  ): Promise<void> {
    this.#mark(plugin.name, true);
    try {
      const next = await run();
      if (next !== null) this.#apply(next);
      if (done !== undefined) notice.show(done(next));
    } catch (e) {
      reportError(`${what} ${plugin.name} failed`, e);
      notice.error(`Could not ${what} ${plugin.name}: ${errorMessage(e)}`);
      void this.refresh();
    } finally {
      this.#mark(plugin.name, false);
    }
  }

  toggle(plugin: PluginState): Promise<void> {
    if (!canToggle(plugin)) return Promise.resolve();
    const off = plugin.enabled;
    return this.#act(
      plugin,
      off ? "disable" : "enable",
      () => (off ? this.#api.disable(plugin.name) : this.#api.enable(plugin.name)),
      () => `${off ? "Disabled" : "Enabled"} ${plugin.name}`,
    );
  }

  reload(plugin: PluginState): Promise<void> {
    return this.#act(
      plugin,
      "reload",
      () => this.#api.reload(plugin.name),
      (next) =>
        next?.status === "failed"
          ? `${plugin.name} still fails to load`
          : `Reloaded ${plugin.name}`,
    );
  }

  async uninstall(plugin: PluginState): Promise<void> {
    if (!canUninstall(plugin)) return;
    const answer = await dialog.ask({
      title: `Uninstall ${plugin.name}?`,
      message:
        "The plugin stops running and its files move to the plugins trash folder. Its approved access is forgotten.",
      actions: [{ id: "uninstall", label: "Uninstall" }],
    });
    if (answer !== "uninstall") return;
    await this.#act(
      plugin,
      "uninstall",
      async () => {
        await this.#api.uninstall(plugin.name);
        this.list = this.list.filter((p) => p.name !== plugin.name);
        if (this.selected === plugin.name) this.selected = null;
        return null;
      },
      () => `Uninstalled ${plugin.name}`,
    );
  }

  /** Approve a plugin that is already installed and waiting (a new version asking for more). */
  async review(plugin: PluginState): Promise<void> {
    const approved = await this.#askConsent({
      name: plugin.name,
      version: plugin.version,
      description: plugin.description,
      publisher: plugin.publisher ?? null,
      mode: plugin.update_pending ? "update" : "review",
      lines: accessLines(plugin.permissions, plugin.new_permissions),
      running: plugin.running_version ?? null,
    });
    if (!approved) return;
    await this.#act(
      plugin,
      "approve",
      () => this.#api.approve(plugin.name, plugin.permissions),
      () => `Approved ${plugin.name} ${plugin.version}`,
    );
  }

  /**
   * The install flow: choose, inspect, ask for consent, install, approve. A
   * plugin is never installed before its access has been read out.
   */
  async install(kind: "archive" | "folder"): Promise<void> {
    if (this.installing) return;
    this.installing = true;
    try {
      const path = await this.#choose(kind);
      if (path === null) return;
      let report: PluginInspect;
      try {
        report = await this.#api.inspect(path);
      } catch (e) {
        reportError("a plugin could not be read", e);
        notice.error(`Could not read that plugin: ${errorMessage(e)}`);
        return;
      }
      const updating = report.installed_version !== null;
      const approved = await this.#askConsent({
        name: report.name,
        version: report.version,
        description: report.description,
        publisher: report.publisher,
        mode: updating ? "update" : "install",
        lines: accessLines(report.permissions, report.new_permissions),
        running: report.installed_version,
      });
      if (!approved) return;
      try {
        const waiting = await this.#api.install(path);
        this.#apply(waiting);
        const ready = await this.#api.approve(report.name, report.permissions);
        this.#apply(ready);
        this.selected = report.name;
        notice.show(`Installed ${report.name} ${report.version}`);
      } catch (e) {
        reportError(`installing ${report.name} failed`, e);
        notice.error(`Could not install ${report.name}: ${errorMessage(e)}`);
        void this.refresh();
      }
    } finally {
      this.installing = false;
    }
  }

  async #choose(kind: "archive" | "folder"): Promise<string | null> {
    // A browser has no file system to pick from: the dev kernel offers one
    // canned plugin so the whole flow can be tried.
    if (!hasNativeDialogs()) return Object.keys(MOCK_PLUGIN_SOURCES)[0] ?? null;
    try {
      return kind === "archive"
        ? await pickPluginArchive("Install a plugin")
        : await pickFolder("Install a plugin from a folder");
    } catch (e) {
      reportError("the plugin picker failed", e);
      notice.error(`Could not open the file picker: ${errorMessage(e)}`);
      return null;
    }
  }

  #askConsent(request: ConsentRequest): Promise<boolean> {
    this.#consentResolve?.(false);
    this.consent = request;
    return new Promise((resolve) => {
      this.#consentResolve = resolve;
    });
  }

  /** The consent dialog's answer: approve everything listed, or nothing. */
  answerConsent(approved: boolean): void {
    const resolve = this.#consentResolve;
    this.#consentResolve = undefined;
    this.consent = null;
    resolve?.(approved);
  }
}

export const plugins = new PluginsState();
