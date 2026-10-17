<script lang="ts">
  import type { Slide } from "$lib/workbench/slides";

  /**
   * One slide, drawn at a true 16:9 on the paper tokens. Every size is in
   * container-query units, so the same markup renders the large slide, the
   * thumbnail and the full-screen presentation without a scale transform.
   * Layout is the shell's; the engine's own render replaces it later.
   */
  interface Props {
    slide: Slide;
    total: number;
  }

  const { slide, total }: Props = $props();
  const title = $derived(slide.level === 1);
</script>

<div
  class="relative aspect-video w-full overflow-hidden border border-line bg-paper text-paper-ink"
  style="container-type: inline-size"
>
  <div class="flex h-full flex-col" style="padding: 6cqw 7cqw">
    <h2
      class="m-0 font-sans font-light"
      style="font-size: {title
        ? '6cqw'
        : '4.4cqw'}; line-height: 1.12; letter-spacing: var(--tracking-display); margin-top: {title
        ? '14cqw'
        : '0'}"
    >
      {slide.title}
    </h2>
    {#each slide.paragraphs as paragraph (paragraph)}
      <p
        class="m-0 font-sans"
        style="font-size: 2.3cqw; line-height: 1.45; margin-top: 2.4cqw; max-width: 80%; letter-spacing: var(--tracking-body)"
      >
        {paragraph}
      </p>
    {/each}
    {#if slide.math !== null}
      <p class="m-0 text-center font-mono" style="font-size: 2.8cqw; margin-top: 4cqw">
        {slide.math}
      </p>
    {/if}
    <span class="mt-auto self-end font-mono text-paper-ink-60" style="font-size: 1.5cqw"
      >{slide.index + 1} / {total}</span
    >
  </div>
</div>
