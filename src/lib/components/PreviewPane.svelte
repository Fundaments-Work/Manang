<script lang="ts">
  import { untrack } from "svelte";

  import DiagramPreview from "$lib/components/DiagramPreview.svelte";
  import PdfControls from "$lib/components/PdfControls.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { formatShortcut } from "$lib/commands/shortcut";
  import { reportError } from "$lib/kernel";
  import { exportPdf } from "$lib/state/actions";
  import { build, formatMs } from "$lib/state/build.svelte";
  import { diagram } from "$lib/state/diagram.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { preview } from "$lib/state/preview.svelte";
  import { settings } from "$lib/state/settings.svelte";
  import { jumpToSource } from "$lib/state/sourceJump";
  import { tabs } from "$lib/state/tabs.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { detectPlatform } from "$lib/util/keyboard";
  import { countLabel } from "$lib/workbench/problems";

  /**
   * The preview: the page the engine rendered, with a small toolbar for page,
   * zoom, refresh and export. The page is paper: it keeps the paper tokens in
   * every theme so what is on screen matches what prints.
   *
   * Typst and LaTeX show their PDF in one viewer (pdf.js, loaded on first use:
   * selection, find, links, outline, zoom), Markdown the sanitised HTML.
   * States: compiling (the last pages stay visible), ready, failed (last good
   * pages plus a way to the errors), and engine trouble (a message that says how to fix it, never a crash).
   */

  const engine = $derived(
    tabs.active === undefined
      ? workspace.engine
      : workspace.engineFor(tabs.active.path),
  );
  const ENGINE_NAMES = {
    typst: "Typst",
    latex: "LaTeX",
    markdown: "Markdown",
  } as const;

  $effect(() => {
    void build.generation;
    untrack(() => void preview.render());
  });

  const kind = $derived(preview.kind);
  /** A diagram file shows its own picture, not the project's pages. */
  const isDiagram = $derived(diagram.active);
  const paged = $derived(!isDiagram && kind === "pdf");
  const settled = $derived(
    build.status === "ok" || build.status === "failed" || build.status === "compiling",
  );
  const shown = $derived(
    !isDiagram &&
      settled &&
      (kind === "html"
        ? build.html !== ""
        : kind === "pdf" &&
          preview.pdf !== null &&
          preview.shownTarget === build.target),
  );

  const exportable = $derived(
    kind !== "html" && (build.status === "ok" || build.status === "failed"),
  );
  const exportHint = formatShortcut({ key: "e", mod: true }, detectPlatform());
  const pages = $derived(preview.pageCount);
  const banner = $derived(
    build.failure !== null ? build.failure.title : countLabel(build.counts),
  );

  /** The Dark pages toggle writes the setting, so it sticks. */
  async function setDarkPages(dark: boolean): Promise<void> {
    preview.view.darkPages = dark;
    const failed = await settings.set("preview.dark-pages", dark, "user");
    if (failed !== "") reportError("preview.dark-pages", new Error(failed));
  }
</script>

<section class="flex h-full min-h-0 min-w-0 flex-col bg-bg" aria-label="Preview">
  <div
    class="@container flex h-9 shrink-0 items-center gap-0.5 border-b border-line px-2"
    role="toolbar"
    aria-label="Preview controls"
  >
    {#if isDiagram}
      <span
        class="shrink-0 px-1.5 font-mono text-[11px] whitespace-nowrap text-text-60 @max-[240px]:hidden"
        >{diagram.name}</span
      >
      {#if diagram.snapshot.error !== ""}
        <button
          type="button"
          class="flex min-w-0 items-center gap-1.5 px-1.5 font-mono text-[11px] text-text hover:text-text-60"
          title="{diagram.snapshot.error}. Show problems"
          data-diagram-error
          onclick={() => layout.showPanel("problems")}
        >
          <Icon name="warning" size={13} />
          <span class="truncate">{diagram.snapshot.error}</span>
        </button>
      {/if}
    {:else if paged}
      <PdfControls
        view={preview.view}
        {pages}
        ondarkpages={(dark: boolean) => void setDarkPages(dark)}
      />
    {:else if kind === "html"}
      <span
        class="min-w-0 truncate px-1.5 font-mono text-[11px] text-text-60 @max-[240px]:hidden"
        >Markdown preview</span
      >
    {/if}

    {#if build.viaMain && build.target !== ""}
      <span
        class="ml-2 min-w-0 truncate font-mono text-[11px] text-text-muted"
        title="This file is compiled through the project's main file"
      >
        via {build.target}
      </span>
    {/if}

    {#if !isDiagram && kind === "pdf" && build.backend !== ""}
      <span
        class="ml-2 min-w-0 truncate font-mono text-[11px] text-text-muted"
        title="The TeX engine that compiled this document"
      >
        {build.backend}
      </span>
    {/if}

    <span class="ml-auto"></span>

    <button
      type="button"
      class="mng-icon-btn @max-[180px]:hidden"
      aria-label="Refresh preview"
      title={isDiagram ? "Draw the diagram again" : "Compile now"}
      disabled={!isDiagram &&
        (build.status === "unsupported" || build.status === "unavailable")}
      onclick={() => (isDiagram ? diagram.refresh() : void build.compile(true))}
    >
      <Icon name="refresh" size={14} />
    </button>
    <button
      type="button"
      class="mng-icon-btn"
      aria-label="Export PDF"
      title={isDiagram
        ? "Export the document that includes this diagram"
        : `Export PDF (${exportHint})`}
      aria-keyshortcuts="Control+E"
      disabled={isDiagram || !exportable || preview.exporting}
      onclick={() => exportPdf()}
    >
      <Icon name="export" size={14} />
    </button>

    <span class="mx-1.5 h-4 w-px shrink-0 bg-line" aria-hidden="true"></span>

    {#if !layout.narrow}
      {@const maximised = layout.effectivePreview === "preview"}
      <button
        type="button"
        class="mng-icon-btn"
        aria-label={maximised ? "Restore split" : "Maximise preview"}
        title={maximised ? "Restore split" : "Maximise preview"}
        onclick={() => layout.toggleMaximisedPreview()}
      >
        <Icon name={maximised ? "split" : "maximise"} size={14} />
      </button>
    {/if}
    <button
      type="button"
      class="mng-icon-btn"
      aria-label="Close preview"
      title="Close preview"
      onclick={() => layout.closePreview()}
    >
      <Icon name="close" size={14} />
    </button>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="relative min-h-0 flex-1 {kind === 'pdf' && shown ? '' : 'overflow-auto'}"
    tabindex={kind === "pdf" && shown ? undefined : 0}
    role="region"
    aria-label="Rendered page"
    aria-busy={build.status === "compiling"}
  >
    {#if shown && kind === "pdf" && preview.pdf !== null}
      {#await import("./PdfViewer.svelte") then viewer}
        {#key preview.shownTarget}
          <viewer.default
            data={preview.pdf}
            view={preview.view}
            docKey={preview.shownTarget}
            onerror={(message: string) => (preview.error = message)}
            onsourceclick={(page: number, x: number, y: number) =>
              void jumpToSource(page, x, y)}
          />
        {/key}
      {:catch}
        <p class="m-0 px-6 pt-16 text-center text-xs text-text-60" role="alert">
          The PDF viewer could not be loaded. Restart Manang and try again.
        </p>
      {/await}
    {:else if shown && kind === "html"}
      <div class="flex min-h-full justify-center px-6 py-6">
        <article
          class="mng-paper-prose block self-start border border-line bg-paper px-10 py-9 text-paper-ink"
          style="width: 100%; max-width: 760px"
          aria-label="Markdown preview"
        >
          <!-- The HTML is sanitised in Rust (`markdown.render-html`): raw HTML is
               filtered to an allow-list and URLs are checked there. -->
          <!-- eslint-disable-next-line svelte/no-at-html-tags -->
          {@html build.html}
        </article>
      </div>
    {:else if isDiagram}
      <DiagramPreview />
    {:else if build.failure !== null}
      <div class="mx-auto max-w-sm px-6 pt-16 text-xs leading-relaxed text-text-60">
        <p class="m-0 text-text">{build.failure.title}</p>
        {#if build.failure.fix !== null}
          <p class="mt-2">{build.failure.fix}</p>
        {/if}
        <div class="mt-3 flex gap-2">
          {#if build.failure.code === "no_tex"}
            <button type="button" class="mng-btn" disabled title="Not available yet"
              >Download the optional built-in engine</button
            >
          {/if}
          {#if build.failure.code === "bundle_unavailable"}
            <button type="button" class="mng-btn" onclick={() => build.allowNetwork()}
              >Download TeX files</button
            >
          {/if}
          <button type="button" class="mng-btn" onclick={() => void build.compile(true)}
            >Try again</button
          >
        </div>
      </div>
    {:else if build.status === "unsupported"}
      <div class="mx-auto max-w-sm px-6 pt-16 text-xs leading-relaxed text-text-60">
        <p class="m-0 text-text">No preview for this file</p>
        <p class="mt-2">
          Open a Typst, LaTeX or Markdown document to see it rendered here.
        </p>
      </div>
    {:else if build.status === "unavailable"}
      <div class="mx-auto max-w-sm px-6 pt-16 text-xs leading-relaxed text-text-60">
        <p class="m-0 text-text">
          The {ENGINE_NAMES[build.engine ?? engine]} compiler is not available
        </p>
        <p class="mt-2">
          The kernel has no compile command for it. Check the Plugins panel, then
          restart Manang.
        </p>
      </div>
    {:else if build.status === "failed"}
      <div class="mx-auto max-w-sm px-6 pt-16 text-xs leading-relaxed text-text-60">
        <p class="m-0 text-text">{countLabel(build.counts)}</p>
        <p class="mt-2">
          The document could not be compiled, so there is nothing to show yet.
        </p>
        <button
          type="button"
          class="mng-btn mt-3"
          onclick={() => layout.showPanel("problems")}>Show problems</button
        >
      </div>
    {:else if preview.error !== ""}
      <p class="m-0 px-6 pt-16 text-center text-xs text-text-60" role="alert">
        {preview.error}
      </p>
    {:else}
      <p class="m-0 px-6 pt-16 text-center text-xs text-text-muted" role="status">
        {build.status === "compiling" ? "Compiling" : "Waiting for the first compile"}
      </p>
    {/if}

    {#if build.status === "failed" && shown}
      <button
        type="button"
        class="absolute top-2 left-1/2 z-10 flex -translate-x-1/2 items-center gap-2 border border-line bg-surface px-3 py-1.5 text-xs text-text hover:bg-surface-hi"
        onclick={() => layout.showPanel("problems")}
      >
        <Icon name="warning" size={13} />
        {banner}. Showing the last good pages. Show problems
      </button>
    {:else if build.status === "ok"}
      <span class="sr-only" role="status">Compiled in {formatMs(build.durationMs)}</span
      >
    {/if}
  </div>
</section>

<style>
  /* "Show in preview": a 1px acid-lime outline that fades. Square, flat. */
  :global(.mng-preview-mark) {
    border: var(--hairline, 1px) solid var(--mng-accent);
    animation: mng-preview-mark 1600ms ease-in forwards;
  }
  @keyframes mng-preview-mark {
    0%,
    70% {
      opacity: 1;
    }
    100% {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.mng-preview-mark) {
      animation: none;
    }
  }

  /* Typography for the Markdown page: DM Sans 400 for text, DM Mono for code,
     size and colour (never weight) carry the hierarchy. Tokens only. */
  .mng-paper-prose {
    font-family: var(--font-sans, "DM Sans", sans-serif);
    font-weight: 400;
    font-size: 14px;
    line-height: 1.65;
    overflow-wrap: anywhere;
  }
  .mng-paper-prose :global(strong),
  .mng-paper-prose :global(b) {
    font-weight: 400;
    color: var(--mng-paper-ink);
  }
  .mng-paper-prose :global(h1),
  .mng-paper-prose :global(h2),
  .mng-paper-prose :global(h3),
  .mng-paper-prose :global(h4),
  .mng-paper-prose :global(h5),
  .mng-paper-prose :global(h6) {
    font-weight: 400;
    line-height: 1.25;
    margin: 1.6em 0 0.5em;
  }
  .mng-paper-prose :global(h1) {
    font-size: 28px;
    font-weight: 300;
    margin-top: 0;
  }
  .mng-paper-prose :global(h2) {
    font-size: 20px;
    border-bottom: 1px solid var(--mng-paper-ink-60);
    padding-bottom: 0.2em;
  }
  .mng-paper-prose :global(h3) {
    font-size: 16px;
  }
  .mng-paper-prose :global(h4),
  .mng-paper-prose :global(h5),
  .mng-paper-prose :global(h6) {
    font-size: 14px;
    color: var(--mng-paper-ink-60);
  }
  .mng-paper-prose :global(p),
  .mng-paper-prose :global(ul),
  .mng-paper-prose :global(ol),
  .mng-paper-prose :global(blockquote),
  .mng-paper-prose :global(pre),
  .mng-paper-prose :global(table) {
    margin: 0 0 1em;
  }
  .mng-paper-prose :global(ul),
  .mng-paper-prose :global(ol) {
    padding-left: 1.5em;
  }
  .mng-paper-prose :global(a) {
    color: inherit;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .mng-paper-prose :global(code),
  .mng-paper-prose :global(pre),
  .mng-paper-prose :global(kbd) {
    font-family: var(--font-mono, "DM Mono", monospace);
    font-size: 12px;
  }
  .mng-paper-prose :global(pre) {
    padding: 12px 14px;
    border: 1px solid var(--mng-paper-ink-60);
    overflow-x: auto;
  }
  .mng-paper-prose :global(blockquote) {
    padding-left: 14px;
    border-left: 1px solid var(--mng-paper-ink-60);
    color: var(--mng-paper-ink-60);
  }
  .mng-paper-prose :global(table) {
    border-collapse: collapse;
  }
  .mng-paper-prose :global(th),
  .mng-paper-prose :global(td) {
    border: 1px solid var(--mng-paper-ink-60);
    padding: 4px 10px;
    text-align: left;
    font-weight: 400;
  }
  .mng-paper-prose :global(hr) {
    border: 0;
    border-top: 1px solid var(--mng-paper-ink-60);
    margin: 1.6em 0;
  }
  .mng-paper-prose :global(img) {
    max-width: 100%;
    height: auto;
  }
  /* Diagram fences: the picture centred, or a quiet note when it has an error. */
  .mng-paper-prose :global(figure.mng-diagram) {
    margin: 0 0 1em;
    text-align: center;
  }
  .mng-paper-prose :global(figure.mng-diagram img) {
    display: inline-block;
  }
  .mng-paper-prose :global(figure[data-diagram-error]) {
    padding: 10px 14px;
    border: 1px solid var(--mng-paper-ink-60);
    color: var(--mng-paper-ink-60);
    font-size: 12px;
    text-align: left;
  }
  .mng-paper-prose :global(figure[data-diagram-error] p) {
    margin: 0;
  }
  .mng-paper-prose :global(:last-child) {
    margin-bottom: 0;
  }
</style>
