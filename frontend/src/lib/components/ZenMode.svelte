<script lang="ts">
  import { desk } from "$lib/state/desk.svelte";
  import { document } from "$lib/state/document.svelte";

  /**
   * Zen Mode — DESIGN.md §9.
   *
   * Zero chrome: header, rails, status and preview all dissolve. A 680px column
   * keeps the measure at 70–80 characters, which is the whole reason to read in
   * this mode at all. Esc or the same shortcut brings the Desk straight back.
   */
</script>

{#if desk.zen}
  <div class="h-full overflow-y-auto bg-bg">
    <article class="mx-auto max-w-[680px] px-6 py-24">
      <h1
        class="font-sans text-3xl font-light text-text"
        style="letter-spacing: var(--tracking-display)"
      >
        = Quantum Thermodynamic Bounds
      </h1>

      <p
        class="mt-6 font-sans text-base leading-relaxed text-text"
        style="letter-spacing: var(--tracking-body)"
      >
        Under unitary coupling to a heat reservoir at inverse temperature
        <span class="font-mono text-math">β = (k_B T)⁻¹</span>, the irreversible entropy
        production rate
        <span class="font-mono text-math">Σ</span> satisfies the finite-time lower bound:
      </p>

      <p class="my-10 text-center font-mono text-lg text-math">
        ΔS_irr ≥ ∫₀ᵗ δQ / T + k_B D
      </p>

      <p
        class="font-sans text-base leading-relaxed text-text"
        style="letter-spacing: var(--tracking-body)"
      >
        This confirms that quantum coherence retains finite work capacity even under
        thermal dissipation.
      </p>

      <p class="mt-10 font-mono text-xs text-text-muted">
        {document.wordCount} words · {document.lineCount} lines
      </p>
    </article>
  </div>
{/if}
