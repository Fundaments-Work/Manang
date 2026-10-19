<script lang="ts">
  import { onMount } from "svelte";

  import BrowseRow from "$lib/components/plugins/BrowseRow.svelte";
  import PluginRow from "$lib/components/plugins/PluginRow.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { kernel } from "$lib/kernel";
  import { market, type PluginsTab } from "$lib/state/market.svelte";
  import { plugins } from "$lib/state/plugins.svelte";
  import {
    BROWSE_SORTS,
    freshnessNotice,
    isoDate,
    problemWording,
    visibleBrowse,
  } from "$lib/workbench/market";
  import {
    PLUGIN_FILTERS,
    filterCounts,
    visiblePlugins,
  } from "$lib/workbench/pluginManager";

  /**
   * Plugins, as a compact list like an extensions view. Two tabs: Installed
   * (everything Manang runs as a plugin, built in and installed) and Browse
   * (the signed plugin registry). One search field serves both. A click on a
   * row opens the plugin's page in the editor area, where the details, the
   * permissions and the actions are. The list follows the manager's
   * `plugins.*` events, so a plugin that fails or appears shows without a
   * reload; updates are looked for when this opens, never installed by it.
   */

  const shown = $derived(visiblePlugins(plugins.list, plugins.filter, plugins.query));
  const counts = $derived(filterCounts(plugins.list));
  const browseShown = $derived(
    visibleBrowse(market.result?.plugins ?? [], plugins.query, market.tag, market.sort),
  );
  const notice = $derived(
    market.result === null ? null : freshnessNotice(market.result.freshness),
  );
  const wording = $derived(
    market.failure === null
      ? null
      : problemWording(market.failure.problem, market.failure.message),
  );

  const TABS: readonly { readonly id: PluginsTab; readonly label: string }[] = [
    { id: "installed", label: "Installed" },
    { id: "browse", label: "Browse" },
  ];

  function pick(tab: PluginsTab): void {
    market.tab = tab;
    if (tab === "browse" && market.result === null && market.failure === null)
      void market.loadBrowse(true);
  }

  function onTabKeys(event: KeyboardEvent): void {
    const at = TABS.findIndex((t) => t.id === market.tab);
    const next =
      event.key === "ArrowRight"
        ? TABS[(at + 1) % TABS.length]
        : event.key === "ArrowLeft"
          ? TABS[(at + TABS.length - 1) % TABS.length]
          : undefined;
    if (next === undefined) return;
    event.preventDefault();
    pick(next.id);
    (event.currentTarget as HTMLElement)
      .querySelector<HTMLElement>(`[data-tab="${next.id}"]`)
      ?.focus();
  }

  function refresh(): void {
    void plugins.refresh();
    if (market.tab === "browse") void market.loadBrowse(true);
    void market.checkUpdates(false);
  }

  onMount(() => {
    void plugins.refresh();
    void market.checkUpdates(false);
    if (market.tab === "browse" && market.result === null) void market.loadBrowse(true);
    let unlisten: (() => void) | undefined;
    let stopped = false;
    void kernel
      .onEvent((event) => {
        if (event.topic.startsWith("plugins.")) plugins.scheduleRefresh();
      })
      .then((stop) => {
        if (stopped) stop();
        else unlisten = stop;
      });
    return () => {
      stopped = true;
      unlisten?.();
      plugins.stop();
    };
  });
</script>

<div class="mng-panel-head">
  <h2>Plugins</h2>
  <button
    type="button"
    class="mng-icon-btn"
    title="Install from a folder"
    aria-label="Install from a folder"
    disabled={plugins.installing}
    onclick={() => void plugins.install("folder")}
  >
    <Icon name="folder-open" size={13} />
  </button>
  <button
    type="button"
    class="mng-icon-btn"
    title="Refresh"
    aria-label="Refresh the plugin list"
    onclick={refresh}
  >
    <Icon name="refresh" size={13} />
  </button>
  <button
    type="button"
    class="inset-focus ml-1 inline-flex h-6 items-center bg-accent px-2.5 text-xs text-accent-ink disabled:opacity-50"
    disabled={plugins.installing}
    data-install
    onclick={() => void plugins.install("archive")}
  >
    Install
  </button>
</div>

<div
  class="flex gap-4 border-b border-line px-3"
  role="tablist"
  aria-label="Plugins"
  tabindex="-1"
  onkeydown={onTabKeys}
>
  {#each TABS as t (t.id)}
    <button
      type="button"
      role="tab"
      id="plugins-tab-{t.id}"
      aria-selected={market.tab === t.id}
      aria-controls="plugins-list"
      tabindex={market.tab === t.id ? 0 : -1}
      data-tab={t.id}
      class="inset-focus -mb-px h-7 border-b text-xs {market.tab === t.id
        ? 'border-text text-text'
        : 'border-transparent text-text-60 hover:text-text'}"
      onclick={() => pick(t.id)}
    >
      {t.label}
      {#if t.id === "installed"}
        <span class="font-mono text-[10px] text-text-muted">{plugins.list.length}</span>
      {:else if market.result !== null}
        <span class="font-mono text-[10px] text-text-muted">{market.result.total}</span>
      {/if}
    </button>
  {/each}
</div>

<div class="px-3 pt-2 pb-1">
  <input
    type="search"
    class="mng-field"
    placeholder={market.tab === "browse"
      ? "Search the registry by name, author or tag"
      : "Filter by name, command or publisher"}
    aria-label={market.tab === "browse" ? "Search the registry" : "Filter plugins"}
    bind:value={plugins.query}
    spellcheck="false"
  />
</div>

<div
  id="plugins-list"
  role="tabpanel"
  aria-labelledby="plugins-tab-{market.tab}"
  class="flex min-h-0 flex-1 flex-col"
>
  {#if market.tab === "installed"}
    <div
      class="flex flex-wrap gap-1 px-3 pt-1 pb-2"
      role="group"
      aria-label="Show plugins"
    >
      {#each PLUGIN_FILTERS as chip (chip.id)}
        <button
          type="button"
          class="inset-focus inline-flex h-6 items-center gap-1 border px-2 text-[11px] {plugins.filter ===
          chip.id
            ? 'border-text-60 bg-surface-hi text-text'
            : 'border-line text-text-60 hover:text-text'}"
          aria-pressed={plugins.filter === chip.id}
          data-filter={chip.id}
          onclick={() => (plugins.filter = chip.id)}
        >
          {chip.label}
          <span class="font-mono text-[10px] text-text-muted">{counts[chip.id]}</span>
        </button>
      {/each}
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto pb-4">
      {#each plugins.warnings as warning (warning)}
        <p
          role="alert"
          class="selectable m-3 border-l border-text-60 pl-3 text-[11px] leading-relaxed text-text"
        >
          {warning}
        </p>
      {/each}

      {#if market.updateCount > 0}
        <div
          class="mx-3 mb-2 flex items-center gap-2 border border-line px-2 py-1.5 text-[11px]"
          data-updates-row
        >
          <span class="min-w-0 flex-1 text-text" role="status"
            >{market.updateCount}
            {market.updateCount === 1 ? "update" : "updates"} available</span
          >
          <button
            type="button"
            class="mng-btn text-[11px]"
            data-update-all
            onclick={() => void market.updateAll()}>Update all</button
          >
        </div>
      {/if}

      {#if plugins.error !== ""}
        <div
          class="m-3 border border-line bg-surface p-3 text-xs text-text"
          role="alert"
        >
          <div class="flex items-center gap-1.5">
            <Icon name="error" size={13} />
            <span>Could not list plugins</span>
          </div>
          <p class="selectable m-0 mt-1 leading-relaxed text-text-60">
            {plugins.error}
          </p>
          <button
            type="button"
            class="mng-btn mt-2 text-xs"
            onclick={() => void plugins.refresh()}>Retry</button
          >
        </div>
      {:else if !plugins.loaded}
        <p class="m-0 px-4 pt-2 text-xs text-text-muted" role="status">
          Reading plugins…
        </p>
      {:else}
        <p
          class="m-0 px-4 py-1 text-[11px] text-text-muted"
          role="status"
          aria-live="polite"
          data-plugin-count
        >
          {#if plugins.query.trim() === "" && plugins.filter === "all"}
            {plugins.list.length} {plugins.list.length === 1 ? "plugin" : "plugins"}
          {:else}
            {shown.length} of {plugins.list.length} shown
          {/if}
        </p>
        {#if shown.length === 0}
          <div class="px-4 pt-1 text-xs leading-relaxed text-text-60">
            {#if plugins.filter === "attention" && plugins.query.trim() === ""}
              <p class="m-0">Every plugin is running or switched off on purpose.</p>
            {:else}
              <p class="m-0">No plugin matches. Clear the filter to see them all.</p>
              <button
                type="button"
                class="mng-btn mt-2 text-xs"
                onclick={() => {
                  plugins.filter = "all";
                  plugins.query = "";
                }}
              >
                Clear filter
              </button>
            {/if}
          </div>
        {/if}
        <ul class="m-0 list-none p-0" aria-label="Plugins">
          {#each shown as plugin (plugin.name)}
            <PluginRow {plugin} />
          {/each}
        </ul>
      {/if}
    </div>
  {:else}
    <div class="min-h-0 flex-1 overflow-y-auto pb-4" data-browse>
      {#if market.failure !== null && wording !== null}
        <div
          class="m-3 border border-line bg-surface p-3 text-xs text-text"
          role="alert"
          data-browse-problem={market.failure.problem}
        >
          <div class="flex items-center gap-1.5">
            <Icon name="error" size={13} />
            <span>{wording.title}</span>
          </div>
          <p class="selectable m-0 mt-1 leading-relaxed text-text-60">
            {wording.detail}
          </p>
          {#if market.failure.problem === "unreachable" || market.failure.problem === "other"}
            <button
              type="button"
              class="mng-btn mt-2 text-xs"
              data-retry
              onclick={() => void market.loadBrowse(true)}>Retry</button
            >
          {/if}
        </div>
      {:else if market.result === null}
        <p class="m-0 px-4 pt-2 text-xs text-text-muted" role="status" data-loading>
          Loading the plugin list…
        </p>
      {:else}
        {#if notice !== null}
          <p
            class="m-3 mb-1 border-l border-text-60 pl-3 text-[11px] leading-relaxed text-text"
            role="status"
            data-offline-notice
          >
            {notice}
            <button
              type="button"
              class="mng-btn ml-1 text-[11px]"
              data-retry
              onclick={() => void market.loadBrowse(true)}>Retry</button
            >
          </p>
        {:else if market.result.freshness.state === "cached"}
          <p class="m-0 px-4 pt-1 text-[11px] text-text-muted" data-cached-note>
            List from {isoDate(market.result.freshness.fetched_at)}
          </p>
        {/if}

        <div class="flex flex-wrap gap-1 px-3 pt-2" role="group" aria-label="Sort by">
          {#each BROWSE_SORTS as s (s.id)}
            <button
              type="button"
              class="inset-focus inline-flex h-6 items-center border px-2 text-[11px] {market.sort ===
              s.id
                ? 'border-text-60 bg-surface-hi text-text'
                : 'border-line text-text-60 hover:text-text'}"
              aria-pressed={market.sort === s.id}
              data-sort={s.id}
              onclick={() => (market.sort = s.id)}>{s.label}</button
            >
          {/each}
        </div>
        {#if market.result.tags.length > 0}
          <div
            class="flex flex-wrap gap-1 px-3 pt-1.5 pb-2"
            role="group"
            aria-label="Tags"
          >
            {#each market.result.tags.slice(0, 8) as t (t.tag)}
              <button
                type="button"
                class="inset-focus inline-flex h-5 items-center border px-1.5 text-[10px] {market.tag ===
                t.tag
                  ? 'border-text-60 bg-surface-hi text-text'
                  : 'border-line text-text-60 hover:text-text'}"
                aria-pressed={market.tag === t.tag}
                data-tag={t.tag}
                onclick={() => (market.tag = market.tag === t.tag ? null : t.tag)}
                >{t.tag}</button
              >
            {/each}
          </div>
        {/if}

        <p
          class="m-0 px-4 py-1 text-[11px] text-text-muted"
          role="status"
          aria-live="polite"
          data-browse-count
        >
          {#if plugins.query.trim() === "" && market.tag === null}
            {market.result.total} {market.result.total === 1 ? "plugin" : "plugins"}
          {:else}
            {browseShown.length} of {market.result.total} shown
          {/if}
        </p>
        {#if browseShown.length === 0}
          <div class="px-4 pt-1 text-xs leading-relaxed text-text-60" data-browse-empty>
            <p class="m-0">
              {market.result.total === 0
                ? "The registry lists no plugins yet."
                : "No plugin matches your search."}
            </p>
            {#if market.result.total > 0}
              <button
                type="button"
                class="mng-btn mt-2 text-xs"
                onclick={() => {
                  plugins.query = "";
                  market.tag = null;
                }}>Clear search</button
              >
            {/if}
          </div>
        {/if}
        <ul class="m-0 list-none p-0" aria-label="Registry plugins">
          {#each browseShown as entry (entry.id)}
            <BrowseRow {entry} />
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</div>
