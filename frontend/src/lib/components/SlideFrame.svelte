<script lang="ts">
  import { deck } from "$lib/state/slides.svelte";

  /**
   * The slide itself — DESIGN.md §8.1.
   *
   * A true 16:9 frame, so the studio shows the same aspect ratio the audience
   * will see. Content is laid out by the engine eventually; the shell provides
   * the frame, the type ramp and the citation rail.
   */
</script>

<div class="flex h-full w-full items-center justify-center p-8">
  <div
    class="aspect-video w-full max-w-5xl border border-line bg-paper px-16 py-14 text-paper-ink"
  >
    {#if deck.current !== undefined}
      {@const slide = deck.current}

      <div
        class="flex items-baseline justify-between font-mono text-[10px] tracking-[0.08em] text-paper-ink-60 uppercase"
      >
        <span>{slide.kicker}</span>
        <span>Slide {deck.displayNumber} / {deck.count}</span>
      </div>

      <h2
        class="mt-10 max-w-3xl font-sans text-3xl font-light"
        style="letter-spacing: var(--tracking-display)"
      >
        {slide.title}
      </h2>

      <p
        class="mt-5 max-w-2xl font-sans text-sm"
        style="letter-spacing: var(--tracking-body)"
      >
        {slide.body}
      </p>

      {#if slide.math !== undefined}
        <p class="mt-10 text-center font-mono text-xl">{slide.math}</p>
      {/if}

      {#if slide.citation !== undefined}
        <p class="mt-auto pt-10 font-mono text-[11px] text-paper-ink-60">
          {slide.citation}
        </p>
      {/if}
    {/if}
  </div>
</div>
