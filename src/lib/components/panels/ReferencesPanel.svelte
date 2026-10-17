<script lang="ts">
  import { onMount } from "svelte";

  import { insertCitation } from "$lib/state/actions";
  import { workspace } from "$lib/state/workspace.svelte";
  import { authorSummary, filterReferences } from "$lib/workbench/references";

  /**
   * References: entries from the project's `.bib` files. Choosing one inserts a
   * citation in the syntax of the open document's engine. Parsing is a small
   * client-side reader (`workbench/references.ts`) until the citations plugin
   * takes over.
   */

  let query = $state("");

  onMount(() => void workspace.loadReferences());

  const shown = $derived(filterReferences(workspace.references, query));
  const bibFiles = $derived(new Set(workspace.references.map((r) => r.file)).size);
</script>

<div class="mng-panel-head"><h2>References</h2></div>

<div class="px-3 pb-1">
  <input
    type="search"
    class="mng-field"
    placeholder="Filter by author, title or key"
    aria-label="Filter references"
    bind:value={query}
    spellcheck="false"
  />
</div>

<div class="min-h-0 flex-1 overflow-y-auto pb-4">
  {#if !workspace.referencesLoaded}
    <p class="m-0 px-4 pt-2 text-xs text-text-muted" role="status">
      Reading bibliography files
    </p>
  {:else if workspace.references.length === 0}
    <div class="px-4 pt-3 text-xs leading-relaxed text-text-60">
      <p class="m-0">
        No references found. Add a <span class="font-mono text-[11px]">.bib</span> file to
        the project and its entries appear here, ready to cite.
      </p>
    </div>
  {:else}
    <p
      class="m-0 px-4 py-2 text-[11px] text-text-muted"
      role="status"
      aria-live="polite"
    >
      {#if query.trim() === ""}
        {workspace.references.length}
        {workspace.references.length === 1 ? "reference" : "references"} in {bibFiles}
        {bibFiles === 1 ? "file" : "files"}
      {:else}
        {shown.length} of {workspace.references.length} match
      {/if}
    </p>
    {#if shown.length === 0}
      <p class="m-0 px-4 text-xs text-text-60">
        Nothing matches "{query.trim()}". Try an author's surname or part of the title.
      </p>
    {/if}
    <ul class="m-0 list-none p-0" aria-label="References">
      {#each shown as entry (`${entry.file}:${entry.key}`)}
        <li>
          <button
            type="button"
            class="inset-focus block w-full px-4 py-2 text-left hover:bg-surface-hi"
            title="Insert citation for {entry.key}"
            onclick={() => insertCitation(entry)}
          >
            <span class="flex items-baseline gap-2 text-xs text-text">
              <span class="truncate">{authorSummary(entry.authors)}</span>
              {#if entry.year !== ""}<span class="shrink-0 text-text-60"
                  >{entry.year}</span
                >{/if}
            </span>
            <span class="mt-0.5 line-clamp-2 text-[11px] leading-snug text-text-60"
              >{entry.title}</span
            >
            <span class="mt-1 block truncate font-mono text-[10px] text-text-muted"
              >{entry.key}</span
            >
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
