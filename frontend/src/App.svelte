<script lang="ts">
  import { onMount } from "svelte";

  import CodeMirrorEditor from "$lib/components/CodeMirrorEditor.svelte";
  import CommandPalette from "$lib/components/CommandPalette.svelte";
  import DeskHeader from "$lib/components/DeskHeader.svelte";
  import InkOverlay from "$lib/components/InkOverlay.svelte";
  import InstrumentShelf from "$lib/components/InstrumentShelf.svelte";
  import LibraryRail from "$lib/components/LibraryRail.svelte";
  import PaperPane from "$lib/components/PaperPane.svelte";
  import SlideStudio from "$lib/components/SlideStudio.svelte";
  import StatusBar from "$lib/components/StatusBar.svelte";
  import ZenMode from "$lib/components/ZenMode.svelte";

  import { desk } from "$lib/state/desk.svelte";
  import { document } from "$lib/state/document.svelte";
  import { library } from "$lib/state/library.svelte";
  import { palette } from "$lib/state/palette.svelte";
  import { deck } from "$lib/state/slides.svelte";
  import { hasMod, isPlainKey, isTextEntry } from "$lib/util/keyboard";

  /**
   * The Desk — DESIGN.md §3, the daily writing home.
   *
   * Shell responsibility only: compose panes, own view toggles, dispatch
   * events. No engine logic lives here, and the manuscript itself is owned by
   * CodeMirror rather than re-rendered by this component.
   */

  onMount(() => {
    // The Library is the shell's view of the folder, so figure references
    // resolve against it from the first keystroke.
    document.setKnownFigures(library.figurePaths);

    /**
     * Deep links. The palette owns its own store, so its route is applied here
     * rather than in `desk`, which keeps the dependency between stores
     * one-directional.
     */
    const applyHash = (): void => {
      const route = window.location.hash.replace(/^#\/?/, "").trim();
      desk.applyDeepLink(route);
      if (route.startsWith("palette")) {
        palette.showWith(route.slice("palette".length).replace(/^\//, ""));
      }
    };

    applyHash();
    window.addEventListener("hashchange", applyHash);

    return () => window.removeEventListener("hashchange", applyHash);
  });

  function onkeydown(event: KeyboardEvent): void {
    const key = event.key;

    // Command palette: the universal launcher.
    if (hasMod(event) && key.toLowerCase() === "k") {
      event.preventDefault();
      palette.toggle();
      return;
    }

    // Palette owns the keyboard while it is open.
    if (palette.open) {
      if (key === "Escape") {
        event.preventDefault();
        palette.hide();
      } else if (key === "ArrowDown") {
        event.preventDefault();
        palette.move(1);
      } else if (key === "ArrowUp") {
        event.preventDefault();
        palette.move(-1);
      } else if (key === "Enter") {
        event.preventDefault();
        palette.execute();
      }
      return;
    }

    // Presentation navigation. Arrows work while authoring the deck as well as
    // while presenting it — nobody should have to enter presentation mode just
    // to check the next slide — but never while the caret is in the manuscript,
    // where an arrow is a caret movement and must stay one.
    if (desk.slideMode || desk.presenting) {
      if (!isTextEntry(event.target)) {
        if (key === "Escape") {
          event.preventDefault();
          if (desk.presenting) desk.presenting = false;
          else desk.slideMode = false;
        } else if (key === "ArrowRight" || key === " " || key === "PageDown") {
          event.preventDefault();
          deck.next();
        } else if (key === "ArrowLeft" || key === "PageUp") {
          event.preventDefault();
          deck.previous();
        }
        return;
      }
    }

    if (hasMod(event) && key === "\\") {
      event.preventDefault();
      desk.toggleZen();
      return;
    }

    if (hasMod(event) && key.toLowerCase() === "p") {
      event.preventDefault();
      desk.libraryOpen = !desk.libraryOpen;
      return;
    }

    if (key === "Escape") {
      if (desk.overlayOpen) {
        event.preventDefault();
        desk.closeOverlays();
      } else if (desk.zen) {
        event.preventDefault();
        desk.toggleZen();
      }
      return;
    }

    if (key === "F5") {
      event.preventDefault();
      desk.slideMode = true;
      desk.presenting = true;
      return;
    }

    // Slash commands. Only from a plain keystroke, so they never shadow a
    // modifier chord or a character being typed into the manuscript.
    if (isPlainKey(event) && key === "/") {
      event.preventDefault();
      palette.show();
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="relative flex h-full flex-col overflow-hidden bg-bg">
  {#if desk.zen}
    <ZenMode />
  {:else}
    <DeskHeader />

    <main class="flex min-h-0 flex-1">
      <CodeMirrorEditor />
      {#if desk.pane === "split"}
        <div
          class="w-px shrink-0 bg-line"
          role="separator"
          aria-orientation="vertical"
        ></div>
        <PaperPane />
      {/if}
    </main>

    <StatusBar />
  {/if}

  <!-- Overlays live above the Desk and never reflow it. -->
  {#if !desk.zen}
    <LibraryRail />
    <InkOverlay />
    <SlideStudio />
    <InstrumentShelf />
  {/if}

  <CommandPalette />
</div>
