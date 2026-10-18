<script lang="ts">
  import VersionDiff from "$lib/components/VersionDiff.svelte";
  import { reportError, kernel } from "$lib/kernel";
  import { projectApi, type TextDiff } from "$lib/kernel/api";
  import { closeTab, saveAs } from "$lib/state/actions";
  import { documents } from "$lib/state/documents.svelte";
  import { figures } from "$lib/state/figures.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { externalMessage } from "$lib/workbench/external";
  import { basename } from "$lib/workbench/paths";

  /**
   * The non-modal bar above an editor whose file changed on disk while it had
   * unsaved edits, or went away. It never decides for the person: Keep my
   * version, Load disk version and Compare for a change; Save again, Save as
   * and Close for a deleted file. Nothing is overwritten until a button is used.
   */
  interface Props {
    path: string;
    kind: "document" | "figure";
  }

  const { path, kind }: Props = $props();

  const session = $derived(kind === "document" ? documents.get(path) : undefined);
  const figure = $derived(kind === "figure" ? figures.get(path) : undefined);
  const status = $derived(session?.external ?? figure?.external ?? null);
  const dirty = $derived(session?.dirty ?? figure?.dirty ?? false);
  const comparing = $derived(session?.comparing === true && status === "changed");

  let diff = $state.raw<TextDiff | null>(null);

  // The comparison is the disk against the text in the editor, fetched when opened.
  $effect(() => {
    if (!comparing || session === undefined) {
      diff = null;
      return;
    }
    const text = session.text();
    projectApi(kernel)
      .diffText(path, text)
      .then((d) => (diff = d))
      .catch((error: unknown) => reportError("externalChange.compare", error));
  });

  function close(): void {
    const tab = tabs.tabs.find((t) => t.path === path);
    if (tab !== undefined) void closeTab(tab.id);
  }

  function keepMine(): void {
    if (kind === "document") documents.keepMine(path);
    else figures.keepMine(path);
  }

  function loadDisk(): void {
    if (kind === "document") documents.loadDisk(path);
    else void figures.loadDisk(path);
  }

  function saveAgain(): void {
    if (kind === "document") void documents.recreate(path);
    else void figures.save(path);
  }
</script>

{#if status !== null}
  <div
    class="border-b border-line bg-surface text-xs"
    role="status"
    data-testid="external-change-bar"
    data-status={status}
  >
    <div class="flex flex-wrap items-center gap-x-3 gap-y-2 px-3 py-2">
      <p class="m-0 min-w-0 flex-1 text-text-60">
        {externalMessage(basename(path), status, dirty)}
        {#if status === "changed"}
          Your unsaved edits are kept.
        {/if}
      </p>
      {#if status === "changed"}
        <button type="button" class="mng-btn" onclick={keepMine}>Keep my version</button
        >
        <button type="button" class="mng-btn" onclick={loadDisk}
          >Load disk version</button
        >
        {#if kind === "document" && session !== undefined}
          <button
            type="button"
            class="mng-btn"
            aria-pressed={session.comparing}
            onclick={() => (session.comparing = !session.comparing)}>Compare</button
          >
        {/if}
      {:else}
        <button type="button" class="mng-btn" onclick={saveAgain}>Save again</button>
        {#if kind === "document"}
          <button type="button" class="mng-btn" onclick={() => void saveAs(path)}
            >Save as</button
          >
        {/if}
        <button type="button" class="mng-btn" onclick={close}>Close</button>
      {/if}
    </div>
    {#if comparing && diff !== null}
      <p class="m-0 border-t border-line px-3 py-1.5 text-[11px] text-text-muted">
        Added lines are in your text; removed lines are what is on disk now.
      </p>
      <VersionDiff {diff} />
    {/if}
  </div>
{/if}
