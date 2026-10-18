<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import type { PluginState } from "$lib/kernel/pluginsApi";
  import { plugins } from "$lib/state/plugins.svelte";
  import { settingsPage } from "$lib/state/settingsPage.svelte";
  import {
    accessLines,
    canToggle,
    canUninstall,
    disabledReason,
    installedDate,
    stateLabel,
    tierLabel,
  } from "$lib/workbench/pluginManager";

  /**
   * One plugin in the Plugins view: name, version, tier and state on one
   * line, the enable switch for everything that is not core, the failure
   * text when it failed, and on request its details (what it can access, its
   * commands) and actions.
   */

  interface Props {
    plugin: PluginState;
  }

  const { plugin }: Props = $props();

  const open = $derived(plugins.selected === plugin.name);
  const busy = $derived(plugins.busy.includes(plugin.name));
  const lines = $derived(accessLines(plugin.permissions, plugin.new_permissions));
  const attention = $derived(
    plugin.status === "failed" || plugin.status === "needs_approval",
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
      class="inset-focus min-w-0 flex-1 text-left"
      aria-expanded={open}
      aria-controls="plugin-details-{plugin.name}"
      onclick={() => plugins.select(plugin.name)}
    >
      <span class="flex items-baseline gap-2">
        <span class="truncate text-xs text-text">{plugin.name}</span>
        <span class="shrink-0 font-mono text-[10px] text-text-muted">
          {plugin.version}
        </span>
      </span>
      <span class="mt-0.5 flex flex-wrap items-center gap-x-1.5 text-[11px]">
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

  {#if plugin.status === "disabled"}
    <p class="m-0 px-3 pb-2 text-[11px] text-text-muted">{disabledReason(plugin)}</p>
  {/if}

  {#if open}
    <div
      id="plugin-details-{plugin.name}"
      class="border-t border-line-subtle bg-bg px-3 pt-2 pb-3 text-[11px] leading-relaxed text-text-60"
      data-details={plugin.name}
    >
      <dl class="m-0 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5">
        <dt class="text-text-muted">Version</dt>
        <dd class="m-0 font-mono text-[11px]">
          {plugin.running_version ?? plugin.version}
          {#if plugin.running_version !== undefined && plugin.running_version !== plugin.version}
            <span class="text-text-muted">({plugin.version} waiting)</span>
          {/if}
        </dd>
        <dt class="text-text-muted">Publisher</dt>
        <dd class="m-0">
          {#if plugin.tier === "native"}
            Manang
          {:else if plugin.publisher !== undefined}
            {plugin.publisher} <span class="text-text-muted">(not verified)</span>
          {:else}
            Not stated <span class="text-text-muted">(unsigned)</span>
          {/if}
        </dd>
        {#if plugin.source !== undefined}
          <dt class="text-text-muted">Source</dt>
          <dd
            class="selectable m-0 truncate font-mono text-[11px]"
            title={plugin.source}
          >
            {plugin.source}
          </dd>
        {/if}
        {#if plugin.installed_at !== undefined}
          <dt class="text-text-muted">Installed</dt>
          <dd class="m-0 font-mono text-[11px]">
            {installedDate(plugin.installed_at)}
          </dd>
        {/if}
      </dl>

      <h3 class="mt-3 mb-1 text-[11px] font-normal text-text">What it can access</h3>
      {#if lines.length === 0}
        <p class="m-0" data-access-none>Nothing beyond its own commands.</p>
      {:else}
        <ul class="m-0 list-none p-0" data-access>
          {#each lines as line (line.text)}
            <li class="flex items-baseline gap-2">
              <span class={line.isNew ? "text-text" : ""}>{line.text}</span>
              {#if line.isNew}
                <span class="border border-text-60 px-1 font-mono text-[10px] text-text"
                  >New</span
                >
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      {#if plugin.commands.length > 0}
        <h3 class="mt-3 mb-1 text-[11px] font-normal text-text">
          {plugin.commands.length}
          {plugin.commands.length === 1 ? "command" : "commands"}
        </h3>
        <ul class="m-0 list-none p-0" data-commands>
          {#each plugin.commands as command (command.id)}
            <li class="min-w-0 py-0.5">
              <span class="block truncate text-text">{command.title}</span>
              <span
                class="selectable block truncate font-mono text-[10px] text-text-muted"
                >{command.id}</span
              >
            </li>
          {/each}
        </ul>
      {/if}

      <p class="m-0 mt-3 text-[11px] text-text-muted">
        {#if plugin.core}
          Required by Manang: it cannot be disabled or removed.
        {:else if plugin.tier === "native"}
          Built in: it can be switched off but not removed.
        {/if}
      </p>

      <div class="mt-2 flex flex-wrap gap-1.5">
        {#if plugin.settings.length > 0}
          <button
            type="button"
            class="mng-btn text-[11px]"
            data-action="settings"
            onclick={() => settingsPage.show(plugin.name)}
          >
            Open settings
          </button>
        {/if}
        {#if plugin.tier === "wasm"}
          <button
            type="button"
            class="mng-btn text-[11px]"
            disabled={busy}
            data-action="reload"
            onclick={() => void plugins.reload(plugin)}
          >
            <Icon name="refresh" size={12} />
            Reload
          </button>
        {/if}
        {#if canToggle(plugin)}
          <button
            type="button"
            class="mng-btn text-[11px]"
            disabled={busy || plugin.disabled_by === "project"}
            data-action="toggle"
            onclick={() => void plugins.toggle(plugin)}
          >
            {plugin.enabled ? "Disable" : "Enable"}
          </button>
        {/if}
        {#if canUninstall(plugin)}
          <button
            type="button"
            class="mng-btn text-[11px]"
            disabled={busy}
            data-action="uninstall"
            onclick={() => void plugins.uninstall(plugin)}
          >
            Uninstall
          </button>
        {/if}
      </div>
    </div>
  {/if}
</li>
