<script lang="ts">
  import { onMount, untrack } from "svelte";

  import { share } from "$lib/state/share.svelte";
  import { statusLine, unsavedHint } from "$lib/workbench/share";

  /**
   * Share and Get updates: one address field on first use; after that a status
   * line and two actions. Only the action the status suggests is drawn lime.
   * The state lives in `share`, so the Versions view and the Share popover show
   * the same thing. `embedded` drops the view's own padding and rule (the
   * popover supplies them).
   */
  interface Props {
    embedded?: boolean;
    /** Do not draw the suggested action lime (another lime action is in view). */
    quiet?: boolean;
  }

  const { embedded = false, quiet = false }: Props = $props();

  const LIME = "border-accent bg-accent text-accent-ink";

  $effect(() => {
    const root = share.root;
    untrack(() => void share.load(root));
  });
  onMount(() => share.listen());

  const lime = (which: "share" | "get-updates"): string =>
    !quiet && share.suggestion === which ? LIME : "";
</script>

<section
  class="space-y-2 text-xs {embedded ? '' : 'border-b border-line px-3 py-2'}"
  aria-label={embedded ? "Share versions" : "Share and get updates"}
>
  {#if share.connected}
    <p class="m-0 flex items-baseline justify-between gap-2">
      <span class="text-text" aria-live="polite" data-testid="share-status"
        >{statusLine(share.status)}</span
      >
      <button
        type="button"
        class="inset-focus shrink-0 text-[10px] text-text-60 underline hover:text-text"
        onclick={() => share.change()}
      >
        Change address
      </button>
    </p>
    <p
      class="m-0 truncate font-mono text-[10px] text-text-muted"
      title={share.remote?.url ?? ""}
    >
      {share.remote?.url}
    </p>
    {#if unsavedHint(share.status) !== ""}
      <p class="m-0 text-[10px] text-text-muted">{unsavedHint(share.status)}</p>
    {/if}
    <div class="flex gap-2">
      <button
        type="button"
        class="mng-btn h-6 px-2 text-xs {lime('share')}"
        disabled={share.busy}
        onclick={() => void share.run("share")}
      >
        Share
      </button>
      <button
        type="button"
        class="mng-btn h-6 px-2 text-xs {lime('get-updates')}"
        disabled={share.busy}
        onclick={() => void share.run("get-updates")}
      >
        Get updates
      </button>
    </div>
  {:else}
    <label class="block text-text-60" for="share-address{embedded ? '-pop' : ''}">
      {share.remote?.url != null
        ? "Address of the shared project"
        : "Share this project with others"}
    </label>
    <div class="flex items-center gap-1">
      <input
        id="share-address{embedded ? '-pop' : ''}"
        bind:value={share.address}
        type="text"
        spellcheck="false"
        autocomplete="off"
        placeholder="https://github.com/you/project.git"
        class="mng-field flex-1 text-xs"
        aria-invalid={share.problem !== ""}
        aria-describedby={share.problem !== "" ? "share-problem" : undefined}
        onkeydown={(event) => {
          if (event.key === "Enter") void share.connect();
        }}
      />
      <button
        type="button"
        class="mng-btn h-6 px-2 text-xs"
        disabled={share.busy}
        onclick={() => void share.connect()}
      >
        Connect
      </button>
    </div>
    {#if share.problem !== ""}
      <p id="share-problem" class="m-0 text-text" role="alert">{share.problem}</p>
    {:else if share.remote?.url == null}
      <p class="m-0 text-[10px] text-text-muted">
        Paste the address from your host (GitHub, GitLab or your own server). Sign-in
        uses the credentials your computer already has.
      </p>
    {/if}
  {/if}
</section>
