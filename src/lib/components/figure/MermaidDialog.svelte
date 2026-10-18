<script lang="ts">
  import type { FigureState } from "$lib/state/figure.svelte";

  /**
   * Insert Mermaid: paste or type Mermaid text and insert it as editable
   * shapes (flowchart, state, class and sequence diagrams) or as a block that
   * keeps its source. The plugin does the conversion (`figure.from-mermaid`).
   */
  interface Props {
    figure: FigureState;
  }

  const { figure }: Props = $props();

  let source = $state(
    "flowchart LR\n  A[Idea] --> B{Works?}\n  B -->|yes| C[Ship]\n  B -->|no| A\n",
  );
  let busy = $state(false);
  let field: HTMLTextAreaElement | undefined = $state();

  $effect(() => {
    field?.focus();
  });

  async function insert(mode: "shapes" | "block"): Promise<void> {
    if (source.trim() === "" || busy) return;
    busy = true;
    const ok = await figure.insertMermaid(source, mode);
    busy = false;
    if (ok) figure.mermaidDialog = false;
  }

  function onkeydown(event: KeyboardEvent): void {
    event.stopPropagation();
    if (event.key === "Escape") {
      event.preventDefault();
      figure.mermaidDialog = false;
    } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      void insert("shapes");
    }
  }
</script>

<div
  class="absolute inset-0 z-20 flex items-start justify-center bg-bg/60 pt-16"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) figure.mermaidDialog = false;
  }}
>
  <div
    class="flex w-[28rem] max-w-[calc(100%-2rem)] flex-col gap-2 border border-line bg-surface p-3 text-[12px] text-text"
    role="dialog"
    aria-modal="true"
    aria-label="Insert Mermaid"
    tabindex="-1"
    {onkeydown}
  >
    <p class="m-0 text-text-60">
      Mermaid text. Flowcharts, state, class and sequence diagrams become editable
      shapes; anything else is inserted as a block.
    </p>
    <textarea
      bind:this={field}
      bind:value={source}
      class="mng-field selectable h-44 resize-y py-1.5 font-mono text-[12px]"
      aria-label="Mermaid source"
      spellcheck="false"
    ></textarea>
    <div class="flex items-center gap-1.5">
      <span class="mng-kbd">Ctrl+Enter</span>
      <span class="ml-auto"></span>
      <button
        type="button"
        class="mng-btn h-7 px-2"
        onclick={() => (figure.mermaidDialog = false)}
      >
        Cancel
      </button>
      <button
        type="button"
        class="mng-btn h-7 px-2"
        disabled={busy}
        onclick={() => void insert("block")}>Insert as block</button
      >
      <button
        type="button"
        class="mng-btn h-7 px-2"
        disabled={busy}
        onclick={() => void insert("shapes")}>Insert as shapes</button
      >
    </div>
  </div>
</div>
