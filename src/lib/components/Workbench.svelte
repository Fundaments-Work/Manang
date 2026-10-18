<script lang="ts">
  import { onMount, untrack } from "svelte";

  import ActivityBar from "./ActivityBar.svelte";
  import BottomPanel from "./BottomPanel.svelte";
  import CommandPalette from "./CommandPalette.svelte";
  import EditorArea from "./EditorArea.svelte";
  import Presentation from "./Presentation.svelte";
  import SideBar from "./SideBar.svelte";
  import StatusBar from "./StatusBar.svelte";
  import TabStrip from "./TabStrip.svelte";
  import TopBar from "./TopBar.svelte";
  import Home from "./Home.svelte";
  import ProjectSwitcher from "./ProjectSwitcher.svelte";
  import ConfirmDialog from "./ui/ConfirmDialog.svelte";
  import Resizer from "./ui/Resizer.svelte";
  import ShortcutOverlay from "./ShortcutOverlay.svelte";
  import SettingsPage from "./settings/SettingsPage.svelte";
  import { installShellSurfaces } from "./menu/surfaces";
  import { resolveKey, typesText } from "$lib/commands/keymap";
  import { keymap } from "$lib/commands/keymap.svelte";
  import type { Command } from "$lib/commands/registry";
  import { workbenchCommands } from "$lib/commands/workbench";
  import { build } from "$lib/state/build.svelte";
  import { dialog } from "$lib/state/dialog.svelte";
  import { home } from "$lib/state/home.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { notice } from "$lib/state/notice.svelte";
  import { context } from "$lib/state/context.svelte";
  import { palette } from "$lib/state/palette.svelte";
  import { settings } from "$lib/state/settings.svelte";
  import { settingsPage } from "$lib/state/settingsPage.svelte";
  import { slides } from "$lib/state/slides.svelte";
  import { switcher } from "$lib/state/switcher.svelte";
  import { captureThumbnail } from "$lib/state/thumbnails";
  import { tabs } from "$lib/state/tabs.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import {
    DEFAULT_LAYOUT,
    PANEL_MAX,
    PANEL_MIN,
    sidePanelVisible,
  } from "$lib/workbench/layout";

  /**
   * The workbench: top bar, activity bar, side panel, tabs and editor area,
   * bottom panel and status bar. This component composes them and owns the
   * global keyboard map.
   */

  // Re-compile when the active file changes, since the preview target may too.
  $effect(() => {
    void tabs.active?.id;
    untrack(() => build.schedule("switch"));
  });

  // After a good compile of the main document, keep the project's Home
  // thumbnail fresh (at most once a minute per project).
  $effect(() => {
    const generation = build.generation;
    untrack(() => {
      const project = workspace.project;
      if (
        generation > 0 &&
        project !== null &&
        project.engine === "typst" &&
        build.status === "ok" &&
        build.target === project.main
      ) {
        void captureThumbnail(project.root, Date.now(), (root) =>
          home.noteThumbnail(root),
        );
      }
    });
  });

  // Project settings layer follows the open project.
  $effect(() => {
    const root = workspace.project?.root ?? null;
    untrack(() => void settings.load(root));
  });

  const sideOverlay = $derived(
    layout.narrow &&
      !home.showing &&
      sidePanelVisible(layout.sideView, workspace.project !== null),
  );

  onMount(() => {
    keymap.start();
    installShellSurfaces();
  });

  /** Readable names for a conflict notice: the commands' titles. */
  function titles(commands: readonly Command[]): string {
    return commands.map((c) => c.title).join(" and ");
  }

  /**
   * The global keyboard map. Every shortcut comes from the keybindings
   * registry (defaults plus the person's changes, see `keymap`), resolved
   * against the command list, so what the palette and menus show is what
   * runs. Escape stays here: it dismisses whatever is on top (zen, Settings,
   * Home over a project, the floating side panel), which is not one command.
   */
  function onkeydown(event: KeyboardEvent): void {
    if (
      event.defaultPrevented ||
      dialog.request !== null ||
      palette.open ||
      switcher.open ||
      slides.presenting
    )
      return;

    if (event.key === "Escape") {
      if (layout.zen) {
        event.preventDefault();
        layout.toggleZen();
      } else if (settingsPage.open) {
        event.preventDefault();
        settingsPage.hide();
      } else if (home.visible && workspace.project !== null) {
        event.preventDefault();
        home.hide();
      } else if (sideOverlay) {
        event.preventDefault();
        layout.closeSide();
      }
      return;
    }

    // A bare key typed into a field is text, whatever it is bound to.
    const target = event.target as HTMLElement | null;
    if (
      typesText(event) &&
      target?.closest("input, textarea, select, [contenteditable]") != null
    )
      return;

    const commands = workbenchCommands((mode) => palette.show(mode));
    const hit = resolveKey(event, keymap.bindings, commands, (when) =>
      context.when(when),
    );
    if (hit === null) return;
    event.preventDefault();
    if (hit.kind === "run") hit.command.run();
    else if (hit.kind === "conflict")
      notice.error(
        `This shortcut is bound to both ${titles(hit.commands)}. Change one in Settings > Keyboard shortcuts.`,
      );
  }
</script>

<svelte:window {onkeydown} bind:innerWidth={layout.width} />

<div class="relative flex h-full flex-col overflow-hidden bg-bg">
  {#if !layout.zen}
    <TopBar />
  {/if}

  <div class="relative flex min-h-0 flex-1">
    {#if !layout.zen && !home.showing}
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

    <main class="relative flex min-h-0 min-w-0 flex-1 flex-col">
      {#if workspace.project !== null}
        <!-- Stays mounted under Home so the editor keeps its caret and scroll. -->
        <div
          class="flex min-h-0 flex-1 flex-col"
          inert={home.visible || settingsPage.open}
        >
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
        </div>
      {/if}
      {#if home.showing}
        <div class="absolute inset-0 z-10 bg-bg" inert={settingsPage.open}>
          <Home />
        </div>
      {/if}
      {#if settingsPage.open && !layout.zen}
        <div class="absolute inset-0 z-20 bg-bg">
          <SettingsPage />
        </div>
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
  <ProjectSwitcher />
  <ConfirmDialog />
  <ShortcutOverlay />
  {#if slides.presenting}
    <Presentation />
  {/if}
</div>
