<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { palette } from "$lib/state/palette.svelte";

  /**
   * Command & Co-Pilot Palette — DESIGN.md §7.
   *
   * 600px, centred, zero radius, one hairline. Keyboard-first: the input is
   * focused on open and the whole surface is operable without a pointer.
   */

  let input: HTMLInputElement | undefined = $state();

  // Focus whenever the palette opens, so typing goes straight to the query.
  $effect(() => {
    if (palette.open) input?.focus();
  });

  /** Absolute row index of a command within the flattened result list. */
  function indexOf(id: string): number {
    return palette.flatOrder.findIndex((command) => command.id === id);
  }
</script>

{#if palette.open}
  <div
    class="fixed inset-0 z-40 flex items-start justify-center pt-[18vh]"
    transition:fade={{ duration: 110 }}
  >
    <!-- Click-away: dismisses without a scrim, per DESIGN.md §4.1. -->
    <button
      type="button"
      class="absolute inset-0 cursor-default"
      aria-label="Dismiss palette"
      onclick={() => palette.hide()}
    ></button>

    <div
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
      class="relative w-[600px] max-w-[92vw] border border-line bg-surface"
      transition:scale={{ duration: 150, start: 0.98, opacity: 0.1 }}
    >
      <label class="sr-only" for="palette-query">Ask or type a command</label>
      <input
        id="palette-query"
        bind:this={input}
        type="text"
        placeholder="Ask or type a command…"
        value={palette.query}
        oninput={(event) => palette.setQuery(event.currentTarget.value)}
        class="h-11 w-full border-b border-line bg-surface px-4 font-mono text-sm text-text placeholder:text-text-muted focus:outline-none"
      />

      <div class="max-h-[52vh] overflow-y-auto py-1">
        {#each palette.groups as group (group.group)}
          <section>
            <h2
              class="px-4 pt-3 pb-1 font-mono text-[10px] tracking-[0.06em] text-text-muted uppercase"
            >
              {group.group}
            </h2>
            <ul role="listbox" aria-label={group.group}>
              {#each group.entries as entry (entry.item.id)}
                {@const row = indexOf(entry.item.id)}
                {@const active = row === palette.cursor}
                <li>
                  <button
                    type="button"
                    role="option"
                    class="flex w-full items-center gap-2 px-4 py-2 text-left font-sans text-xs transition-colors duration-fast ease-manang"
                    class:bg-surface-hi={active}
                    class:text-text={active}
                    class:text-text-60={!active}
                    aria-selected={active}
                    onmouseenter={() => (palette.cursor = row)}
                    onclick={() => palette.execute()}
                  >
                    {#if entry.item.group === "AI Actions"}
                      <span class="shrink-0 text-math" aria-hidden="true">✦</span>
                    {:else}
                      <span class="shrink-0" aria-hidden="true">·</span>
                    {/if}

                    <span class="min-w-0 flex-1 truncate">{entry.item.title}</span>

                    {#if entry.item.hint !== undefined}
                      <span class="shrink-0 font-mono text-[10px] text-text-muted">
                        {entry.item.hint}
                      </span>
                    {/if}
                  </button>
                </li>
              {/each}
            </ul>
          </section>
        {:else}
          <p class="px-4 py-6 text-center font-sans text-xs text-text-muted">
            Nothing matches that.
          </p>
        {/each}
      </div>

      <footer
        class="flex items-center gap-4 border-t border-line px-4 py-2 font-mono text-[10px] text-text-muted"
      >
        <span>↑ ↓ navigate</span>
        <span>↵ run</span>
        <span>esc dismiss</span>
      </footer>
    </div>
  </div>
{/if}
