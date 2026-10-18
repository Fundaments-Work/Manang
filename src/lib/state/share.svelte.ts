/**
 * Sharing versions with co-authors: the connected address, how the shared copy
 * compares with this one, and the actions. One store serves the Versions view
 * and the Share popover, so both always agree, and the Share button can tell
 * when updates are waiting without either being open.
 */

import { kernel, reportError } from "$lib/kernel";
import { gitApi, type GitRemoteInfo, type GitSyncStatus } from "$lib/kernel/api";
import { gitActionableError, parseGitError } from "$lib/workbench/git";
import { addressProblem, shareSuggestion } from "$lib/workbench/share";

import { getUpdates, shareVersions } from "./versionActions";
import { workspace } from "./workspace.svelte";

const api = gitApi(kernel);

export class ShareState {
  remote = $state.raw<GitRemoteInfo | null>(null);
  status = $state.raw<GitSyncStatus | null>(null);
  address = $state("");
  editing = $state(false);
  problem = $state("");
  busy = $state(false);

  #generation = 0;
  #listening = false;

  get root(): string {
    return workspace.project?.root ?? "";
  }

  /** An address is saved and not being edited. */
  get connected(): boolean {
    return this.remote?.url != null && !this.editing;
  }

  get suggestion(): ReturnType<typeof shareSuggestion> {
    return shareSuggestion(this.status);
  }

  /** Others shared versions this project does not have yet. */
  get updatesWaiting(): boolean {
    return this.suggestion === "get-updates";
  }

  /** Follow the project's git changes (once). */
  listen(): void {
    if (this.#listening) return;
    this.#listening = true;
    void kernel.onEvent((event) => {
      if (event.topic === "git.changed") void this.load(this.root);
    });
  }

  async load(root: string): Promise<void> {
    if (root === "") return;
    const mine = ++this.#generation;
    try {
      const [info, sync] = await Promise.all([
        api.remoteGet({ root }),
        api.syncStatus({ root }),
      ]);
      if (mine !== this.#generation) return;
      this.remote = info;
      this.status = sync;
    } catch (error) {
      if (mine !== this.#generation) return;
      reportError("reading the share status", error);
    }
  }

  async connect(): Promise<void> {
    this.problem = addressProblem(this.address);
    if (this.problem !== "" || this.busy) return;
    this.busy = true;
    try {
      this.remote = await api.remoteSet({ root: this.root, url: this.address.trim() });
      this.editing = false;
      this.address = "";
      this.status = await api.syncStatus({ root: this.root });
    } catch (error) {
      reportError("saving the shared address", error);
      const parsed = parseGitError(error);
      this.problem = gitActionableError(parsed.code, parsed.message);
    } finally {
      this.busy = false;
    }
  }

  /** Share, or Get updates, then re-read the status. */
  async run(action: "share" | "get-updates"): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    try {
      await (action === "share" ? shareVersions() : getUpdates());
    } finally {
      await this.load(this.root);
      this.busy = false;
    }
  }

  change(): void {
    this.address = this.remote?.url ?? "";
    this.problem = "";
    this.editing = true;
  }
}

export const share = new ShareState();
