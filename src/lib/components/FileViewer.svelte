<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import { openAsText, openInSystemApp, revealFile } from "$lib/state/actions";
  import { PdfView } from "$lib/state/pdfView.svelte";
  import { viewers } from "$lib/state/viewers.svelte";
  import {
    PREVIEW_BYTES,
    describeFile,
    formatModified,
    formatSize,
  } from "$lib/workbench/opener";
  import { basename, extension } from "$lib/workbench/paths";

  /**
   * A read-only tab for a file the text editor does not edit: an image, a PDF,
   * text too large to edit, or anything else (an info card).
   *
   * An image fits the tab (never enlarged past its own size); a click, or the
   * toolbar button, shows it at 1:1 and scrolls. The ground is the plain base
   * colour, no checkerboard. A PDF opens in the PDF viewer, fetched the first
   * time a PDF tab needs it. What a file opens as is decided in
   * `workbench/opener.ts`, not here.
   */
  interface Props {
    path: string;
  }

  const { path }: Props = $props();

  const file = $derived(viewers.get(path));
  const name = $derived(basename(path));

  let actual = $state(false);
  let natural = $state<{ w: number; h: number } | null>(null);
  let broken = $state(false);

  // PDF viewer state, local to this tab.
  const pdfView = new PdfView();
  let pdfError = $state("");

  $effect(() => {
    // A different file (or a reload) starts fitted, with fresh dimensions.
    void file?.url;
    actual = false;
    natural = null;
    broken = false;
  });

  const quiet =
    "mng-btn h-7 border-transparent bg-transparent px-2 text-text-60 hover:text-text";
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
    <span class="truncate text-text">{name}</span>
    {#if file !== undefined}
      <span class="font-mono text-[11px]">{formatSize(file.size)}</span>
    {/if}
    {#if natural !== null}
      <span class="font-mono text-[11px]">{natural.w} by {natural.h} px</span>
    {/if}
    {#if file?.kind === "pdf" && pdfView.pageCount > 0}
      <span class="font-mono text-[11px]"
        >page {pdfView.page + 1} of {pdfView.pageCount}</span
      >
    {/if}
    <span class="ml-auto"></span>
    {#if file?.kind === "image"}
      {#if extension(path) === "svg"}
        <button
          type="button"
          class={quiet}
          title="Edit the SVG source"
          onclick={() => void openAsText(path)}
        >
          Open as text
        </button>
      {/if}
      <button
        type="button"
        class="{quiet} aria-pressed:border-text-60 aria-pressed:text-text"
        aria-pressed={actual}
        title="Show the image at its own size (click the image to switch)"
        onclick={() => (actual = !actual)}
      >
        1:1
      </button>
    {/if}
    {#if file !== undefined && file.kind !== "binary"}
      <button
        type="button"
        class="mng-icon-btn"
        aria-label="Open with system app"
        title="Open with system app"
        onclick={() => void openInSystemApp(path)}
      >
        <Icon name="external" size={14} />
      </button>
    {/if}
  </div>

  {#if file === undefined}
    <p class="m-auto text-xs text-text-60" role="status">Opening {name}</p>
  {:else if file.kind === "image" && file.url !== null}
    <div
      class="flex min-h-0 flex-1 overflow-auto {actual
        ? ''
        : 'items-center justify-center p-6'}"
      role="region"
      aria-label="Image {name}"
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
          alt={name}
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
    <div class="relative min-h-0 flex-1" role="region" aria-label="PDF {name}">
      {#if pdfError !== ""}
        <p class="m-0 px-6 pt-16 text-center text-xs text-text-60" role="alert">
          {pdfError}
        </p>
      {:else}
        {#await import("./PdfViewer.svelte") then viewer}
          <viewer.default
            data={file.pdf}
            view={pdfView}
            docKey={path}
            onerror={(message: string) => (pdfError = message)}
          />
        {:catch}
          <p class="m-0 px-6 pt-16 text-center text-xs text-text-60" role="alert">
            The PDF viewer could not be loaded. Restart Manang and try again.
          </p>
        {/await}
      {/if}
    </div>
  {:else if file.kind === "large-text" && file.preview !== null}
    <div class="flex min-h-0 flex-1 flex-col" role="region" aria-label="Text {name}">
      <p class="m-0 border-b border-line px-3 py-2 text-xs text-text-60" role="note">
        Too large to edit here. Read-only{file.size > PREVIEW_BYTES
          ? `, showing the first ${formatSize(PREVIEW_BYTES)}`
          : ""}. Open it in another app to edit it.
      </p>
      <pre
        class="selectable m-0 min-h-0 flex-1 overflow-auto px-4 py-3 font-mono text-[12px] leading-relaxed whitespace-pre text-text">{file.preview}</pre>
    </div>
  {:else}
    <div
      class="mx-auto flex h-full w-full max-w-[420px] flex-col justify-center px-8 text-xs leading-relaxed text-text-60"
      role="region"
      aria-label="File {name}"
    >
      <p class="m-0 text-[13px] text-text">{name}</p>
      <dl class="mt-3 mb-4 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1">
        <dt>Type</dt>
        <dd class="m-0 text-text">{describeFile(path)}</dd>
        <dt>Size</dt>
        <dd class="m-0 font-mono text-text">{formatSize(file.size)}</dd>
        {#if file.modified !== null}
          <dt>Modified</dt>
          <dd class="m-0 font-mono text-text">{formatModified(file.modified)}</dd>
        {/if}
        <dt>Path</dt>
        <dd class="m-0 truncate font-mono text-text" title={path}>{path}</dd>
      </dl>
      <p class="m-0 mb-4">Manang cannot show this kind of file.</p>
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          class="mng-btn"
          onclick={() => void openInSystemApp(path)}
        >
          <Icon name="external" size={13} /> Open with system app
        </button>
        <button type="button" class="mng-btn" onclick={() => void revealFile(path)}>
          <Icon name="folder-open" size={13} /> Show in file manager
        </button>
      </div>
    </div>
  {/if}
</div>
