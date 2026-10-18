<script lang="ts">
  import { onMount } from "svelte";

  import { engine } from "$lib/state/engine.svelte";
  import { downloadLabel } from "$lib/workbench/engine";

  /**
   * "Download the built-in engine (about NN MB)": shown wherever LaTeX cannot
   * compile because no TeX is installed. It only opens the consent dialog; the
   * download starts when the person presses Download there. Renders nothing
   * when this build cannot offer the engine (so the surrounding text, which
   * also names TeX Live and MiKTeX, stands on its own).
   */

  onMount(() => {
    void engine.refresh();
  });
</script>

{#if engine.offered}
  <button
    type="button"
    class="mng-btn"
    data-engine-offer
    onclick={() => void engine.openOffer()}>{downloadLabel(engine.status?.size)}</button
  >
{/if}
