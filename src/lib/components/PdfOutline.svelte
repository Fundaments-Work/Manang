<script lang="ts">
  import type { OutlineEntry } from "$lib/workbench/pdf/links";

  /**
   * The PDF's outline (LaTeX hyperref bookmarks, Typst headings) as a narrow
   * list beside the pages. Each entry is a button; Up/Down move between them.
   */
  interface Props {
    entries: readonly OutlineEntry[];
    onselect: (entry: OutlineEntry) => void;
  }

  const { entries, onselect }: Props = $props();

  function onkeydown(event: KeyboardEvent): void {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const buttons = [
      ...(event.currentTarget as HTMLElement).querySelectorAll<HTMLButtonElement>(
        "button[data-outline-entry]",
      ),
    ];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const next = buttons[at + (event.key === "ArrowDown" ? 1 : -1)];
    if (next === undefined) return;
    event.preventDefault();
    next.focus();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<nav
  class="h-full w-52 shrink-0 overflow-y-auto border-r border-line bg-bg py-1"
  aria-label="PDF outline"
  data-pdf-outline
  {onkeydown}
>
  <ul class="m-0 list-none p-0">
    {#each entries as entry (entry.id)}
      <li>
        <button
          type="button"
          class="inset-focus flex h-7 w-full items-center truncate pr-2 text-left text-xs text-text-60 hover:bg-surface-hi hover:text-text"
          style="padding-left: {8 + entry.depth * 12}px"
          title={entry.title}
          data-outline-entry
          onclick={() => onselect(entry)}
        >
          <span class="truncate">{entry.title}</span>
        </button>
      </li>
    {/each}
  </ul>
</nav>
