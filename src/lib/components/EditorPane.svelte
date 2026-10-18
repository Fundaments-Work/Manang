<script lang="ts">
  import { EditorState } from "@codemirror/state";
  import { untrack } from "svelte";

  import { pushDiagnostics } from "$lib/editor/gutter";
  import { createEditorView } from "$lib/editor/view";
  import { build } from "$lib/state/build.svelte";
  import { diagram } from "$lib/state/diagram.svelte";
  import { documents } from "$lib/state/documents.svelte";
  import { heal } from "$lib/state/heal.svelte";
  import { problemsIn, toGutter } from "$lib/workbench/problems";

  /**
   * The text editor for the active document.
   *
   * One CodeMirror view serves every open document; this component owns it and
   * swaps in the document's own state when `path` changes. CodeMirror owns the
   * DOM beneath it, so typing never passes through Svelte. Problem markers flow
   * one way: the build's problems are mapped onto the shown text and pushed in.
   */
  interface Props {
    path: string;
    /** Zen mode: a quiet reading column with no gutter. */
    zen?: boolean;
  }

  const { path, zen = false }: Props = $props();

  let host: HTMLDivElement | undefined = $state();
  let view = $state.raw<ReturnType<typeof createEditorView> | undefined>();

  $effect(() => {
    if (host === undefined) return;
    const created = createEditorView(host, EditorState.create({ doc: "" }));
    documents.attach(created);
    view = created;
    return () => {
      documents.detach();
      created.destroy();
      view = undefined;
    };
  });

  // Swap in the right document. `version` re-runs this once a session exists.
  $effect(() => {
    void documents.version;
    const session = documents.get(path);
    if (view === undefined || session === undefined) return;
    untrack(() => {
      documents.show(session);
      documents.applyReveal();
    });
  });

  $effect(() => {
    const problems = [...build.problems, ...diagram.problems, ...heal.problems];
    void heal.results;
    void documents.version;
    if (view === undefined) return;
    const session = documents.get(path);
    if (session === undefined || documents.shown !== session) return;
    pushDiagnostics(
      view,
      toGutter(
        untrack(() => session.text()),
        problemsIn(problems, path),
        (problem) => heal.choicesFor(problem),
      ),
    );
  });

  let lastFocus = untrack(() => documents.focusRequest);
  $effect(() => {
    const request = documents.focusRequest;
    if (request !== lastFocus) {
      lastFocus = request;
      view?.focus();
    }
  });

  $effect(() => {
    if (documents.revealRequest !== null) untrack(() => documents.applyReveal());
  });
</script>

<div
  bind:this={host}
  class="h-full min-h-0 min-w-0 overflow-hidden bg-bg"
  class:zen-editor={zen}
></div>
