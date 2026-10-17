<script lang="ts">
  import EntryGlyph from "./EntryGlyph.svelte";
  import Overlay from "./ui/Overlay.svelte";
  import { library } from "$lib/state/library.svelte";
  import { desk } from "$lib/state/desk.svelte";

  /**
   * The Library Shelf — DESIGN.md §5.
   *
   * A 260px rail sliding out from the left. A local-first app is a folder, so
   * the rail is a file browser with the file manager's anxieties removed: no
   * columns, no sorting menus, no confirmation dialogs. Type to filter.
   */

  const WIDTH = 260;

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      desk.libraryOpen = false;
    }
  }
</script>

<svelte:window {onkeydown} />

{#if desk.libraryOpen}
  <Overlay side="left">
    <aside
      class="flex h-full flex-col border-r border-line bg-bg"
      style="width: {WIDTH}px"
      aria-label="Library shelf"
    >
      <div
        class="flex h-10 shrink-0 items-center justify-between border-b border-line px-3"
      >
        <h2
          id="library-title"
          class="font-mono text-xs text-text-60"
          style="letter-spacing: 0.04em"
        >
          LIBRARY SHELF
        </h2>
        <button
          type="button"
          class="flex h-6 w-6 items-center justify-center border border-transparent text-text-60 transition-colors duration-fast ease-manang hover:border-line hover:text-text"
          aria-label="Close library shelf"
          onclick={() => (desk.libraryOpen = false)}
        >
          <svg
            viewBox="0 0 12 12"
            width="10"
            height="10"
            fill="none"
            stroke="currentColor"
            stroke-width="1.25"
            aria-hidden="true"
          >
            <path d="M2.5 2.5 L9.5 9.5" />
            <path d="M9.5 2.5 L2.5 9.5" />
          </svg>
        </button>
      </div>

      <div class="border-b border-line p-2">
        <label class="sr-only" for="library-filter">Filter files</label>
        <input
          id="library-filter"
          type="text"
          placeholder="Filter…"
          value={library.query}
          oninput={(event) => library.setQuery(event.currentTarget.value)}
          class="h-7 w-full border border-line bg-surface px-2 font-mono text-xs text-text placeholder:text-text-muted focus:border-accent focus:outline-none"
        />
      </div>

      <nav class="min-h-0 flex-1 overflow-y-auto py-1" aria-label="Project files">
        {#each library.visibleFolders as { folder, entries } (folder.id)}
          <section class="mb-1">
            <h3
              class="flex items-center gap-1 px-3 py-1 font-mono text-[11px] text-text-60"
            >
              <svg
                viewBox="0 0 12 12"
                width="9"
                height="9"
                fill="none"
                stroke="currentColor"
                stroke-width="1.25"
                aria-hidden="true"
              >
                <path d="M2.5 4.5 L6 8 L9.5 4.5" />
              </svg>
              {folder.name}
            </h3>

            <ul>
              {#each entries as entry (entry.id)}
                <li>
                  <button
                    type="button"
                    class="flex w-full items-center gap-2 py-1.5 pr-3 pl-3 text-left font-mono text-xs transition-colors duration-fast ease-manang hover:bg-surface-hi"
                    class:bg-surface={library.isActive(entry.id)}
                    class:border-l-2={library.isActive(entry.id)}
                    class:border-accent={library.isActive(entry.id)}
                    class:border-l-transparent={!library.isActive(entry.id)}
                    aria-current={library.isActive(entry.id) ? "true" : undefined}
                    onclick={() => library.activate(entry.id)}
                  >
                    <EntryGlyph kind={entry.kind} active={library.isActive(entry.id)} />
                    <span
                      class="min-w-0 flex-1 truncate"
                      class:text-accent={library.isActive(entry.id)}
                    >
                      {entry.name}
                    </span>
                    {#if entry.itemCount !== undefined}
                      <span class="text-text-muted">({entry.itemCount})</span>
                    {/if}
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {:else}
          <p class="px-3 py-4 font-mono text-xs text-text-muted">No matches</p>
        {/each}
      </nav>

      <div class="grid shrink-0 grid-cols-2 gap-px border-t border-line bg-line">
        <button
          type="button"
          class="bg-bg px-3 py-2.5 text-left font-sans text-xs text-text-60 transition-colors duration-fast ease-manang hover:bg-surface-hi hover:text-text"
        >
          + New Document
        </button>
        <button
          type="button"
          class="bg-bg px-3 py-2.5 text-left font-sans text-xs text-text-60 transition-colors duration-fast ease-manang hover:bg-surface-hi hover:text-text"
          onclick={() => {
            desk.libraryOpen = false;
            desk.canvasOpen = true;
          }}
        >
          + New Diagram
        </button>
      </div>
    </aside>

    <!-- Click-away region. Deliberately not a dark scrim: DESIGN.md §4.1 asks
         for overlays without backdrop darkness. -->
    <button
      type="button"
      class="flex-1 cursor-default"
      aria-label="Close library shelf"
      onclick={() => (desk.libraryOpen = false)}
    ></button>
  </Overlay>
{/if}
