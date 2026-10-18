<script lang="ts">
  import { onMount } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";
  import { isTabDirty, newFigure, openFile } from "$lib/state/actions";
  import { tabs } from "$lib/state/tabs.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { basename, dirname } from "$lib/workbench/paths";

  /** Figures: every `.figure` (and legacy `.figure.json`) in the project. Each opens in the figure editor, in a tab. */

  onMount(() => void workspace.ensureAll());
</script>

<div class="mng-panel-head">
  <h2>Figures</h2>
  <button
    type="button"
    class="mng-icon-btn"
    aria-label="New figure"
    title="New figure"
    onclick={() => void newFigure()}
  >
    <Icon name="plus" />
  </button>
</div>

<div class="min-h-0 flex-1 overflow-y-auto pb-4">
  {#if workspace.all === null}
    <p class="m-0 px-4 text-xs text-text-muted" role="status">Loading figures</p>
  {:else if workspace.figures.length === 0}
    <div class="px-4 text-xs leading-relaxed text-text-60">
      <p class="m-0">
        No figures yet. A figure is drawn in Manang and saved as a <span
          class="font-mono text-[11px]">.figure</span
        > file you can embed in any document.
      </p>
      <button type="button" class="mng-btn mt-3" onclick={() => void newFigure()}
        >New figure</button
      >
    </div>
  {:else}
    <ul class="m-0 list-none p-0" aria-label="Figure files">
      {#each workspace.figures as figure (figure.path)}
        {@const active = tabs.active?.path === figure.path}
        <li>
          <button
            type="button"
            class="inset-focus flex h-8 w-full items-center gap-2 px-4 text-left text-xs hover:bg-surface-hi {active
              ? 'bg-surface-hi text-text'
              : 'text-text-60 hover:text-text'}"
            aria-current={active ? "true" : undefined}
            onclick={() => void openFile(figure.path)}
          >
            <Icon name="figures" size={14} class="shrink-0 opacity-70" />
            <span class="truncate"
              >{basename(figure.path).replace(/\.figure\.json$/, "")}</span
            >
            {#if dirname(figure.path) !== ""}<span class="truncate text-text-muted"
                >{dirname(figure.path)}</span
              >{/if}
            {#if isTabDirty(figure.path)}<span
                class="ml-auto h-1.5 w-1.5 shrink-0 bg-text"
                aria-label="Unsaved changes"
              ></span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
