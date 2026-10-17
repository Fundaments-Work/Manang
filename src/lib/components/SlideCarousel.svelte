<script lang="ts">
  import { deck } from "$lib/state/slides.svelte";

  /**
   * Slide carousel — DESIGN.md §8.1.
   *
   * Miniature cards for rapid jumping. The active slide is marked by a
   * lime hairline along its top edge: a 1px line, because depth is tonal
   * and the system has no shadows to lean on.
   */
</script>

<nav class="flex shrink-0 items-end justify-center gap-2 px-6 pb-5" aria-label="Slides">
  {#each deck.slides as slide, index (slide.id)}
    <button
      type="button"
      class="group w-40 border border-line bg-surface px-3 py-2.5 text-left transition-colors duration-fast ease-manang hover:bg-surface-hi"
      class:border-t-2={index === deck.index}
      class:border-t-accent={index === deck.index}
      aria-current={index === deck.index ? "true" : undefined}
      onclick={() => deck.goTo(index)}
    >
      <span class="block font-mono text-[10px] text-text-muted">{slide.ordinal}</span>
      <span
        class="mt-1 block truncate font-sans text-[11px] text-text-60"
        class:text-text={index === deck.index}
      >
        {slide.title}
      </span>
    </button>
  {/each}
</nav>
