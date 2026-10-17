<script lang="ts">
  import { tick } from "svelte";

  import SlideFrame from "./SlideFrame.svelte";
  import { slides } from "$lib/state/slides.svelte";
  import type { Slide } from "$lib/workbench/slides";

  /**
   * Thumbnail strip: a listbox of slides. Up and Down move and select; the
   * selected thumbnail carries the lime edge. Thumbnails reuse the real slide
   * frame, so they can never drift from what is presented.
   */
  interface Props {
    deck: readonly Slide[];
  }

  const { deck }: Props = $props();

  let list: HTMLElement | undefined = $state();

  async function select(index: number): Promise<void> {
    slides.goTo(index, deck.length);
    await tick();
    list?.querySelector<HTMLElement>(`[data-slide="${slides.index}"]`)?.focus();
  }

  function onkeydown(event: KeyboardEvent): void {
    const keys: Record<string, number> = {
      ArrowDown: slides.index + 1,
      ArrowUp: slides.index - 1,
      Home: 0,
      End: deck.length - 1,
    };
    const next = keys[event.key];
    if (next === undefined) return;
    event.preventDefault();
    void select(next);
  }
</script>

<div
  bind:this={list}
  role="listbox"
  tabindex="-1"
  aria-label="Slides"
  aria-orientation="vertical"
  class="flex w-48 shrink-0 flex-col gap-3 overflow-y-auto border-r border-line bg-surface p-3"
  {onkeydown}
>
  {#each deck as slide (slide.id)}
    {@const current = slide.index === slides.index}
    <button
      type="button"
      role="option"
      data-slide={slide.index}
      aria-selected={current}
      aria-label="Slide {slide.index + 1}: {slide.title}"
      tabindex={current ? 0 : -1}
      class="inset-focus relative block w-full text-left"
      onclick={() => slides.goTo(slide.index, deck.length)}
    >
      {#if current}
        <span class="absolute inset-y-0 -left-3 w-0.5 bg-accent" aria-hidden="true"
        ></span>
      {/if}
      <span class="pointer-events-none block" aria-hidden="true">
        <SlideFrame {slide} total={deck.length} />
      </span>
    </button>
  {/each}
</div>
