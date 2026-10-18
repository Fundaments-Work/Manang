<script lang="ts">
  import { onMount } from "svelte";

  import { build } from "$lib/state/build.svelte";
  import { engine } from "$lib/state/engine.svelte";
  import { downloadLabel, formatSize, unavailableReason } from "$lib/workbench/engine";

  /**
   * Settings > LaTeX: the optional built-in engine. Its status and version, the
   * download (through the same consent dialog as everywhere else), Remove, and
   * which engine the last compile used. The engine choice itself (automatic,
   * system TeX, built-in) is the `latex.backend` row above.
   */

  onMount(() => {
    void engine.refresh();
  });

  const status = $derived(engine.status);
</script>

{#if status !== null}
  <div
    class="mt-4 border-t border-line pt-4 text-xs leading-relaxed text-text-60"
    data-latex-engine
  >
    <h4 class="m-0 text-[13px] font-normal text-text">Built-in engine</h4>
    <p class="m-0 mt-1 max-w-[60ch]">
      A program Manang can download to compile LaTeX when you have no TeX installed. It
      handles most documents; it does not run <span class="font-mono text-[11px]"
        >pdflatex</span
      >
      or LuaLaTeX.
    </p>

    <p class="m-0 mt-2 text-text" data-engine-state>
      {#if status.installed}
        Installed: Tectonic {status.version}{status.size === undefined
          ? ""
          : ` (${formatSize(status.size)})`}
      {:else if status.installing}
        Downloading…
      {:else if status.downloadable}
        Not installed
      {:else}
        Not available
      {/if}
    </p>
    {#if status.installed && status.stored_in}
      <p class="selectable m-0 mt-1 font-mono text-[11px] break-all text-text-muted">
        {status.stored_in}
      </p>
    {/if}
    {#if !status.downloadable && !status.installed}
      <p class="m-0 mt-1 max-w-[60ch]">{unavailableReason(status.reason)}</p>
    {/if}
    {#if build.backend !== ""}
      <p class="m-0 mt-1" data-engine-used>Last compile used: {build.backend}</p>
    {/if}

    <div class="mt-3 flex flex-wrap gap-2">
      {#if status.installed}
        <button
          type="button"
          class="mng-btn"
          data-engine-remove
          disabled={engine.removing}
          onclick={() => void engine.remove()}>Remove the built-in engine</button
        >
      {:else if status.downloadable}
        <button
          type="button"
          class="mng-btn"
          data-engine-offer
          disabled={status.installing}
          onclick={() => void engine.openOffer()}>{downloadLabel(status.size)}</button
        >
      {/if}
    </div>
  </div>
{/if}
