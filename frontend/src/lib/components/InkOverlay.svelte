<script lang="ts">
  import InkCanvas from "./InkCanvas.svelte";
  import Mark from "./Mark.svelte";
  import Overlay from "./ui/Overlay.svelte";
  import { desk } from "$lib/state/desk.svelte";

  /**
   * The Canvas — DESIGN.md §4.
   *
   * Glides in over the writing pane with no modal border and no backdrop
   * darkness. The header keeps the breadcrumb so the author never loses track of
   * which figure they are editing.
   */

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      desk.canvasOpen = false;
    }
  }
</script>

<svelte:window {onkeydown} />

{#if desk.canvasOpen}
  <Overlay side="none">
    <section class="flex h-full w-full flex-col bg-bg" aria-label="Vector ink canvas">
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

        <span class="font-mono text-xs text-text-60">
          figures / vectors / state_lattice.svg
        </span>

        <div class="ml-auto flex items-center gap-1.5">
          <button
            type="button"
            class="flex h-7 items-center gap-1.5 border border-line bg-surface px-2 font-sans text-xs text-text transition-colors duration-fast ease-manang hover:bg-surface-hi"
            onclick={() => desk.toggleCanvas()}
          >
            Done
            <span class="font-mono text-[10px] text-text-muted">↵</span>
          </button>
          <button
            type="button"
            class="flex h-7 w-7 items-center justify-center border border-line bg-surface text-text-60 transition-colors duration-fast ease-manang hover:bg-surface-hi hover:text-text"
            aria-label="Close canvas"
            onclick={() => (desk.canvasOpen = false)}
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

      <InkCanvas />
    </section>
  </Overlay>
{/if}
