/**
 * Saving by itself, as the `files.autosave` setting says: `after-delay` writes
 * a modified document a second after the last edit, `on-focus-loss` writes the
 * modified ones when the window loses focus, `off` never does. It never writes
 * over a file that changed on disk (see `canAutosave`), and a conflict shows
 * the usual bar above the editor rather than a dialog.
 */

import { AutosaveScheduler, autosaveMode, canAutosave } from "$lib/workbench/autosave";

import { documents } from "./documents.svelte";
import { settings } from "./settings.svelte";
import { workspace } from "./workspace.svelte";

class AutosaveStore {
  readonly #scheduler = new AutosaveScheduler((path) => void this.#save(path));

  get mode() {
    return autosaveMode(settings.value("files.autosave"));
  }

  /** Start following edits. Call once at boot. */
  start(): void {
    documents.onChange((path) => {
      if (this.mode === "after-delay") this.#scheduler.touch(path);
    });
    documents.onSave((path) => this.#scheduler.cancel(path));
    window.addEventListener("blur", () => this.#focusLost());
    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState === "hidden") this.#focusLost();
    });
  }

  async #save(path: string): Promise<void> {
    if (workspace.project === null || this.mode === "off") return;
    if (!canAutosave(documents.get(path))) return;
    await documents.save(path, { silent: true });
  }

  #focusLost(): void {
    if (this.mode !== "on-focus-loss") return;
    for (const path of documents.dirtyPaths) void this.#save(path);
  }
}

export const autosave = new AutosaveStore();
