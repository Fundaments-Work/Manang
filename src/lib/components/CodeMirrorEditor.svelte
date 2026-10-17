<script lang="ts">
  import { createEditor, syncDiagnostics } from "$lib/editor/view";
  import { untrack } from "svelte";

  import { kernelSyncExtension } from "$lib/editor/kernelSync";
  import { document } from "$lib/state/document.svelte";
  import { session } from "$lib/state/session.svelte";

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

    // The Rust editor plugin owns the document. User edits go to it as
    // minimal commands; its own changes come back as tagged transactions.
    // `untrack`: the initial text and language seed the view once. Reading them
    // reactively here would rebuild the editor (and the kernel document) on
    // every keystroke.
    const seed = untrack(() => ({
      text: document.text,
      language: document.meta.language,
    }));
    const created = createEditor(
      host,
      seed.text,
      seed.language,
      {
        onChange: (text) => {
          document.replace(text);
          session.scheduleLint();
        },
        onFix: (text) => document.replace(text),
      },
      kernelSyncExtension(session.sync),
    );
    view = created;
    const detach = untrack(() => session.attach(created));

    return () => {
      detach();
      view?.destroy();
      view = undefined;
    };
  });

  $effect(() => {
    // Reading the derived list subscribes this effect to every re-analysis.
    const local = document.diagnostics;
    const remote = session.lint;
    if (view === undefined) return;
    syncDiagnostics(view, [...local, ...remote]);
  });
</script>

<div class="h-full min-w-0 flex-1 overflow-hidden" bind:this={host}></div>
