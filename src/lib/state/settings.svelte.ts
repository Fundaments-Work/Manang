/**
 * Settings, as the settings plugin reports them: the schema (core settings and
 * every plugin's, declared in their manifests) and the effective value of each.
 *
 * One store for the whole shell. The Settings page edits through it, and the
 * preferences applier (`preferences.svelte.ts`) reads from it, so a change made
 * anywhere (the page, the settings file on disk, another window) reaches the
 * editor through the same `settings.changed` event.
 */

import { kernel } from "$lib/kernel";
import { settingsApi, type SettingDef, type SettingScope } from "$lib/kernel/api";

const api = settingsApi(kernel);

export type SettingSource = "default" | "user" | "project";

interface Entry {
  readonly value: unknown;
  readonly source: SettingSource;
}

function message(error: unknown, fallback: string): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "message" in error)
    return String((error as { message: unknown }).message);
  return fallback;
}

export class SettingsState {
  defs = $state.raw<readonly SettingDef[]>([]);
  values = $state.raw<Readonly<Record<string, Entry>>>({});
  loaded = $state(false);
  error = $state("");
  /** A setting that only applies after a restart was changed this session. */
  restartPending = $state(false);

  #listening = false;
  #root: string | null | undefined = undefined;

  /** The effective value, or the schema default, or undefined. */
  value(id: string): unknown {
    const entry = this.values[id];
    if (entry !== undefined) return entry.value;
    return this.defs.find((d) => d.id === id)?.default;
  }

  source(id: string): SettingSource {
    return this.values[id]?.source ?? "default";
  }

  /**
   * Load the schema and values, selecting the project layer for `root`.
   * Cheap to call again: it only re-reads when the project changed or `force`.
   */
  async load(root: string | null, force = false): Promise<void> {
    this.#listen();
    if (!force && this.loaded && root === this.#root) return;
    this.#root = root;
    try {
      await api.useProject(root);
      const [schema, all] = await Promise.all([api.schema(), api.get()]);
      this.defs = schema.settings;
      this.values = all.values;
      this.error = "";
    } catch (error) {
      this.error = message(error, "The settings plugin did not answer.");
    } finally {
      this.loaded = true;
    }
  }

  /** Write a value to one layer. Resolves to an error message, or "" on success. */
  async set(id: string, value: unknown, scope: SettingScope): Promise<string> {
    try {
      const res = await api.set({ id, value, scope });
      this.#put(id, { value: res.value, source: res.source });
      return "";
    } catch (error) {
      return message(error, "The setting could not be saved.");
    }
  }

  /** Remove a value from one layer, falling back to the next. */
  async reset(id: string, scope: SettingScope): Promise<string> {
    try {
      const res = await api.reset({ id, scope });
      this.#put(id, { value: res.value, source: res.source });
      return "";
    } catch (error) {
      return message(error, "The setting could not be reset.");
    }
  }

  async openFile(scope: "user" | "project" | "keybindings"): Promise<string> {
    const res = await api.openFile(scope);
    return res.path;
  }

  #put(id: string, entry: Entry): void {
    const before = this.values[id];
    this.values = { ...this.values, [id]: entry };
    if (
      before !== undefined &&
      JSON.stringify(before.value) !== JSON.stringify(entry.value) &&
      this.defs.find((d) => d.id === id)?.requires_restart === true
    )
      this.restartPending = true;
  }

  #listen(): void {
    if (this.#listening) return;
    this.#listening = true;
    void kernel.onEvent((event) => {
      if (event.topic === "settings.changed") {
        const data = event.payload as
          { id?: unknown; value?: unknown; scope?: unknown } | null | undefined;
        if (data !== null && data !== undefined && typeof data.id === "string") {
          const source: SettingSource =
            data.scope === "user" || data.scope === "project" ? data.scope : "default";
          this.#put(data.id, { value: data.value, source });
        }
      } else if (event.topic === "settings.registered") {
        // A plugin (for example a WebAssembly plugin loaded later) added settings.
        void this.load(this.#root ?? null, true);
      }
    });
  }
}

export const settings = new SettingsState();
