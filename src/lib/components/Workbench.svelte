<script lang="ts">
  import { untrack } from "svelte";

  import ActivityBar from "./ActivityBar.svelte";
  import BottomPanel from "./BottomPanel.svelte";
  import CommandPalette from "./CommandPalette.svelte";
  import EditorArea from "./EditorArea.svelte";
  import Presentation from "./Presentation.svelte";
  import SideBar from "./SideBar.svelte";
  import StatusBar from "./StatusBar.svelte";
  import TabStrip from "./TabStrip.svelte";
  import TopBar from "./TopBar.svelte";
  import Welcome from "./Welcome.svelte";
  import ConfirmDialog from "./ui/ConfirmDialog.svelte";
  import Resizer from "./ui/Resizer.svelte";
  import { matches } from "$lib/commands/shortcut";
  import {
    PALETTE_COMMANDS_SHORTCUTS,
    PALETTE_FILES_SHORTCUT,
    shortcutTable,
    workbenchCommands,
  } from "$lib/commands/workbench";
  import { toggleSlides } from "$lib/state/actions";
  import { build } from "$lib/state/build.svelte";
  import { dialog } from "$lib/state/dialog.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { palette } from "$lib/state/palette.svelte";
  import { slides } from "$lib/state/slides.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { engineOf } from "$lib/workbench/paths";
  import {
    DEFAULT_LAYOUT,
    PANEL_MAX,
    PANEL_MIN,
    sidePanelVisible,
  } from "$lib/workbench/layout";

  /**
   * The workbench: top bar, activity bar, side panel, tabs and editor area,
   * bottom panel and status bar. This component composes them and owns the
   * global keyboard map; every shortcut comes from the command list, so what the
   * palette shows is what runs.
   */

  // Re-compile when the active file changes, since the preview target may too.
  $effect(() => {
    void tabs.active?.id;
    untrack(() => build.schedule());
  });

  const sideOverlay = $derived(
    layout.narrow && sidePanelVisible(layout.sideView, workspace.project !== null),
  );

  function onkeydown(event: KeyboardEvent): void {
    if (
      event.defaultPrevented ||
      dialog.request !== null ||
      palette.open ||
      slides.presenting
    )
      return;
    const mod = event.ctrlKey || event.metaKey;

    if (event.key === "Escape") {
      if (layout.zen) {
        event.preventDefault();
        layout.toggleZen();
      } else if (sideOverlay) {
        event.preventDefault();
        layout.closeSide();
      }
      return;
    }

    if (
      event.key === "F5" &&
      tabs.active !== undefined &&
      engineOf(tabs.active.path) === "typst"
    ) {
      event.preventDefault();
      if (tabs.active.view !== "slides") toggleSlides();
      slides.present();
      return;
    }

    if (!mod) return;

    if (PALETTE_COMMANDS_SHORTCUTS.some((s) => matches(event, s))) {
      event.preventDefault();
      palette.show("commands");
      return;
    }
    if (workspace.project !== null && matches(event, PALETTE_FILES_SHORTCUT)) {
      event.preventDefault();
      palette.show("files");
      return;
    }
    if (!event.shiftKey && !event.altKey && /^[1-9]$/.test(event.key)) {
      event.preventDefault();
      tabs.byShortcut(Number(event.key));
      return;
    }

    const hit = shortcutTable(workbenchCommands((mode) => palette.show(mode))).find(
      (entry) => matches(event, entry.shortcut),
    );
    if (hit !== undefined) {
      event.preventDefault();
      hit.run();
    }
  }
</script>

<svelte:window {onkeydown} bind:innerWidth={layout.width} />

<div class="relative flex h-full flex-col overflow-hidden bg-bg">
  {#if !layout.zen}
    <TopBar />
  {/if}

  <div class="relative flex min-h-0 flex-1">
    {#if !layout.zen}
      <ActivityBar />
      <SideBar />
      {#if sideOverlay}
        <!-- Under 800px the side panel floats over the editor; this catches the click that dismisses it. -->
        <button
          type="button"
          tabindex="-1"
          class="absolute inset-y-0 right-0 left-12 z-10 cursor-default"
          aria-label="Close side panel"
          onclick={() => layout.closeSide()}
        ></button>
      {/if}
    {/if}

    <main class="flex min-h-0 min-w-0 flex-1 flex-col">
      {#if workspace.project === null}
        <Welcome />
      {:else}
        {#if !layout.zen}
          <TabStrip />
        {/if}
        <EditorArea />
        {#if !layout.zen && layout.panelOpen}
          <Resizer
            orientation="horizontal"
            label="Resize bottom panel"
            value={layout.panelHeight}
            min={PANEL_MIN}
            max={PANEL_MAX}
            reverse
            onchange={(h) => layout.setPanelHeight(h)}
            onreset={() => layout.setPanelHeight(DEFAULT_LAYOUT.panelHeight)}
          />
          <BottomPanel />
        {/if}
      {/if}
    </main>
  </div>

  {#if !layout.zen}
    <StatusBar />
  {:else}
    <p
      class="pointer-events-none absolute bottom-3 left-1/2 m-0 -translate-x-1/2 text-[11px] text-text-muted"
      style="animation: mng-hint 3.2s ease-out forwards"
      role="status"
    >
      Press Esc to leave zen mode
    </p>
  {/if}

  <CommandPalette />
  <ConfirmDialog />
  {#if slides.presenting}
    <Presentation />
  {/if}
</div>
