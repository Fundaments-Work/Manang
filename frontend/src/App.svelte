<script lang="ts">
  import DeskHeader from "$lib/components/DeskHeader.svelte";
  import PaperPane from "$lib/components/PaperPane.svelte";
  import WritingPane from "$lib/components/WritingPane.svelte";
  import { desk } from "$lib/state/desk.svelte";

  /**
   * The Desk — DESIGN.md §3, the daily writing home.
   *
   * Shell responsibility only: compose panes, own view toggles, dispatch
   * events. No engine logic lives here.
   */
  const manuscript: readonly string[] = [
    "= Quantum Thermodynamic Bounds",
    "",
    "Under unitary coupling, the entropy",
    "production rate satisfies:",
    "",
    "$$ Delta S_irr >= integral_0^t (delta Q)/T $$",
    "",
    "This generalizes Landauer's principle",
    "to finite-time operation.",
  ];
</script>

<div class="flex h-full flex-col overflow-hidden bg-bg">
  {#if !desk.chromeHidden}
    <DeskHeader />
  {/if}

  <main class="flex min-h-0 flex-1">
    <WritingPane lines={manuscript} />
    {#if desk.pane === "split"}
      <div
        class="w-px shrink-0 bg-line"
        role="separator"
        aria-orientation="vertical"
      ></div>
      <PaperPane />
    {/if}
  </main>
</div>
