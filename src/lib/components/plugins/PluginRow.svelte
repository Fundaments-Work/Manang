<script lang="ts">
  import PluginIcon from "$lib/components/plugins/PluginIcon.svelte";
  import type { PluginState } from "$lib/kernel/pluginsApi";
  import { market } from "$lib/state/market.svelte";
  import { plugins } from "$lib/state/plugins.svelte";
  import { canToggle, stateLabel, tierLabel } from "$lib/workbench/pluginManager";

  /**
   * One plugin in the Installed list: icon, name, version, who made it, its
   * state and a one-line description. Clicking opens the plugin's page in the
   * editor area (the details, permissions and actions live there). The enable
   * switch stays here for the quick case, and a plugin that waits for consent
   * says so with a Review button.
   */

  interface Props {
    plugin: PluginState;
  }

  const { plugin }: Props = $props();

  const busy = $derived(plugins.busy.includes(plugin.name));
  const update = $derived(market.updateFor(plugin.name));
  const attention = $derived(
    plugin.status === "failed" || plugin.status === "needs_approval",
  );
  const maker = $derived(
    plugin.tier === "native" ? "Manang" : (plugin.publisher ?? "Unknown publisher"),
  );
</script>

<li
  class="border-b border-line-subtle last:border-b-0"
  data-plugin={plugin.name}
  data-status={plugin.status}
  data-tier={plugin.tier}
>
  <div class="flex items-start gap-2 px-3 py-2">
    <button
      type="button"
      class="inset-focus flex min-w-0 flex-1 items-start gap-2 text-left"
      data-open-page
      aria-label="Open {plugin.name}"
      onclick={() => market.openPage(plugin.name)}
    >
      <PluginIcon name={plugin.name} />
      <span class="min-w-0 flex-1">
        <span class="flex items-baseline gap-2">
          <span class="truncate text-xs text-text">{plugin.name}</span>
          <span class="shrink-0 font-mono text-[10px] text-text-muted">
            {plugin.running_version ?? plugin.version}
          </span>
        </span>
        <span class="mt-0.5 flex flex-wrap items-center gap-x-1.5 text-[11px]">
          <span class="truncate text-text-60" data-maker>{maker}</span>
          <span class="text-text-muted" aria-hidden="true">·</span>
          <span class="text-text-60" data-tier-label>{tierLabel(plugin)}</span>
          <span class="text-text-muted" aria-hidden="true">·</span>
          <span class={attention ? "text-text" : "text-text-60"} data-state-label
            >{stateLabel(plugin)}</span
          >
        </span>
        {#if plugin.description !== ""}
          <span class="mt-0.5 line-clamp-2 block text-[11px] leading-snug text-text-60">
            {plugin.description}
          </span>
        {/if}
        {#if update !== undefined}
          <span
            class="mt-1 inline-block border border-text-60 px-1 font-mono text-[10px] text-text"
            data-update-badge>Update to {update.latest}</span
          >
        {/if}
      </span>
    </button>
    {#if canToggle(plugin)}
      <button
        type="button"
        role="switch"
        aria-checked={plugin.enabled}
        aria-label="{plugin.name} enabled"
        title={plugin.enabled ? "Disable" : "Enable"}
        disabled={busy || plugin.disabled_by === "project"}
        class="inset-focus mt-0.5 flex h-4 w-8 shrink-0 items-center border border-line px-px disabled:opacity-40 {plugin.enabled
          ? 'justify-end'
          : 'justify-start'}"
        onclick={() => void plugins.toggle(plugin)}
      >
        <span
          class="h-2.5 w-3 {plugin.enabled ? 'bg-text' : 'bg-text-muted'}"
          aria-hidden="true"
        ></span>
      </button>
    {:else}
      <span
        class="mt-0.5 shrink-0 text-[10px] text-text-muted"
        title="Required by Manang: it cannot be disabled or removed"
        data-core>Core</span
      >
    {/if}
  </div>

  {#if plugin.status === "failed" && plugin.error !== undefined}
    <p
      class="selectable m-0 mr-3 mb-2 ml-3 border-l border-text-60 pl-3 text-[11px] leading-relaxed text-text"
      data-plugin-error
    >
      {plugin.error}
    </p>
  {/if}

  {#if plugin.status === "needs_approval"}
    <div class="flex flex-wrap items-center gap-2 px-3 pb-2">
      <button
        type="button"
        class="mng-btn text-[11px]"
        disabled={busy}
        data-review
        onclick={() => void plugins.review(plugin)}
      >
        Review access
      </button>
      <span class="text-[11px] text-text-60">
        {#if plugin.update_pending}
          Version {plugin.version} asks for more{plugin.running_version
            ? `; ${plugin.running_version} keeps running`
            : ""}
        {:else}
          Not running until you approve it
        {/if}
      </span>
    </div>
  {/if}
</li>
