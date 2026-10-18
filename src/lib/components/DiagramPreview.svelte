<script lang="ts">
  import { diagram } from "$lib/state/diagram.svelte";
  import { layout } from "$lib/state/layout.svelte";

  /**
   * The rendered diagram of the active diagram file. The SVG comes from the
   * `diagram` plugin as bytes and is shown through a Blob URL in an `<img>`, so
   * nothing inside it (scripts, links) can run. While the source has an error
   * the last good picture stays, dimmed; the error is in the header and in
   * Problems.
   */

  const snapshot = $derived(diagram.snapshot);
</script>

{#if snapshot.url !== null}
  <div class="flex min-h-full justify-center px-6 py-6">
    <img
      class="block h-auto max-w-full self-start transition-opacity duration-150"
      class:opacity-40={snapshot.stale}
      src={snapshot.url}
      alt="{diagram.name} diagram{snapshot.stale ? ', out of date' : ''}"
      data-diagram
      data-theme={snapshot.theme}
      data-stale={snapshot.stale ? "" : undefined}
    />
  </div>
{:else if snapshot.error !== ""}
  <div class="mx-auto max-w-sm px-6 pt-16 text-xs leading-relaxed text-text-60">
    <p class="m-0 text-text">{snapshot.error}</p>
    <p class="mt-2">The diagram could not be drawn, so there is nothing to show yet.</p>
    <button
      type="button"
      class="mng-btn mt-3"
      onclick={() => layout.showPanel("problems")}>Show problems</button
    >
  </div>
{:else}
  <p class="m-0 px-6 pt-16 text-center text-xs text-text-muted" role="status">
    Drawing the diagram
  </p>
{/if}
