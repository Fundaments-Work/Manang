<script lang="ts">
  /**
   * Live paper sheet — DESIGN.md §3.3.
   *
   * Placeholder until the Typst pipeline streams rendered pages. The sheet uses
   * the paper tokens rather than the app theme: a page of paper is not a themed
   * surface, and it must read identically on screen and in print.
   */
  import { onMount, type Component } from "svelte";

  import { preview } from "$lib/state/preview.svelte";

  /**
   * When a plugin provides `typst.render` or `latex.export-pdf`, the kernel's
   * artefact replaces the sample sheet. The viewer is a separate chunk, fetched
   * only once such a command is detected.
   */
  let Viewer = $state.raw<Component | undefined>();

  onMount(() => {
    void preview.detect();
  });

  $effect(() => {
    if (preview.available && Viewer === undefined) {
      void import("./PreviewPane.svelte").then((module) => {
        Viewer = module.default;
      });
    }
  });

  const blocks: readonly {
    kind: "heading" | "body" | "math" | "cite";
    text: string;
  }[] = [
    { kind: "heading", text: "2. Quantum Thermodynamic Bounds" },
    {
      kind: "body",
      text: "Under unitary coupling to a heat reservoir at inverse temperature, the irreversible entropy production rate satisfies the finite-time lower bound",
    },
    { kind: "math", text: "ΔS_irr  ≥  ∫₀ᵗ δQ / T  +  k_B D" },
    { kind: "cite", text: "[1] Landauer, R. (1961) IBM J. Res. Dev. 5, 215" },
  ];
</script>

{#if Viewer !== undefined && preview.available}
  <Viewer />
{:else}
  <section
    class="flex h-full min-w-0 flex-1 items-start justify-center overflow-y-auto bg-bg px-8 py-6"
    aria-label="Rendered paper"
  >
    <article
      class="w-full max-w-[42rem] border border-line bg-paper px-10 py-12 text-paper-ink"
    >
      {#each blocks as block (block.text)}
        {#if block.kind === "heading"}
          <h2
            class="font-sans text-lg font-light"
            style="letter-spacing: var(--tracking-display)"
          >
            {block.text}
          </h2>
        {:else if block.kind === "math"}
          <p class="my-6 text-center font-mono text-base">$$ {block.text} $$</p>
        {:else if block.kind === "cite"}
          <p class="mt-8 font-mono text-xs text-paper-ink-60">{block.text}</p>
        {:else}
          <p
            class="mt-4 font-sans text-sm"
            style="letter-spacing: var(--tracking-body)"
          >
            {block.text}
          </p>
        {/if}
      {/each}
    </article>
  </section>
{/if}
