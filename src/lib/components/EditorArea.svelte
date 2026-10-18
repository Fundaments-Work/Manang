<script lang="ts">
  import { untrack, type Component } from "svelte";

  import EditorPane from "./EditorPane.svelte";
  import FileViewer from "./FileViewer.svelte";
  import Resizer from "./ui/Resizer.svelte";
  import { formatShortcut } from "$lib/commands/shortcut";
  import { reportError } from "$lib/kernel";
  import { newFileHere } from "$lib/state/actions";
  import { figures } from "$lib/state/figures.svelte";
  import type { FigureState } from "$lib/state/figure.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { palette } from "$lib/state/palette.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { detectPlatform } from "$lib/util/keyboard";
  import { DEFAULT_LAYOUT, SPLIT_MAX, SPLIT_MIN } from "$lib/workbench/layout";
  import { basename } from "$lib/workbench/paths";

  /**
   * The editor area: whatever the active tab holds. A document shows its editor
   * and, beside it, the preview. A figure shows the figure editor and a slides
   * view shows the deck. The heavy pieces (preview, figure editor, slides) are
   * separate chunks, fetched the first time a tab needs them.
   */

  const platform = detectPlatform();
  const tab = $derived(tabs.active);
  const mode = $derived(layout.zen ? "editor" : layout.effectivePreview);

  let areaWidth = $state(800);

  let Preview = $state.raw<Component | undefined>();
  let Figure = $state.raw<Component<{ figure: FigureState }> | undefined>();
  let Slides = $state.raw<Component<{ path: string }> | undefined>();

  /**
   * Why a heavy view could not load (its chunk failed to import, or the figure
   * could not be read). Shown with a Retry; before, a failed import left the
   * tab on "Opening figure" for good.
   */
  let loadError = $state("");
  let attempt = $state(0);

  function lazy<T>(load: () => Promise<{ default: T }>, set: (c: T) => void): void {
    load()
      .then((m) => set(m.default))
      .catch((error: unknown) => {
        reportError("a view could not be loaded", error);
        loadError = "This view could not be loaded.";
      });
  }

  $effect(() => {
    void attempt;
    if (
      tab?.kind === "document" &&
      tab.view === "text" &&
      mode !== "editor" &&
      Preview === undefined
    ) {
      lazy(
        () => import("./PreviewPane.svelte"),
        (c) => (Preview = c),
      );
    }
    if (tab?.kind === "figure" && Figure === undefined) {
      lazy(
        () => import("./FigureEditor.svelte"),
        (c) => (Figure = c),
      );
    }
    if (tab?.kind === "document" && tab.view === "slides" && Slides === undefined) {
      lazy(
        () => import("./SlidesView.svelte"),
        (c) => (Slides = c),
      );
    }
  });

  const figure = $derived(tab?.kind === "figure" ? figures.get(tab.path) : undefined);

  // A figure tab whose scene is not loaded (restored, or its state was lost)
  // loads it instead of waiting for someone else to.
  $effect(() => {
    void attempt;
    if (tab?.kind !== "figure" || figure !== undefined) return;
    const path = tab.path;
    void untrack(() => figures.open(path)).then((loaded) => {
      if (loaded === null && tabs.active?.path === path)
        loadError = `${basename(path)} could not be opened.`;
    });
  });

  function retry(): void {
    loadError = "";
    attempt += 1;
  }
  const labelledBy = $derived(
    tab === undefined
      ? undefined
      : `tab-${tabs.tabs.findIndex((t) => t.id === tab.id)}`,
  );
</script>

<div
  id="editor-area"
  bind:clientWidth={areaWidth}
  role="tabpanel"
  aria-labelledby={labelledBy}
  aria-label={tab === undefined ? "Editor" : undefined}
  class="flex min-h-0 min-w-0 flex-1"
>
  {#if tab === undefined}
    <div
      class="mx-auto flex h-full w-full max-w-[420px] flex-col justify-center px-8 text-xs leading-relaxed text-text-60"
    >
      <p class="m-0 text-[13px] text-text">No file is open</p>
      <p class="mt-2 mb-4">Pick a file from Files, or start a new one.</p>
      <div class="flex flex-wrap gap-2">
        <button type="button" class="mng-btn" onclick={() => palette.show("files")}>
          Go to file <span class="mng-kbd"
            >{formatShortcut({ key: "p", mod: true }, platform)}</span
          >
        </button>
        <button type="button" class="mng-btn" onclick={() => void newFileHere()}>
          New file <span class="mng-kbd"
            >{formatShortcut({ key: "n", mod: true }, platform)}</span
          >
        </button>
      </div>
    </div>
  {:else if tab.kind === "figure"}
    {#if figure !== undefined && Figure !== undefined}
      <Figure {figure} />
    {:else if loadError !== ""}
      <div class="m-auto flex flex-col items-center gap-3 text-xs" role="alert">
        <p class="m-0 text-text-60">{loadError}</p>
        <button type="button" class="mng-btn" onclick={retry}>Retry</button>
      </div>
    {:else}
      <p class="m-auto text-xs text-text-muted" role="status">Opening figure</p>
    {/if}
  {:else if tab.kind === "viewer"}
    <FileViewer path={tab.path} />
  {:else if tab.view === "slides"}
    {#if Slides !== undefined}
      <Slides path={tab.path} />
    {:else}
      <p class="m-auto text-xs text-text-muted" role="status">Opening slides</p>
    {/if}
  {:else}
    {#if mode !== "preview"}
      <div
        class="min-h-0 min-w-0"
        style="flex: {mode === 'split' ? layout.splitRatio : 1} 1 0"
      >
        <EditorPane path={tab.path} zen={layout.zen} />
      </div>
    {/if}
    {#if mode === "split"}
      <Resizer
        orientation="vertical"
        label="Resize editor and preview"
        value={layout.splitRatio}
        min={SPLIT_MIN}
        max={SPLIT_MAX}
        scale={1 / Math.max(1, areaWidth)}
        step={0.03}
        onchange={(ratio) => layout.setSplitRatio(ratio)}
        onreset={() => layout.setSplitRatio(DEFAULT_LAYOUT.splitRatio)}
      />
    {/if}
    {#if mode !== "editor"}
      <div
        class="min-h-0 min-w-0"
        style="flex: {mode === 'split' ? 1 - layout.splitRatio : 1} 1 0"
      >
        {#if Preview !== undefined}
          <Preview />
        {:else}
          <p class="m-auto p-6 text-xs text-text-muted" role="status">
            Opening preview
          </p>
        {/if}
      </div>
    {/if}
  {/if}
</div>
