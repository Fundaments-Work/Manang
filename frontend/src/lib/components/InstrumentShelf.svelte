<script lang="ts">
  import Overlay from "./ui/Overlay.svelte";
  import { capabilitySummary, instruments } from "$lib/state/instruments.svelte";
  import { desk } from "$lib/state/desk.svelte";

  /**
   * The Instrument Shelf — DESIGN.md §6.
   *
   * Manang is plugin-first: compilers, drawing engines and verifiers are all
   * isolated WebAssembly instruments. The card therefore shows what an
   * instrument may touch, not just its version — a plugin manager that hides
   * permissions is a trust prompt with nicer typography.
   */
</script>

{#if desk.shelfOpen}
  <Overlay side="none">
    <section class="flex h-full w-full flex-col bg-bg" aria-label="Instrument shelf">
      <header
        class="flex h-10 shrink-0 items-center justify-between border-b border-line px-4"
      >
        <h2 class="font-mono text-xs text-text-60" style="letter-spacing: 0.04em">
          INSTRUMENT SHELF
          <span class="ml-2 text-text-muted">
            {instruments.enabledCount}/{instruments.instruments.length} enabled
          </span>
        </h2>
        <button
          type="button"
          class="flex h-6 w-6 items-center justify-center border border-transparent text-text-60 transition-colors duration-fast ease-manang hover:border-line hover:text-text"
          aria-label="Close instrument shelf"
          onclick={() => (desk.shelfOpen = false)}
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
      </header>

      <div class="min-h-0 flex-1 overflow-y-auto p-6">
        <!-- Cards carry their own borders rather than relying on a background
             showing through grid gaps: with an odd number of cards the exposed
             background leaves a stray hairline hanging below the last row. -->
        <ul class="mx-auto grid max-w-4xl grid-cols-1 gap-2 sm:grid-cols-2">
          {#each instruments.instruments as instrument (instrument.id)}
            {@const enabled = instruments.isEnabled(instrument.id)}
            <li class="group border border-line bg-bg">
              <div class="flex items-start gap-3 p-4">
                <!-- Square zero-radius checkbox: enabling sandboxes instantly. -->
                <button
                  type="button"
                  role="switch"
                  aria-checked={enabled}
                  aria-label="Enable {instrument.name}"
                  disabled={instrument.essential === true}
                  onclick={() => instruments.toggle(instrument.id)}
                  class="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center border transition-colors duration-fast ease-manang"
                  class:border-accent={enabled}
                  class:bg-accent={enabled}
                  class:border-line={!enabled}
                  class:cursor-not-allowed={instrument.essential === true}
                  class:opacity-60={instrument.essential === true}
                >
                  {#if enabled}
                    <svg
                      viewBox="0 0 12 12"
                      width="10"
                      height="10"
                      fill="none"
                      stroke="var(--mng-accent-ink)"
                      stroke-width="1.75"
                      aria-hidden="true"
                    >
                      <path d="M2.5 6.5 L5 9 L9.5 3.5" />
                    </svg>
                  {/if}
                </button>

                <div class="min-w-0 flex-1">
                  <div class="flex items-baseline gap-2">
                    <h3 class="truncate font-sans text-sm font-light text-text">
                      {instrument.name}
                    </h3>
                    <span class="shrink-0 font-mono text-[10px] text-text-muted">
                      v{instrument.version}
                    </span>
                  </div>

                  <p class="mt-0.5 font-sans text-xs text-text-60">
                    {instrument.description}
                  </p>

                  <p class="mt-2 font-mono text-[10px] text-text-muted">
                    {instrument.kind}
                    {#if instrument.essential === true}
                      <span class="ml-1 text-text-60">· always on</span>
                    {/if}
                  </p>

                  <!-- Capability transparency: always one hover away. -->
                  <p
                    class="mt-2 font-mono text-[10px] leading-relaxed text-text-muted opacity-0 transition-opacity duration-fast ease-manang group-hover:opacity-100"
                  >
                    {capabilitySummary(instrument.capabilities)}
                  </p>
                </div>
              </div>
            </li>
          {/each}
        </ul>
      </div>

      <footer
        class="flex shrink-0 items-center justify-between border-t border-line px-4 py-3"
      >
        <button
          type="button"
          class="border border-line px-3 py-1.5 font-sans text-xs text-text-60 transition-colors duration-fast ease-manang hover:bg-surface-hi hover:text-text"
        >
          + Install WASM Plugin (.wasm)
        </button>
        <button
          type="button"
          class="font-sans text-xs text-text-60 transition-colors duration-fast ease-manang hover:text-text"
        >
          Browse Curated Lab
        </button>
      </footer>
    </section>
  </Overlay>
{/if}
