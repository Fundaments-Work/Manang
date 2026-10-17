<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import SlideFrame from "./SlideFrame.svelte";
  import SlideStrip from "./SlideStrip.svelte";
  import { toggleSlides } from "$lib/state/actions";
  import { documents } from "$lib/state/documents.svelte";
  import { slides } from "$lib/state/slides.svelte";
  import { parseSlides } from "$lib/workbench/slides";

  /**
   * Slides view: a Typst document seen as a deck. Every heading starts a slide.
   * It lives inside the document's tab (the Text and Slides switch at the right
   * of the tab strip), not in a separate app. A thumbnail strip on the left, the
   * current slide in the middle, and a Present button for full screen.
   */
  interface Props {
    path: string;
  }

  const { path }: Props = $props();

  const deck = $derived.by(() => {
    void documents.version;
    return parseSlides(documents.get(path)?.text() ?? "");
  });
  const current = $derived(deck[Math.min(slides.index, Math.max(0, deck.length - 1))]);

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "ArrowRight" || event.key === "PageDown")
      slides.next(deck.length);
    else if (event.key === "ArrowLeft" || event.key === "PageUp")
      slides.previous(deck.length);
    else return;
    event.preventDefault();
  }
</script>

<div class="flex h-full min-h-0 w-full min-w-0 flex-1 flex-col bg-bg">
  <div
    class="flex h-9 shrink-0 items-center gap-2 border-b border-line px-3"
    role="toolbar"
    aria-label="Slides controls"
  >
    <span class="text-xs text-text-60" aria-live="polite">
      {#if deck.length > 0}Slide {slides.index + 1} of {deck.length}{:else}No slides{/if}
    </span>
    <span class="ml-auto"></span>
    <button
      type="button"
      class="mng-btn h-6 px-2"
      disabled={deck.length === 0}
      onclick={() => slides.present()}
    >
      <Icon name="present" size={13} /> Present
      <span class="mng-kbd">F5</span>
    </button>
  </div>

  {#if deck.length === 0}
    <div class="mx-auto max-w-sm px-6 pt-20 text-xs leading-relaxed text-text-60">
      <p class="m-0 text-text">This document has no slides yet</p>
      <p class="mt-2">
        Each heading becomes a slide. Add one in the text view, for example <span
          class="font-mono text-[11px]">== My first slide</span
        >.
      </p>
      <button type="button" class="mng-btn mt-3" onclick={() => toggleSlides()}
        >Back to text</button
      >
    </div>
  {:else}
    <div class="flex min-h-0 flex-1">
      <SlideStrip {deck} />
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <div
        class="flex min-w-0 flex-1 items-center justify-center overflow-auto p-8"
        role="region"
        aria-label="Current slide"
        tabindex="0"
        {onkeydown}
      >
        {#if current !== undefined}
          <div class="w-full max-w-4xl">
            <SlideFrame slide={current} total={deck.length} />
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>
