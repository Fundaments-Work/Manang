<script lang="ts">
  import Icon, { type IconName } from "$lib/components/ui/Icon.svelte";
  import { TOOLS, type Tool, type ToolSpec } from "$lib/figure/keymap";
  import { figures } from "$lib/state/figures.svelte";
  import { figureUi } from "$lib/state/figureUi.svelte";
  import { notice } from "$lib/state/notice.svelte";
  import type { FigureState } from "$lib/state/figure.svelte";

  /**
   * The figure studio's one toolbar row, in groups: select and hand (with the
   * eraser and laser), shapes, lines and drawing, text and formulas and
   * Mermaid, history, zoom, then the panels (narrow windows) and the file
   * actions at the right. The active tool is shown by state (`aria-pressed`, a
   * tonal step), never by colour alone; every button names its shortcut in its
   * tooltip. One tab stop: Tab enters on the active tool, the arrow keys move
   * along the row.
   */
  interface Props {
    figure: FigureState;
    onsave: () => void;
    /** The studio is too narrow for side panels: they open as overlays from here. */
    narrow?: boolean;
    /** There is room for text labels on the file actions. */
    roomy?: boolean;
  }

  const { figure, onsave, narrow = false, roomy = false }: Props = $props();

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

  const spec = (tool: Tool): ToolSpec => {
    const found = TOOLS.find((t) => t.tool === tool);
    if (found === undefined) throw new Error(`unknown tool ${tool}`);
    return found;
  };

  const GROUPS: readonly { readonly name: string; readonly tools: readonly Tool[] }[] =
    [
      { name: "Pointer tools", tools: ["select", "hand", "eraser", "laser"] },
      { name: "Shapes", tools: ["rect", "diamond", "ellipse", "frame"] },
      { name: "Lines and drawing", tools: ["arrow", "line", "draw"] },
    ];

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

  let bar: HTMLDivElement | undefined = $state();

  /** Arrow keys, Home and End move along the row. */
  function onkeydown(event: KeyboardEvent): void {
    const keys = ["ArrowRight", "ArrowLeft", "Home", "End"];
    if (!keys.includes(event.key) || bar === undefined) return;
    const items = [
      ...bar.querySelectorAll<HTMLButtonElement>("button:not([disabled])"),
    ];
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    if (at === -1) return;
    event.preventDefault();
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? items.length - 1
          : (at + (event.key === "ArrowRight" ? 1 : -1) + items.length) % items.length;
    items[next]?.focus();
  }

  /** The one button Tab lands on: the active tool. */
  const stop = (tool: Tool): 0 | -1 => (figure.tool === tool ? 0 : -1);

  const SIZE = $derived(narrow ? "h-6 w-6" : "h-7 w-7");
</script>

{#snippet tool(t: Tool)}
  {@const s = spec(t)}
  <button
    type="button"
    class="mng-icon-btn {SIZE}"
    aria-label={s.label}
    aria-pressed={figure.tool === t}
    aria-keyshortcuts={s.key}
    title="{s.label} ({s.key})"
    tabindex={stop(t)}
    onclick={() => figure.setTool(t)}
  >
    <Icon name={ICONS[t]} size={14} />
  </button>
{/snippet}

{#snippet divider()}
  <span class="mx-1 h-4 w-px shrink-0 bg-line" aria-hidden="true"></span>
{/snippet}

<div
  bind:this={bar}
  class="flex h-9 shrink-0 items-center gap-0.5 border-b border-line px-2"
  role="toolbar"
  aria-label="Figure tools"
  tabindex="-1"
  {onkeydown}
>
  {#if narrow}
    <button
      type="button"
      class="mng-icon-btn {SIZE}"
      aria-label="Layers"
      aria-pressed={figureUi.layersOverlay}
      title="Layers"
      tabindex="-1"
      onclick={() => {
        figureUi.layersOverlay = !figureUi.layersOverlay;
        if (figureUi.layersOverlay) figureUi.inspectorOverlay = false;
      }}
    >
      <Icon name="layers" size={14} />
    </button>
    {@render divider()}
  {/if}

  {#each GROUPS as group, i (group.name)}
    {#if i > 0}{@render divider()}{/if}
    <div class="flex gap-0.5" role="group" aria-label={group.name}>
      {#each group.tools as t (t)}{@render tool(t)}{/each}
    </div>
  {/each}

  {@render divider()}
  <div class="flex gap-0.5" role="group" aria-label="Text, formulas and diagrams">
    {@render tool("text")}
    {@render tool("math")}
    <button
      type="button"
      class="mng-icon-btn {SIZE}"
      aria-label="Insert Mermaid"
      aria-haspopup="dialog"
      title="Insert a Mermaid diagram as editable shapes or as a block"
      tabindex="-1"
      onclick={() => (figure.mermaidDialog = true)}
    >
      <Icon name="mermaid" size={14} />
    </button>
  </div>

  {@render divider()}
  <div class="flex gap-0.5" role="group" aria-label="History">
    <button
      type="button"
      class="mng-icon-btn {SIZE}"
      aria-label="Undo"
      disabled={!figure.canUndo}
      title="Undo (Ctrl+Z)"
      aria-keyshortcuts="Control+Z"
      tabindex="-1"
      onclick={() => void figure.undo()}
    >
      <Icon name="undo" size={14} />
    </button>
    <button
      type="button"
      class="mng-icon-btn {SIZE}"
      aria-label="Redo"
      disabled={!figure.canRedo}
      title="Redo (Ctrl+Shift+Z)"
      aria-keyshortcuts="Control+Shift+Z"
      tabindex="-1"
      onclick={() => void figure.redo()}
    >
      <Icon name="redo" size={14} />
    </button>
  </div>

  {@render divider()}
  <div class="flex items-center gap-0.5" role="group" aria-label="Zoom">
    {#if !narrow}
      <button
        type="button"
        class="mng-icon-btn {SIZE}"
        aria-label="Zoom out"
        title="Zoom out (Ctrl+-)"
        aria-keyshortcuts="Control+-"
        tabindex="-1"
        onclick={() => figure.zoomBy(-1)}
      >
        <Icon name="zoom-out" size={14} />
      </button>
    {/if}
    <button
      type="button"
      class="mng-lime-hover h-7 shrink-0 bg-transparent px-1 text-center font-mono text-[11px] text-text-60 {narrow
        ? 'w-10'
        : 'w-12'}"
      data-zoom
      title="Zoom to 100% (Ctrl+0)"
      aria-label="Zoom {figure.zoomPercent}%, reset to 100%"
      tabindex="-1"
      onclick={() => figure.zoomReset()}>{figure.zoomPercent}%</button
    >
    {#if !narrow}
      <button
        type="button"
        class="mng-icon-btn {SIZE}"
        aria-label="Zoom in"
        title="Zoom in (Ctrl+=)"
        aria-keyshortcuts="Control+="
        tabindex="-1"
        onclick={() => figure.zoomBy(1)}
      >
        <Icon name="zoom-in" size={14} />
      </button>
    {/if}
    <button
      type="button"
      class="mng-icon-btn {SIZE}"
      aria-label="Zoom to fit"
      title="Zoom to fit (Shift+1)"
      aria-keyshortcuts="Shift+1"
      tabindex="-1"
      onclick={() => figure.zoomToFit()}
    >
      <Icon name="fit" size={14} />
    </button>
  </div>

  <span class="ml-auto"></span>

  <button
    type="button"
    class={roomy ? "mng-btn h-7 shrink-0 px-2" : `mng-icon-btn ${SIZE}`}
    aria-label="Copy embed line"
    onclick={() => void copyEmbed()}
    title="Copy the line that embeds this figure's SVG in a document (saves first if needed)"
    tabindex="-1"
  >
    <Icon name="copy" size={13} />{#if roomy}Copy embed line{/if}
  </button>
  <button
    type="button"
    class={roomy ? "mng-btn ml-1 h-7 shrink-0 px-2" : `mng-icon-btn ${SIZE}`}
    aria-label="Save"
    aria-keyshortcuts="Control+S"
    title="Save (Ctrl+S)"
    disabled={!figure.dirty}
    tabindex="-1"
    onclick={onsave}
  >
    <Icon name="save" size={13} />{#if roomy}Save <span class="mng-kbd">Ctrl+S</span
      >{/if}
  </button>
  {#if narrow}
    {@render divider()}
    <button
      type="button"
      class="mng-icon-btn {SIZE}"
      aria-label="Inspector"
      aria-pressed={figureUi.inspectorOverlay}
      title="Inspector"
      tabindex="-1"
      onclick={() => {
        figureUi.inspectorOverlay = !figureUi.inspectorOverlay;
        if (figureUi.inspectorOverlay) figureUi.layersOverlay = false;
      }}
    >
      <Icon name="sidebar" size={14} />
    </button>
  {/if}
</div>
