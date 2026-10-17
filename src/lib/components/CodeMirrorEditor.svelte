<script lang="ts">
  import { createEditor, syncDiagnostics } from "$lib/editor/view";
  import { document } from "$lib/state/document.svelte";

  /**
   * CodeMirror 6 mount.
   *
   * CodeMirror owns this subtree once mounted and Svelte never touches it
   * again, so typing never passes through the view framework. Diagnostics flow
   * one way: store to editor, and the store is the single source of truth.
   */

  let host: HTMLDivElement | undefined = $state();

  // Non-reactive: the view is an imperative handle, not render state.
  let view: ReturnType<typeof createEditor> | undefined;

  $effect(() => {
    if (host === undefined) return;

    view = createEditor(host, document.text, document.meta.language, {
      onChange: (text) => document.replace(text),
      onFix: (text) => document.replace(text),
    });

    return () => {
      view?.destroy();
      view = undefined;
    };
  });

  $effect(() => {
    // Reading the derived list subscribes this effect to every re-analysis.
    const diagnostics = document.diagnostics;
    if (view === undefined) return;
    syncDiagnostics(view, diagnostics);
  });
</script>

<div class="h-full min-w-0 flex-1 overflow-hidden" bind:this={host}></div>
