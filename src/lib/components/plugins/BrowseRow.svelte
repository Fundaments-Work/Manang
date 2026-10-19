<script lang="ts">
  import PluginIcon from "$lib/components/plugins/PluginIcon.svelte";
  import type { BrowseEntry } from "$lib/kernel/marketApi";
  import { market } from "$lib/state/market.svelte";
  import { formatCount } from "$lib/workbench/market";

  /**
   * One registry plugin in the Browse list: name, author, latest version, a
   * short description, tags, whether it is installed and its download count.
   * Clicking opens its page; Install reads the access out for consent first.
   */

  interface Props {
    entry: BrowseEntry;
  }

  const { entry }: Props = $props();

  const working = $derived(market.working.includes(entry.id));
  const progress = $derived(market.progress[entry.id]);
</script>

<li
  class="border-b border-line-subtle last:border-b-0"
  data-registry-plugin={entry.id}
  data-installed={entry.installed === null ? "no" : "yes"}
>
  <div class="flex items-start gap-2 px-3 py-2">
    <button
      type="button"
      class="inset-focus flex min-w-0 flex-1 items-start gap-2 text-left"
      data-open-page
      aria-label="Open {entry.name}"
      onclick={() => market.openPage(entry.id)}
    >
      <PluginIcon name={entry.name} />
      <span class="min-w-0 flex-1">
        <span class="flex items-baseline gap-2">
          <span class="truncate text-xs text-text">{entry.name}</span>
          <span class="shrink-0 font-mono text-[10px] text-text-muted"
            >{entry.latest}</span
          >
        </span>
        <span class="mt-0.5 flex flex-wrap items-center gap-x-1.5 text-[11px]">
          <span class="truncate text-text-60">{entry.author}</span>
          {#if entry.downloads !== null}
            <span class="text-text-muted" aria-hidden="true">·</span>
            <span class="text-text-60" data-downloads title="Downloads"
              >{formatCount(entry.downloads)} downloads</span
            >
          {/if}
        </span>
        <span class="mt-0.5 line-clamp-2 block text-[11px] leading-snug text-text-60">
          {entry.description}
        </span>
        {#if entry.tags.length > 0}
          <span class="mt-1 flex flex-wrap gap-1">
            {#each entry.tags.slice(0, 3) as tag (tag)}
              <span class="border border-line px-1 text-[10px] text-text-60">{tag}</span
              >
            {/each}
          </span>
        {/if}
      </span>
    </button>
    <span class="mt-0.5 flex shrink-0 flex-col items-end gap-1 text-[11px]">
      {#if entry.installed === null}
        <button
          type="button"
          class="mng-btn text-[11px]"
          disabled={working || !entry.compatible}
          title={entry.compatible ? undefined : "Needs a newer Manang"}
          data-install-registry
          onclick={() => void market.installLatest(entry)}
        >
          {working ? "Installing" : "Install"}
        </button>
      {:else if entry.installed.update_available}
        <span class="text-text" data-state-label>Update available</span>
      {:else}
        <span class="text-text-60" data-state-label>Installed</span>
      {/if}
      {#if progress !== undefined && progress.total > 0}
        <span class="font-mono text-[10px] text-text-muted" role="status"
          >{Math.round((progress.received / progress.total) * 100)}%</span
        >
      {/if}
    </span>
  </div>
</li>
