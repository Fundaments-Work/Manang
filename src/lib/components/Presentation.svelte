<script lang="ts">
  import { onMount } from "svelte";

  import SlideFrame from "./SlideFrame.svelte";
  import { documents } from "$lib/state/documents.svelte";
  import { slides } from "$lib/state/slides.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { parseSlides } from "$lib/workbench/slides";

  /**
   * Full-window presentation. Right, Space and PageDown advance; Left and
   * PageUp go back; Esc leaves. The author's tooling is gone: the audience sees
   * one slide.
   */

  let root: HTMLDivElement | undefined = $state();

  const deck = $derived(
    parseSlides(documents.get(tabs.active?.path ?? "")?.text() ?? ""),
  );
  const current = $derived(deck[Math.min(slides.index, Math.max(0, deck.length - 1))]);

  onMount(() => {
    const returnTo =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    root?.focus();
    return () => returnTo?.focus();
  });

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") slides.stop();
    else if (["ArrowRight", " ", "PageDown", "Enter"].includes(event.key))
      slides.next(deck.length);
    else if (["ArrowLeft", "PageUp", "Backspace"].includes(event.key))
      slides.previous(deck.length);
    else return;
    event.preventDefault();
    event.stopPropagation();
  }
</script>

<div
  bind:this={root}
  role="dialog"
  aria-modal="true"
  aria-label="Presentation"
  tabindex="-1"
  class="fixed inset-0 z-50 flex items-center justify-center bg-bg p-6 outline-none"
  {onkeydown}
>
  {#if current !== undefined}
    <button
      type="button"
      tabindex="-1"
      class="w-full max-w-[min(100%,calc((100vh-48px)*16/9))] cursor-default"
      aria-label="Next slide"
      onclick={() => slides.next(deck.length)}
    >
      <SlideFrame slide={current} total={deck.length} />
    </button>
  {/if}
  <p
    class="absolute bottom-3 left-1/2 m-0 -translate-x-1/2 text-[11px] text-text-muted"
    aria-live="polite"
  >
    Slide {slides.index + 1} of {deck.length}. Esc to leave.
  </p>
</div>
