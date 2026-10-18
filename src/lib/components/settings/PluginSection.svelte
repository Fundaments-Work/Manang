<script lang="ts">
  import type { Component, Snippet } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";
  import type { PluginEntry } from "$lib/workbench/plugins";
  import { formatCategoryLabel } from "$lib/workbench/settings";

  import LatexEngine from "./LatexEngine.svelte";
  import PersonalDictionary from "./PersonalDictionary.svelte";

  /**
   * One plugin on the Settings page: what it is, what it may touch, its
   * commands on request, then its own settings (passed in as `children`, the
   * same rows the app's settings use).
   */

  interface Props {
    entry: PluginEntry;
    children?: Snippet;
  }

  const { entry, children }: Props = $props();

  /** Settings a plugin keeps outside its manifest, shown after its rows. */
  const EXTRAS: Readonly<Record<string, Component>> = {
    writing: PersonalDictionary,
    latex: LatexEngine,
  };
  const Extra = $derived(EXTRAS[entry.name]);

  let showCommands = $state(false);

  const tier = $derived(
    entry.tier === "wasm"
      ? "Sandboxed (WebAssembly)"
      : entry.tier === "native"
        ? "Built in"
        : "",
  );
</script>

<section
  data-group="plugin:{entry.name}"
  data-plugin={entry.name}
  aria-labelledby="plugin-title-{entry.name}"
  class="mt-6 border-t border-line pt-6 pb-2"
>
  <div class="flex flex-wrap items-baseline gap-x-3 gap-y-1">
    <h3 id="plugin-title-{entry.name}" class="m-0 text-[14px] font-normal text-text">
      {formatCategoryLabel(entry.name)}
    </h3>
    {#if entry.version !== ""}
      <span class="font-mono text-[11px] text-text-muted">{entry.version}</span>
    {/if}
    {#if tier !== ""}
      <span class="text-[11px] text-text-60">{tier}</span>
    {/if}
  </div>
  {#if entry.description !== ""}
    <p class="m-0 mt-1 max-w-[60ch] text-xs leading-relaxed text-text-60">
      {entry.description}
    </p>
  {/if}
  {#if entry.access.length > 0}
    <ul class="m-0 mt-2 list-none p-0 text-[11px] leading-relaxed text-text-60">
      {#each entry.access as line (line)}
        <li>{line}</li>
      {/each}
    </ul>
  {/if}

  {#if entry.commands.length > 0}
    <button
      type="button"
      class="inset-focus mt-2 -ml-1 flex h-7 items-center gap-1 px-1 text-xs text-text-60 hover:text-text"
      aria-expanded={showCommands}
      aria-controls="plugin-commands-{entry.name}"
      onclick={() => (showCommands = !showCommands)}
    >
      <Icon name={showCommands ? "chevron-down" : "chevron-right"} size={12} />
      {entry.commands.length}
      {entry.commands.length === 1 ? "command" : "commands"}
    </button>
    {#if showCommands}
      <ul
        id="plugin-commands-{entry.name}"
        class="m-0 mb-2 grid list-none gap-x-6 p-0 pl-4 @xl:grid-cols-2"
        aria-label="{entry.name} commands"
      >
        {#each entry.commands as command (command.id)}
          <li class="min-w-0 py-1">
            <span class="block truncate text-xs text-text">{command.title}</span>
            <span
              class="selectable block truncate font-mono text-[11px] text-text-muted"
              >{command.id}</span
            >
          </li>
        {/each}
      </ul>
    {/if}
  {/if}

  {#if children !== undefined || Extra !== undefined}
    <div class="mt-2">
      {@render children?.()}
      {#if Extra !== undefined}
        <Extra />
      {/if}
    </div>
  {/if}
</section>
