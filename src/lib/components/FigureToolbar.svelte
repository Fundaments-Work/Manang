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
    arrow: "arrow",
    math: "formula",
  };

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
    disabled={figure.nodeCount === 0}
    onclick={() => figure.clear()}
  >
    <Icon name="trash" size={14} /> Clear
  </button>

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
