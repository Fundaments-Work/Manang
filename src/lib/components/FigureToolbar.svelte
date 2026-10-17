<script lang="ts">
  import Icon, { type IconName } from "$lib/components/ui/Icon.svelte";
  import { notice } from "$lib/state/notice.svelte";
  import { TOOLS, type FigureState, type PenTool } from "$lib/state/figure.svelte";

  /**
   * Figure tools: a toolbar above the canvas. The active tool is shown by
   * state (`aria-pressed`, a tonal step and a heavier border), never by colour
   * alone. Single-letter keys select tools while the canvas has focus.
   */
  interface Props {
    figure: FigureState;
    onsave: () => void;
  }

  const { figure, onsave }: Props = $props();

  const ICONS: Record<PenTool, IconName> = {
    select: "select",
    freehand: "freehand",
    geometry: "rect",
    ellipse: "ellipse",
    arrow: "arrow",
    math: "formula",
  };

  /** The selected formula, if any: its text is edited in the toolbar. */
  const formula = $derived(
    figure.selected?.kind === "math" ? figure.selected : undefined,
  );

  function commitFormula(event: Event): void {
    const input = event.currentTarget as HTMLInputElement;
    if (formula !== undefined) figure.setMathText(formula.id, input.value);
  }

  async function copyEmbed(): Promise<void> {
    const line = figure.embedSnippet();
    try {
      await navigator.clipboard.writeText(line);
      notice.show("Copied the embed line. Paste it into a document.");
    } catch {
      notice.error(`Could not reach the clipboard. Type this line instead: ${line}`);
    }
  }
</script>

<div
  class="flex h-9 shrink-0 items-center gap-0.5 border-b border-line px-2"
  role="toolbar"
  aria-label="Figure tools"
>
  {#each TOOLS as { tool, label, key } (tool)}
    <button
      type="button"
      class="mng-btn h-7 border-transparent bg-transparent px-2 text-text-60 hover:text-text aria-pressed:border-text-60 aria-pressed:text-text"
      aria-pressed={figure.tool === tool}
      aria-keyshortcuts={key}
      title="{label} ({key})"
      onclick={() => figure.setTool(tool)}
    >
      <Icon name={ICONS[tool]} size={14} />
      {label}
    </button>
  {/each}

  <span class="mx-1.5 h-4 w-px bg-line" aria-hidden="true"></span>

  <button
    type="button"
    class="mng-btn h-7 border-transparent bg-transparent px-2 text-text-60 hover:text-text"
    disabled={!figure.canUndo}
    title="Undo (Ctrl+Z)"
    aria-keyshortcuts="Control+Z"
    onclick={() => figure.undo()}
  >
    <Icon name="undo" size={14} /> Undo
  </button>
  <button
    type="button"
    class="mng-btn h-7 border-transparent bg-transparent px-2 text-text-60 hover:text-text"
    disabled={!figure.canRedo}
    title="Redo (Ctrl+Shift+Z)"
    aria-keyshortcuts="Control+Shift+Z"
    onclick={() => figure.redo()}
  >
    <Icon name="redo" size={14} /> Redo
  </button>
  <button
    type="button"
    class="mng-btn h-7 border-transparent bg-transparent px-2 text-text-60 hover:text-text"
    disabled={figure.nodeCount === 0}
    onclick={() => figure.clear()}
  >
    <Icon name="trash" size={14} /> Clear
  </button>

  {#if formula !== undefined}
    <span class="mx-1.5 h-4 w-px bg-line" aria-hidden="true"></span>
    <label class="flex items-center gap-1.5 text-text-60">
      Formula
      <input
        class="mng-field selectable h-7 w-48 font-mono text-[12px]"
        value={formula.tex}
        aria-label="Formula text"
        onchange={commitFormula}
        onkeydown={(event) => {
          if (event.key === "Enter") (event.currentTarget as HTMLInputElement).blur();
        }}
      />
    </label>
  {/if}

  <span class="ml-auto"></span>

  <button
    type="button"
    class="mng-btn h-7 px-2"
    onclick={() => void copyEmbed()}
    title="Copy the line that embeds this figure in a Typst document"
  >
    <Icon name="copy" size={13} /> Copy embed line
  </button>
  <button
    type="button"
    class="mng-btn h-7 px-2"
    disabled={!figure.dirty}
    onclick={onsave}
  >
    <Icon name="save" size={13} /> Save
    <span class="mng-kbd">Ctrl+S</span>
  </button>
</div>
