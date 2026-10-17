<script lang="ts">
  import { document } from "$lib/state/document.svelte";
  import { preview } from "$lib/state/preview.svelte";

  /**
   * Kernel-rendered preview — DESIGN.md §3.3.
   *
   * Shows the binary artefact produced by `typst.render` or
   * `latex.export-pdf`. Loaded lazily by PaperPane, so the viewer costs nothing
   * until a producing plugin exists. Three quiet states: loading, empty, error.
   */

  $effect(() => {
    // Re-render when the manuscript settles; the store debounces.
    void document.text;
    preview.schedule();
  });

  $effect(() => () => preview.release());
</script>

<section
  class="relative flex h-full min-w-0 flex-1 flex-col overflow-hidden bg-bg"
  aria-label="Rendered preview"
  aria-busy={preview.status === "loading"}
>
  {#if preview.url !== null && preview.status !== "error"}
    {#if preview.kind === "pdf"}
      <object
        class="h-full w-full flex-1 bg-paper"
        data={preview.url}
        type="application/pdf"
        aria-label="Rendered PDF"
      >
        <p class="p-6 font-sans text-xs text-text-60">
          This view cannot display PDFs inline.
        </p>
      </object>
    {:else}
      <div class="flex-1 overflow-y-auto px-8 py-6">
        <img
          class="mx-auto w-full max-w-[42rem] border border-line bg-paper"
          src={preview.url}
          alt="Rendered manuscript"
        />
      </div>
    {/if}
  {:else if preview.status === "error"}
    <div class="m-auto max-w-[42rem] px-8 text-center" role="alert">
      <p class="font-mono text-[10px] tracking-[0.06em] text-text-60 uppercase">
        render failed
      </p>
      <p class="mt-2 font-sans text-xs text-text">{preview.message}</p>
    </div>
  {:else if preview.status === "loading"}
    <p
      class="m-auto font-mono text-[10px] tracking-[0.06em] text-text-muted uppercase"
      role="status"
    >
      rendering…
    </p>
  {:else}
    <p class="m-auto font-sans text-xs text-text-muted">Nothing to preview yet.</p>
  {/if}

  {#if preview.status === "loading" && preview.url !== null}
    <!-- Keep the last good page visible while the next one renders. -->
    <span
      class="absolute top-2 right-3 font-mono text-[10px] text-text-muted"
      role="status">rendering…</span
    >
  {/if}
</section>
