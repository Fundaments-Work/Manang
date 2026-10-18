<script lang="ts">
  import { tick } from "svelte";

  import Icon, { type IconName } from "$lib/components/ui/Icon.svelte";
  import {
    filterLayers,
    isRenamable,
    kindName,
    layerName,
    layerOrder,
    type DropSide,
  } from "$lib/figure/inspect";
  import type { FigureNode } from "$lib/figure/scene";
  import type { FigureState } from "$lib/state/figure.svelte";
  import { figureUi } from "$lib/state/figureUi.svelte";

  /**
   * The layers panel, docked to the left of the canvas: every node of the
   * figure, topmost first, with its type icon, name, group mark, and lock and
   * visibility toggles. Click selects (Shift: a range, Ctrl or Cmd: toggle) and
   * stays in step with the canvas selection. Double-click or F2 renames (a
   * node's label is its name); drag a row to change the stacking order, or
   * press Alt with the arrow keys. It collapses to a strip, or is an overlay in
   * a narrow window.
   */
  interface Props {
    figure: FigureState;
    /** A narrow window: shown over the canvas when opened from the toolbar. */
    overlay?: boolean;
  }

  const { figure, overlay = false }: Props = $props();

  const ICONS: Readonly<Record<string, IconName>> = {
    rect: "rect",
    diamond: "diamond",
    ellipse: "ellipse",
    text: "text",
    frame: "frame",
    image: "preview",
    mermaid: "mermaid",
    arrow: "arrow",
    line: "line",
    freedraw: "freehand",
    math: "formula",
  };

  const open = $derived(overlay ? figureUi.layersOverlay : figureUi.layersOpen);
  let query = $state("");
  let searching = $state(false);
  let search: HTMLInputElement | undefined = $state();
  let list: HTMLDivElement | undefined = $state();

  const all = $derived(layerOrder(figure.display));
  const rows = $derived(filterLayers(all, query));
  const selected = $derived(new Set(figure.selection));

  /** The row being renamed, and its text. */
  let renaming = $state<string | null>(null);
  let draft = $state("");
  /** Drag and drop: the row dragged, and where it would land. */
  let dragging = $state<string | null>(null);
  let drop = $state<{ id: string; side: DropSide } | null>(null);

  function toggle(): void {
    if (overlay) figureUi.layersOverlay = false;
    else figureUi.setLayersOpen(!figureUi.layersOpen);
  }

  async function startSearch(): Promise<void> {
    searching = true;
    await tick();
    search?.focus();
  }

  function stopSearch(): void {
    searching = false;
    query = "";
  }

  function click(event: MouseEvent, node: FigureNode): void {
    figure.selectLayer(
      node.id,
      event.shiftKey ? "range" : event.ctrlKey || event.metaKey ? "toggle" : "replace",
    );
  }

  async function startRename(node: FigureNode): Promise<void> {
    if (!isRenamable(node) || node.locked === true) return;
    figure.selectLayer(node.id, "replace");
    draft =
      layerName(node) === `${kindName(node)} ${node.id}` ? "" : (labelOf(node) ?? "");
    renaming = node.id;
    await tick();
    const input = list?.querySelector<HTMLInputElement>("input[data-rename]");
    input?.focus();
    input?.select();
  }

  function labelOf(node: FigureNode): string | undefined {
    return node.kind === "math" || node.kind === "freedraw" ? undefined : node.label;
  }

  function finishRename(commit: boolean): void {
    const id = renaming;
    renaming = null;
    if (commit && id !== null) figure.setLabel(id, draft.trim());
    list?.querySelector<HTMLElement>(`[data-layer="${id}"]`)?.focus();
  }

  function rowKey(event: KeyboardEvent, node: FigureNode, index: number): void {
    if (renaming !== null) return;
    const go = (to: number): void => {
      const next = rows[Math.min(Math.max(to, 0), rows.length - 1)];
      if (next === undefined) return;
      figure.selectLayer(next.id, event.shiftKey ? "range" : "replace");
      list?.querySelector<HTMLElement>(`[data-layer="${next.id}"]`)?.focus();
    };
    switch (event.key) {
      case "ArrowDown":
      case "ArrowUp":
        event.preventDefault();
        if (event.altKey) {
          if (!selected.has(node.id)) figure.selectLayer(node.id, "replace");
          // The list runs topmost first: Up brings the layer forward.
          figure.reorder(event.key === "ArrowUp" ? "forward" : "backward");
        } else {
          go(index + (event.key === "ArrowDown" ? 1 : -1));
        }
        break;
      case "Home":
        event.preventDefault();
        go(0);
        break;
      case "End":
        event.preventDefault();
        go(rows.length - 1);
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        figure.selectLayer(
          node.id,
          event.shiftKey
            ? "range"
            : event.ctrlKey || event.metaKey
              ? "toggle"
              : "replace",
        );
        break;
      case "F2":
        event.preventDefault();
        void startRename(node);
        break;
      case "Escape":
        if (overlay) {
          event.stopPropagation();
          figureUi.layersOverlay = false;
        }
        break;
      default:
    }
  }

  function dragOver(event: DragEvent, node: FigureNode): void {
    if (dragging === null) return;
    event.preventDefault();
    if (event.dataTransfer !== null) event.dataTransfer.dropEffect = "move";
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    drop = {
      id: node.id,
      side: event.clientY < box.top + box.height / 2 ? "above" : "below",
    };
  }

  function dropped(event: DragEvent): void {
    event.preventDefault();
    const d = drop;
    const id = dragging;
    dragging = null;
    drop = null;
    if (d !== null && id !== null) figure.moveLayer(id, d.id, d.side);
  }

  // A layer picked on the canvas scrolls into view in the list.
  $effect(() => {
    const [first] = figure.selection;
    if (first === undefined || list === undefined) return;
    list
      .querySelector<HTMLElement>(`[data-layer="${first}"]`)
      ?.scrollIntoView({ block: "nearest" });
  });

  // Where the Tab key lands: the first selected row, else the first.
  const tabStop = $derived(
    rows.find((n) => selected.has(n.id))?.id ?? rows[0]?.id ?? null,
  );
</script>

{#if open || !overlay}
  <aside
    class="flex min-h-0 shrink-0 flex-col border-r border-line bg-surface text-[11px] text-text-60 {open
      ? 'w-52'
      : 'w-8'} {overlay ? 'absolute inset-y-0 left-0 z-20' : ''}"
    aria-label="Layers"
    data-layers
    data-open={open}
  >
    <div class="flex h-8 shrink-0 items-center gap-0.5 border-b border-line pr-1 pl-2">
      {#if open}
        {#if searching}
          <input
            bind:this={search}
            bind:value={query}
            class="mng-field selectable h-6 min-w-0 flex-1 px-1.5 text-[11px]"
            aria-label="Filter layers"
            placeholder="Filter layers"
            spellcheck="false"
            onkeydown={(e) => {
              e.stopPropagation();
              if (e.key === "Escape") stopSearch();
            }}
          />
        {:else}
          <h2 class="m-0 min-w-0 flex-1 truncate text-[12px] font-normal text-text">
            Layers
            <span class="font-mono text-[11px] text-text-muted">{all.length}</span>
          </h2>
        {/if}
        <button
          type="button"
          class="mng-icon-btn h-6 w-6"
          aria-label={searching ? "Close filter" : "Filter layers"}
          aria-pressed={searching}
          title={searching ? "Close filter" : "Filter layers"}
          onclick={() => (searching ? stopSearch() : void startSearch())}
        >
          <Icon name={searching ? "close" : "search"} size={13} />
        </button>
      {/if}
      <button
        type="button"
        class="mng-icon-btn h-6 w-6 {open ? '' : '-ml-1'}"
        aria-label={open ? "Collapse layers" : "Expand layers"}
        aria-expanded={open}
        title={open ? "Collapse layers" : "Layers"}
        data-layers-toggle
        onclick={toggle}
      >
        <Icon name={open ? "chevron-right" : "layers"} size={14} />
      </button>
    </div>

    {#if open}
      <div
        bind:this={list}
        class="min-h-0 flex-1 overflow-y-auto"
        role="listbox"
        aria-label="Layers"
        aria-multiselectable="true"
        tabindex="-1"
      >
        {#each rows as node, index (node.id)}
          {@const isSel = selected.has(node.id)}
          {@const hidden = node.hidden === true}
          {@const group = (node.groups ?? [])[(node.groups ?? []).length - 1]}
          <div
            class="mng-layer relative flex h-7 items-center border-b border-transparent pr-0.5 pl-1.5 {isSel
              ? 'bg-surface-hi text-text'
              : 'text-text-60'} {hidden ? 'opacity-60' : ''}"
            role="option"
            aria-selected={isSel}
            aria-label="{layerName(node)}, {kindName(node)}{node.locked === true
              ? ', locked'
              : ''}{hidden ? ', hidden' : ''}"
            tabindex={node.id === tabStop ? 0 : -1}
            data-layer={node.id}
            data-selected={isSel ? "" : undefined}
            draggable={renaming === null}
            onclick={(e) => click(e, node)}
            ondblclick={() => void startRename(node)}
            onkeydown={(e) => rowKey(e, node, index)}
            ondragstart={(e) => {
              dragging = node.id;
              e.dataTransfer?.setData("text/plain", node.id);
              if (e.dataTransfer !== null) e.dataTransfer.effectAllowed = "move";
            }}
            ondragover={(e) => dragOver(e, node)}
            ondrop={dropped}
            ondragend={() => {
              dragging = null;
              drop = null;
            }}
          >
            {#if isSel}
              <span class="absolute inset-y-0 left-0 w-px bg-accent" aria-hidden="true"
              ></span>
            {/if}
            {#if drop?.id === node.id}
              <span
                class="pointer-events-none absolute inset-x-0 h-px bg-accent {drop.side ===
                'above'
                  ? 'top-0'
                  : 'bottom-0'}"
                aria-hidden="true"
                data-drop={drop.side}
              ></span>
            {/if}
            <span class="mr-1.5 flex w-4 shrink-0 justify-center" aria-hidden="true">
              <Icon name={ICONS[node.kind] ?? "rect"} size={13} />
            </span>
            {#if renaming === node.id}
              <input
                data-rename
                class="mng-field selectable h-5 min-w-0 flex-1 px-1 text-[11px]"
                aria-label="Layer name"
                placeholder={kindName(node)}
                bind:value={draft}
                spellcheck="false"
                onclick={(e) => e.stopPropagation()}
                ondblclick={(e) => e.stopPropagation()}
                onblur={() => renaming !== null && finishRename(true)}
                onkeydown={(e) => {
                  e.stopPropagation();
                  if (e.key === "Enter") {
                    e.preventDefault();
                    finishRename(true);
                  } else if (e.key === "Escape") {
                    e.preventDefault();
                    finishRename(false);
                  }
                }}
              />
            {:else}
              <span class="min-w-0 flex-1 truncate text-[11px]" data-layer-name>
                {layerName(node)}
              </span>
              {#if group !== undefined}
                <span
                  class="mr-1 font-mono text-[10px] text-text-muted"
                  title="Group {group}">{group}</span
                >
              {/if}
            {/if}
            <button
              type="button"
              class="mng-icon-btn h-6 w-6 {node.locked === true
                ? 'text-text'
                : 'text-text-muted'}"
              tabindex="-1"
              aria-label="{node.locked === true ? 'Unlock' : 'Lock'} {layerName(node)}"
              aria-pressed={node.locked === true}
              title={node.locked === true ? "Unlock" : "Lock"}
              data-layer-lock
              onclick={(e) => {
                e.stopPropagation();
                figure.setLocked([node.id], node.locked !== true);
              }}
            >
              <Icon name={node.locked === true ? "lock" : "unlock"} size={12} />
            </button>
            <button
              type="button"
              class="mng-icon-btn h-6 w-6 {hidden ? 'text-text' : 'text-text-muted'}"
              tabindex="-1"
              aria-label="{hidden ? 'Show' : 'Hide'} {layerName(node)}"
              aria-pressed={hidden}
              title={hidden ? "Show" : "Hide"}
              data-layer-visibility
              onclick={(e) => {
                e.stopPropagation();
                figure.setHidden([node.id], !hidden);
              }}
            >
              <Icon name={hidden ? "eye-off" : "eye"} size={12} />
            </button>
          </div>
        {:else}
          <p class="m-0 p-3 text-[11px] text-text-muted">
            {all.length === 0
              ? "Nothing on this figure yet. Pick a tool and draw."
              : "No layer matches."}
          </p>
        {/each}
        <!-- The drop target below the last row: send to the back. -->
        {#if dragging !== null && rows.length > 0}
          <div
            class="h-4"
            role="presentation"
            ondragover={(e) => {
              e.preventDefault();
              const last = rows[rows.length - 1];
              if (last !== undefined) drop = { id: last.id, side: "below" };
            }}
            ondrop={dropped}
          ></div>
        {/if}
      </div>
    {/if}
  </aside>
{/if}
