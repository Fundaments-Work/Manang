<script lang="ts">
  import { untrack } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";
  import { exportPdf } from "$lib/state/actions";
  import { build, formatMs } from "$lib/state/build.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { preview } from "$lib/state/preview.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { countLabel } from "$lib/workbench/problems";

  /**
   * The preview: the page the engine rendered, with a small toolbar for page,
   * zoom, refresh and export. The page is paper: it keeps the paper tokens in
   * every theme so what is on screen matches what prints.
   *
   * States: compiling (with the last page kept visible), ready, failed (last
   * good page plus a way to the errors), and the two cases where there is
   * nothing to show because the engine is missing.
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

  const widthStyle = $derived(
    preview.zoom === null
      ? "width: 100%; max-width: 920px"
      : `width: ${Math.round(640 * preview.zoom)}px`,
  );

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "PageDown") preview.go(preview.page + 1);
    else if (event.key === "PageUp") preview.go(preview.page - 1);
    else return;
    event.preventDefault();
  }

  const exportable = $derived(build.status === "ok" || build.status === "failed");
</script>

<section class="flex h-full min-h-0 min-w-0 flex-col bg-bg" aria-label="Preview">
  <div
    class="flex h-9 shrink-0 items-center gap-0.5 border-b border-line px-2"
    role="toolbar"
    aria-label="Preview controls"
  >
    <button
      type="button"
      class="mng-icon-btn"
      aria-label="Previous page"
      disabled={preview.page <= 0 || build.pages === 0}
      onclick={() => preview.go(preview.page - 1)}
    >
      <Icon name="arrowUp" size={14} />
    </button>
    <button
      type="button"
      class="mng-icon-btn"
      aria-label="Next page"
      disabled={preview.page >= build.pages - 1}
      onclick={() => preview.go(preview.page + 1)}
    >
      <Icon name="arrowDown" size={14} />
    </button>
    <span class="px-1.5 font-mono text-[11px] text-text-60" aria-live="polite">
      {#if build.pages > 0}Page {preview.page + 1} of {build.pages}{:else}No pages{/if}
    </span>

    <span class="mx-1.5 h-4 w-px bg-line" aria-hidden="true"></span>

    <button
      type="button"
      class="mng-icon-btn"
      aria-label="Zoom out"
      disabled={(preview.zoom ?? 1) <= 0.5}
      onclick={() => preview.zoomOut()}
    >
      <Icon name="clear" size={14} />
    </button>
    <button
      type="button"
      class="mng-btn h-6 min-w-12 justify-center border-transparent bg-transparent px-1.5 font-mono text-[11px] text-text-60"
      aria-label={preview.zoom === null
        ? "Zoom: fit width. Select to reset"
        : `Zoom ${Math.round(preview.zoom * 100)} percent. Select to fit width`}
      onclick={() => preview.fit()}
    >
      {preview.zoom === null ? "Fit" : `${Math.round(preview.zoom * 100)}%`}
    </button>
    <button
      type="button"
      class="mng-icon-btn"
      aria-label="Zoom in"
      disabled={(preview.zoom ?? 1) >= 2}
      onclick={() => preview.zoomIn()}
    >
      <Icon name="plus" size={14} />
    </button>

    <span class="ml-auto"></span>

    <button
      type="button"
      class="mng-icon-btn"
      aria-label="Refresh preview"
      title="Compile now"
      disabled={build.status === "unsupported" || build.status === "unavailable"}
      onclick={() => void build.compile()}
    >
      <Icon name="refresh" size={14} />
    </button>
    <button
      type="button"
      class="mng-icon-btn"
      aria-label="Export PDF"
      title="Export PDF"
      disabled={!exportable || preview.exporting}
      onclick={() => exportPdf()}
    >
      <Icon name="export" size={14} />
    </button>
  </div>

  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="relative min-h-0 flex-1 overflow-auto"
    tabindex="0"
    role="region"
    aria-label="Rendered page"
    aria-busy={build.status === "compiling"}
    {onkeydown}
  >
    {#if preview.url !== null && (build.status === "ok" || build.status === "failed" || build.status === "compiling")}
      <div class="flex min-h-full justify-center px-6 py-6">
        <img
          class="block h-auto self-start border border-line bg-paper"
          style={widthStyle}
          src={preview.url}
          alt="Page {preview.page + 1} of {build.pages}"
        />
      </div>
    {:else if build.status === "unsupported"}
      <div class="mx-auto max-w-sm px-6 pt-16 text-xs leading-relaxed text-text-60">
        <p class="m-0 text-text">No preview for this file</p>
        <p class="mt-2">
          {#if workspace.project !== null && tabs.active !== undefined && (engine === "latex" || engine === "markdown")}
            {ENGINE_NAMES[engine]} documents can be edited and saved, but there is no {ENGINE_NAMES[
              engine
            ]} compiler plugin installed yet. Typst files preview here.
          {:else}
            Open a Typst document to see it rendered here.
          {/if}
        </p>
      </div>
    {:else if build.status === "unavailable"}
      <div class="mx-auto max-w-sm px-6 pt-16 text-xs leading-relaxed text-text-60">
        <p class="m-0 text-text">The Typst compiler is not available</p>
        <p class="mt-2">
          The kernel has no <span class="font-mono text-[11px]">typst.compile</span> command.
          Check the Plugins panel, then restart Manang.
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
    {:else}
      <p class="m-0 px-6 pt-16 text-center text-xs text-text-muted" role="status">
        {build.status === "compiling" ? "Compiling" : "Waiting for the first compile"}
      </p>
    {/if}

    {#if build.status === "failed" && preview.url !== null}
      <button
        type="button"
        class="absolute top-2 left-1/2 flex -translate-x-1/2 items-center gap-2 border border-line bg-surface px-3 py-1.5 text-xs text-text hover:bg-surface-hi"
        onclick={() => layout.showPanel("problems")}
      >
        <Icon name="warning" size={13} />
        {countLabel(build.counts)}. Showing the last good page.
      </button>
    {:else if build.status === "ok"}
      <span class="sr-only" role="status">Compiled in {formatMs(build.durationMs)}</span
      >
    {/if}
  </div>
</section>
