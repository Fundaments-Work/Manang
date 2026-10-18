<script lang="ts">
  import Icon, { type IconName } from "$lib/components/ui/Icon.svelte";
  import { TOOLS, type Tool } from "$lib/figure/keymap";
  import { figures } from "$lib/state/figures.svelte";
  import { notice } from "$lib/state/notice.svelte";
  import type { FigureState } from "$lib/state/figure.svelte";

  /**
   * The figure studio's one toolbar: tools, history, zoom, Insert Mermaid,
   * then the file actions. The active tool is shown by state (`aria-pressed`,
   * a tonal step and a heavier border), never by colour alone. Tool keys work
   * while the canvas has focus and are listed in the shortcut overlay.
   */
  interface Props {
    figure: FigureState;
    onsave: () => void;
  }

  const { figure, onsave }: Props = $props();

  const ICONS: Record<Tool, IconName> = {
    select: "select",
    hand: "hand",
    rect: "rect",
    diamond: "diamond",
    ellipse: "ellipse",
    arrow: "arrow",
    line: "line",
    draw: "freehand",
    text: "text",
    frame: "frame",
    eraser: "eraser",
    laser: "laser",
    math: "formula",
  };

  /** Saves first when needed, so the line always names an SVG that exists. */
  async function copyEmbed(): Promise<void> {
    const line = await figures.embed(figure.path);
    if (line === null) return;
    try {
      await navigator.clipboard.writeText(line);
      notice.show("Copied the embed line. Paste it into a document.");
    } catch {
      notice.error(`Could not reach the clipboard. Type this line instead: ${line}`);
    }
  }

  const BUTTON =
    "mng-btn h-7 w-7 justify-center border-transparent bg-transparent p-0 text-text-60 hover:text-text aria-pressed:border-text-60 aria-pressed:text-text";
</script>

<div
  class="flex h-9 shrink-0 items-center gap-0.5 overflow-x-auto border-b border-line px-2"
  role="toolbar"
  aria-label="Figure tools"
>
  {#each TOOLS as { tool, label, key } (tool)}
    <button
      type="button"
      class={BUTTON}
      aria-label={label}
      aria-pressed={figure.tool === tool}
      aria-keyshortcuts={key}
      title="{label} ({key})"
      onclick={() => figure.setTool(tool)}
    >
      <Icon name={ICONS[tool]} size={14} />
    </button>
  {/each}

  <span class="mx-1.5 h-4 w-px shrink-0 bg-line" aria-hidden="true"></span>

  <button
    type="button"
    class={BUTTON}
    aria-label="Undo"
    disabled={!figure.canUndo}
    title="Undo (Ctrl+Z)"
    aria-keyshortcuts="Control+Z"
    onclick={() => void figure.undo()}
  >
    <Icon name="undo" size={14} />
  </button>
  <button
    type="button"
    class={BUTTON}
    aria-label="Redo"
    disabled={!figure.canRedo}
    title="Redo (Ctrl+Shift+Z)"
    aria-keyshortcuts="Control+Shift+Z"
    onclick={() => void figure.redo()}
  >
    <Icon name="redo" size={14} />
  </button>

  <span class="mx-1.5 h-4 w-px shrink-0 bg-line" aria-hidden="true"></span>

  <button
    type="button"
    class={BUTTON}
    aria-label="Zoom out"
    title="Zoom out (Ctrl+-)"
    aria-keyshortcuts="Control+-"
    onclick={() => figure.zoomBy(-1)}
  >
    <Icon name="zoom-out" size={14} />
  </button>
  <button
    type="button"
    class="mng-btn h-7 w-12 shrink-0 justify-center border-transparent bg-transparent px-0 font-mono text-[11px] text-text-60 hover:text-text"
    data-zoom
    title="Zoom to 100% (Ctrl+0)"
    aria-label="Zoom {figure.zoomPercent}%, reset to 100%"
    onclick={() => figure.zoomReset()}>{figure.zoomPercent}%</button
  >
  <button
    type="button"
    class={BUTTON}
    aria-label="Zoom in"
    title="Zoom in (Ctrl+=)"
    aria-keyshortcuts="Control+="
    onclick={() => figure.zoomBy(1)}
  >
    <Icon name="zoom-in" size={14} />
  </button>
  <button
    type="button"
    class={BUTTON}
    aria-label="Zoom to fit"
    title="Zoom to fit (Shift+1)"
    aria-keyshortcuts="Shift+1"
    onclick={() => figure.zoomToFit()}
  >
    <Icon name="fit" size={14} />
  </button>

  <span class="mx-1.5 h-4 w-px shrink-0 bg-line" aria-hidden="true"></span>

  <button
    type="button"
    class="mng-btn h-7 shrink-0 border-transparent bg-transparent px-2 text-text-60 hover:text-text"
    title="Insert a Mermaid diagram as editable shapes or as a block"
    aria-haspopup="dialog"
    onclick={() => (figure.mermaidDialog = true)}
  >
    <Icon name="mermaid" size={14} /> Insert Mermaid
  </button>

  <span class="ml-auto"></span>

  <button
    type="button"
    class="mng-btn h-7 shrink-0 px-2"
    onclick={() => void copyEmbed()}
    title="Copy the line that embeds this figure's SVG in a document (saves first if needed)"
  >
    <Icon name="copy" size={13} /> Copy embed line
  </button>
  <button
    type="button"
    class="mng-btn ml-1 h-7 shrink-0 px-2"
    disabled={!figure.dirty}
    onclick={onsave}
  >
    <Icon name="save" size={13} /> Save
    <span class="mng-kbd">Ctrl+S</span>
  </button>
</div>
