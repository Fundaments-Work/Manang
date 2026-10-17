<script lang="ts">
  import type { Component } from "svelte";

  import EditorPane from "./EditorPane.svelte";
  import Resizer from "./ui/Resizer.svelte";
  import { formatShortcut } from "$lib/commands/shortcut";
  import { newFileHere } from "$lib/state/actions";
  import { figures } from "$lib/state/figures.svelte";
  import type { FigureState } from "$lib/state/figure.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { palette } from "$lib/state/palette.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { detectPlatform } from "$lib/util/keyboard";
  import { DEFAULT_LAYOUT, SPLIT_MAX, SPLIT_MIN } from "$lib/workbench/layout";

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

  $effect(() => {
    if (
      tab?.kind === "document" &&
      tab.view === "text" &&
      mode !== "editor" &&
      Preview === undefined
    ) {
      void import("./PreviewPane.svelte").then((m) => (Preview = m.default));
    }
    if (tab?.kind === "figure" && Figure === undefined) {
      void import("./FigureEditor.svelte").then((m) => (Figure = m.default));
    }
    if (tab?.kind === "document" && tab.view === "slides" && Slides === undefined) {
      void import("./SlidesView.svelte").then((m) => (Slides = m.default));
    }
  });

  const figure = $derived(tab?.kind === "figure" ? figures.get(tab.path) : undefined);
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
    {:else}
      <p class="m-auto text-xs text-text-muted" role="status">Opening figure</p>
    {/if}
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
