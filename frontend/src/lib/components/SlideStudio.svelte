<script lang="ts">
  import Mark from "./Mark.svelte";
  import SlideCarousel from "./SlideCarousel.svelte";
  import SlideFrame from "./SlideFrame.svelte";
  import Overlay from "./ui/Overlay.svelte";
  import { desk } from "$lib/state/desk.svelte";

  /**
   * The Presentation Studio — DESIGN.md §8.
   *
   * Adapts the live sheet into a 16:9 studio when the manuscript targets
   * Touying or Polylux. In presentation mode the carousel dissolves too: the
   * audience sees one slide, not the author's tooling.
   */
</script>

{#if desk.slideMode}
  <Overlay side="none">
    <section class="flex h-full w-full flex-col bg-bg" aria-label="Presentation studio">
      <header class="flex h-10 shrink-0 items-center gap-4 border-b border-line px-4">
        <div class="flex items-center gap-2">
          <Mark class="text-text" />
          <span
            class="font-sans text-[15px] font-light text-text"
            style="letter-spacing: var(--tracking-headline)"
          >
            Manang
          </span>
        </div>

        <span class="font-mono text-xs text-text-60">slides / quantum_talk.typ</span>

        <div class="ml-auto flex items-center gap-1.5">
          <button
            type="button"
            class="flex h-7 items-center gap-1.5 border border-line bg-surface px-2 font-sans text-xs text-text-60 transition-colors duration-fast ease-manang hover:bg-surface-hi hover:text-text"
            onclick={() => (desk.presenting = true)}
          >
            Present
            <span class="font-mono text-[10px] text-text-muted">F5</span>
          </button>

          <button
            type="button"
            class="flex h-7 w-7 items-center justify-center border border-line bg-surface text-text-60 transition-colors duration-fast ease-manang hover:bg-surface-hi hover:text-text"
            aria-label="Close presentation studio"
            onclick={() => (desk.slideMode = false)}
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
      </header>

      <div class="min-h-0 flex-1">
        <SlideFrame />
      </div>

      {#if !desk.presenting}
        <SlideCarousel />
      {/if}
    </section>
  </Overlay>
{/if}
