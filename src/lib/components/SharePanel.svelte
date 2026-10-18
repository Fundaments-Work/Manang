<script lang="ts">
  import { onMount, untrack } from "svelte";

  import { kernel, reportError } from "$lib/kernel";
  import { gitApi, type GitRemoteInfo, type GitSyncStatus } from "$lib/kernel/api";
  import { getUpdates, shareVersions } from "$lib/state/versionActions";
  import { workspace } from "$lib/state/workspace.svelte";
  import { gitActionableError, parseGitError } from "$lib/workbench/git";
  import {
    addressProblem,
    shareSuggestion,
    statusLine,
    unsavedHint,
  } from "$lib/workbench/share";

  /**
   * Share and Get updates, in the Versions view. One address field on first
   * use; after that a status line and two actions. Only the action the status
   * suggests is drawn lime.
   */

  const api = gitApi(kernel);
  const LIME = "border-accent bg-accent text-accent-ink";

  const projectRoot = $derived(workspace.project?.root ?? "");

  let remote = $state.raw<GitRemoteInfo | null>(null);
  let status = $state.raw<GitSyncStatus | null>(null);
  let address = $state("");
  let editing = $state(false);
  let problem = $state("");
  let busy = $state(false);
  let generation = 0;

  const connected = $derived(remote?.url != null && !editing);
  const suggestion = $derived(shareSuggestion(status));

  async function load(root: string): Promise<void> {
    if (root === "") return;
    const mine = ++generation;
    try {
      const [info, sync] = await Promise.all([
        api.remoteGet({ root }),
        api.syncStatus({ root }),
      ]);
      if (mine !== generation) return;
      remote = info;
      status = sync;
    } catch (error) {
      if (mine !== generation) return;
      reportError("reading the share status", error);
    }
  }

  $effect(() => {
    const root = projectRoot;
    untrack(() => void load(root));
  });

  onMount(() => {
    const unsubscribe = kernel.onEvent((event) => {
      if (event.topic === "git.changed") void load(projectRoot);
    });
    return () => {
      void unsubscribe.then((off) => off());
    };
  });

  async function connect(): Promise<void> {
    problem = addressProblem(address);
    if (problem !== "" || busy) return;
    busy = true;
    try {
      remote = await api.remoteSet({ root: projectRoot, url: address.trim() });
      editing = false;
      address = "";
      status = await api.syncStatus({ root: projectRoot });
    } catch (error) {
      reportError("saving the shared address", error);
      const parsed = parseGitError(error);
      problem = gitActionableError(parsed.code, parsed.message);
    } finally {
      busy = false;
    }
  }

  async function run(action: () => Promise<boolean>): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      await action();
    } finally {
      await load(projectRoot);
      busy = false;
    }
  }

  function change(): void {
    address = remote?.url ?? "";
    problem = "";
    editing = true;
  }
</script>

<section
  class="space-y-2 border-b border-line px-3 py-2 text-xs"
  aria-label="Share and get updates"
>
  {#if connected}
    <p class="m-0 flex items-baseline justify-between gap-2">
      <span class="text-text" aria-live="polite" data-testid="share-status"
        >{statusLine(status)}</span
      >
      <button
        type="button"
        class="inset-focus shrink-0 text-[10px] text-text-60 underline hover:text-text"
        onclick={change}
      >
        Change address
      </button>
    </p>
    <p class="m-0 truncate font-mono text-[10px] text-text-muted" title={remote?.url ?? ""}>
      {remote?.url}
    </p>
    {#if unsavedHint(status) !== ""}
      <p class="m-0 text-[10px] text-text-muted">{unsavedHint(status)}</p>
    {/if}
    <div class="flex gap-2">
      <button
        type="button"
        class="mng-btn h-6 px-2 text-xs {suggestion === 'share' ? LIME : ''}"
        disabled={busy}
        onclick={() => void run(shareVersions)}
      >
        Share
      </button>
      <button
        type="button"
        class="mng-btn h-6 px-2 text-xs {suggestion === 'get-updates' ? LIME : ''}"
        disabled={busy}
        onclick={() => void run(getUpdates)}
      >
        Get updates
      </button>
    </div>
  {:else}
    <label class="block text-text-60" for="share-address">
      {remote?.url != null
        ? "Address of the shared project"
        : "Share this project with others"}
    </label>
    <div class="flex items-center gap-1">
      <input
        id="share-address"
        bind:value={address}
        type="text"
        spellcheck="false"
        autocomplete="off"
        placeholder="https://github.com/you/project.git"
        class="mng-field flex-1 text-xs"
        aria-invalid={problem !== ""}
        aria-describedby={problem !== "" ? "share-problem" : undefined}
        onkeydown={(event) => {
          if (event.key === "Enter") void connect();
        }}
      />
      <button
        type="button"
        class="mng-btn h-6 px-2 text-xs"
        disabled={busy}
        onclick={() => void connect()}
      >
        Connect
      </button>
    </div>
    {#if problem !== ""}
      <p id="share-problem" class="m-0 text-text" role="alert">{problem}</p>
    {:else if remote?.url == null}
      <p class="m-0 text-[10px] text-text-muted">
        Paste the address from your host (GitHub, GitLab or your own server). Sign-in
        uses the credentials your computer already has.
      </p>
    {/if}
  {/if}
</section>
