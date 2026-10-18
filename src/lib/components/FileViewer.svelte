<script lang="ts">
  import { viewers } from "$lib/state/viewers.svelte";
  import { basename } from "$lib/workbench/paths";

  /**
   * A read-only tab for an image or a PDF from the project.
   *
   * An image fits the tab (never enlarged past its own size); a click, or the
   * toolbar button, shows it at 1:1 and scrolls. The ground is the plain base
   * colour, no checkerboard. A PDF opens in the PDF viewer, fetched the first
   * time a PDF tab needs it.
   */
  interface Props {
    path: string;
  }

  const { path }: Props = $props();

  const file = $derived(viewers.get(path));

  let actual = $state(false);
  let natural = $state<{ w: number; h: number } | null>(null);
  let broken = $state(false);

  // PDF viewer state, local to this tab.
  let page = $state(0);
  let pages = $state(0);
  let pdfError = $state("");

  $effect(() => {
    // A different file (or a reload) starts fitted, with fresh dimensions.
    void file?.url;
    actual = false;
    natural = null;
    broken = false;
  });

  function size(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
</script>

<div
  class="flex h-full min-h-0 w-full min-w-0 flex-1 flex-col bg-bg"
  data-viewer={path}
>
  <div
    class="flex h-9 shrink-0 items-center gap-3 border-b border-line px-3 text-xs text-text-60"
    role="toolbar"
    aria-label="Viewer"
  >
    <span class="truncate text-text">{basename(path)}</span>
    {#if file !== undefined}
      <span class="font-mono text-[11px]">{size(file.size)}</span>
    {/if}
    {#if natural !== null}
      <span class="font-mono text-[11px]">{natural.w} by {natural.h} px</span>
    {/if}
    {#if file?.kind === "pdf" && pages > 0}
      <span class="font-mono text-[11px]">page {page + 1} of {pages}</span>
    {/if}
    <span class="ml-auto"></span>
    {#if file?.kind === "image"}
      <button
        type="button"
        class="mng-btn h-7 border-transparent bg-transparent px-2 text-text-60 hover:text-text aria-pressed:border-text-60 aria-pressed:text-text"
        aria-pressed={actual}
        title="Show the image at its own size (click the image to switch)"
        onclick={() => (actual = !actual)}
      >
        1:1
      </button>
    {/if}
  </div>

  {#if file === undefined}
    <p class="m-auto text-xs text-text-60" role="status">Opening {basename(path)}</p>
  {:else if file.kind === "image" && file.url !== null}
    <div
      class="flex min-h-0 flex-1 overflow-auto {actual
        ? ''
        : 'items-center justify-center p-6'}"
      role="region"
      aria-label="Image {basename(path)}"
    >
      {#if broken}
        <p class="m-auto text-xs text-text-60" role="alert">
          This file could not be shown as an image. It may be damaged or in an
          unsupported format.
        </p>
      {:else}
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_click_events_have_key_events -->
        <img
          src={file.url}
          alt={basename(path)}
          draggable="false"
          class="block select-none {actual
            ? 'm-auto max-w-none cursor-zoom-out'
            : 'max-h-full max-w-full cursor-zoom-in object-contain'}"
          onload={(event) => {
            const img = event.currentTarget as HTMLImageElement;
            natural = { w: img.naturalWidth, h: img.naturalHeight };
          }}
          onerror={() => (broken = true)}
          onclick={() => (actual = !actual)}
        />
      {/if}
    </div>
  {:else if file.kind === "pdf" && file.pdf !== null}
    <div
      class="relative min-h-0 flex-1"
      role="region"
      aria-label="PDF {basename(path)}"
    >
      {#if pdfError !== ""}
        <p class="m-0 px-6 pt-16 text-center text-xs text-text-60" role="alert">
          {pdfError}
        </p>
      {:else}
        {#await import("./PdfViewer.svelte") then viewer}
          <viewer.default
            data={file.pdf}
            zoom={null}
            jump={0}
            page={0}
            onpage={(next: number) => (page = next)}
            onpages={(count: number) => (pages = count)}
            onerror={(message: string) => (pdfError = message)}
          />
        {:catch}
          <p class="m-0 px-6 pt-16 text-center text-xs text-text-60" role="alert">
            The PDF viewer could not be loaded. Restart Manang and try again.
          </p>
        {/await}
      {/if}
    </div>
  {/if}
</div>
