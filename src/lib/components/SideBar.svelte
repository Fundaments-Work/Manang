<script lang="ts">
  import FiguresPanel from "./panels/FiguresPanel.svelte";
  import FilesPanel from "./panels/FilesPanel.svelte";
  import OutlinePanel from "./panels/OutlinePanel.svelte";
  import PluginsPanel from "./panels/PluginsPanel.svelte";
  import ReferencesPanel from "./panels/ReferencesPanel.svelte";
  import SearchPanel from "./panels/SearchPanel.svelte";
  import SettingsPanel from "./panels/SettingsPanel.svelte";
  import SourceControlPanel from "./panels/SourceControlPanel.svelte";
  import Resizer from "./ui/Resizer.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import {
    DEFAULT_LAYOUT,
    SIDE_MAX,
    SIDE_MIN,
    sidePanelVisible,
  } from "$lib/workbench/layout";

  /**
   * The resizable side panel. Under 800px it overlays the editor instead of
   * pushing it, so the editor keeps its width on a small window.
   */

  const LABELS = {
    files: "Files",
    search: "Search",
    "source-control": "Source control",
    outline: "Outline",
    figures: "Figures",
    references: "References",
    plugins: "Plugins",
    settings: "Settings",
  } as const;
</script>

{#if sidePanelVisible(layout.sideView, workspace.project !== null)}
  {@const view = layout.sideView}
  <aside
    id="side-panel"
    class="flex min-h-0 shrink-0 flex-col bg-surface {layout.narrow
      ? 'absolute top-0 bottom-0 left-12 z-20 border-r border-line'
      : ''}"
    style="width: {layout.narrow
      ? Math.min(layout.sideWidth, layout.width - 48 - 40)
      : layout.sideWidth}px"
    aria-label="{LABELS[view]} panel"
  >
    {#if view === "files"}
      <FilesPanel />
    {:else if view === "search"}
      <SearchPanel />
    {:else if view === "source-control"}
      <SourceControlPanel />
    {:else if view === "outline"}
      <OutlinePanel />
    {:else if view === "figures"}
      <FiguresPanel />
    {:else if view === "references"}
      <ReferencesPanel />
    {:else if view === "plugins"}
      <PluginsPanel />
    {:else}
      <SettingsPanel />
    {/if}
  </aside>
  {#if !layout.narrow}
    <Resizer
      orientation="vertical"
      label="Resize side panel"
      value={layout.sideWidth}
      min={SIDE_MIN}
      max={SIDE_MAX}
      onchange={(w) => layout.setSideWidth(w)}
      onreset={() => layout.setSideWidth(DEFAULT_LAYOUT.sideWidth)}
    />
  {/if}
{/if}
