<script lang="ts">
  import FigureToolbar from "./FigureToolbar.svelte";
  import {
    arrowHead,
    nodeBounds,
    polygonToPath,
    strokeToPath,
  } from "$lib/figure/geometry";
  import type { FigureNode } from "$lib/figure/scene";
  import { figures } from "$lib/state/figures.svelte";
  import type { FigureState, PenTool } from "$lib/state/figure.svelte";

  /**
   * The figure editor, shown inside a tab.
   *
   * Native SVG, one element per scene node. Rendering through the framework is
   * affordable because the scene is small and the pointer work is imperative:
   * strokes are sampled on pointer move and only the committed scene is
   * reactive. The scene is saved as `*.figure.json`; the Rust figure crate
   * renders the same scene to SVG headlessly.
   */
  interface Props {
    figure: FigureState;
  }

  const { figure }: Props = $props();

  let surface: SVGSVGElement | undefined = $state();

  function toScene(event: PointerEvent): { x: number; y: number } {
    const svg = surface;
    if (svg === undefined) return { x: 0, y: 0 };
    const box = svg.getBoundingClientRect();
    // The viewBox is letterboxed ("meet"), so map through the screen CTM.
    const matrix = svg.getScreenCTM();
    if (matrix === null)
      return { x: event.clientX - box.left, y: event.clientY - box.top };
    const point = new DOMPoint(event.clientX, event.clientY).matrixTransform(
      matrix.inverse(),
    );
    return { x: point.x, y: point.y };
  }

  /** Mice report 0.5 for every event; only a pen has true pressure. */
  function pressureOf(event: PointerEvent): number {
    return event.pointerType === "pen" ? event.pressure : 0.5;
  }

  function onpointerdown(event: PointerEvent): void {
    surface?.focus();
    surface?.setPointerCapture(event.pointerId);
    figure.beginStroke(toScene(event), pressureOf(event));
  }

  function onpointermove(event: PointerEvent): void {
    if (!figure.isDrawing) return;
    figure.extend(toScene(event), pressureOf(event));
  }

  function onpointerup(event: PointerEvent): void {
    if (!figure.isDrawing) return;
    surface?.releasePointerCapture(event.pointerId);
    figure.endStroke();
  }

  const TOOL_KEYS: Record<string, PenTool> = {
    p: "select",
    b: "freehand",
    g: "geometry",
    a: "arrow",
    m: "math",
  };

  function onkeydown(event: KeyboardEvent): void {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      figure.deleteSelected();
      return;
    }
    const next = TOOL_KEYS[event.key.toLowerCase()];
    if (next !== undefined) {
      event.preventDefault();
      figure.setTool(next);
    }
  }
</script>

<div class="flex h-full min-h-0 w-full min-w-0 flex-1 flex-col bg-bg">
  <FigureToolbar {figure} onsave={() => void figures.save(figure.path)} />

  <div class="relative min-h-0 flex-1 overflow-hidden">
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions, a11y_no_noninteractive_tabindex -->
    <svg
      bind:this={surface}
      role="application"
      tabindex="0"
      aria-label="Figure canvas. Drag to draw. Tool keys: P select, B freehand, G rectangle, A arrow, M formula. Delete removes the selection."
      viewBox="0 0 {figure.scene.width} {figure.scene.height}"
      width="100%"
      height="100%"
      preserveAspectRatio="xMidYMid meet"
      class="block touch-none p-6 select-none"
      style="cursor: {figure.tool === 'select' ? 'default' : 'crosshair'}"
      {onpointerdown}
      {onpointermove}
      {onpointerup}
      onpointercancel={onpointerup}
      {onkeydown}
    >
      <!-- The frame the figure is drawn on: radius 0, one hairline. -->
      <rect
        x="0.5"
        y="0.5"
        width={figure.scene.width - 1}
        height={figure.scene.height - 1}
        fill="none"
        stroke="var(--mng-line)"
        stroke-width="1"
      />

      {#each figure.scene.nodes as node (node.id)}
        {#if node.kind === "stroke"}
          <path d={strokeToPath(node)} fill={node.color} />
        {:else if node.kind === "rect"}
          <rect
            x={node.x}
            y={node.y}
            width={node.width}
            height={node.height}
            fill={node.fill ?? "none"}
            stroke={node.stroke}
            stroke-width="1"
          />
        {:else if node.kind === "ellipse"}
          <ellipse
            cx={node.cx}
            cy={node.cy}
            rx={node.rx}
            ry={node.ry}
            fill={node.fill ?? "none"}
            stroke={node.stroke}
            stroke-width="1"
          />
        {:else if node.kind === "arrow"}
          <line
            x1={node.from.x}
            y1={node.from.y}
            x2={node.to.x}
            y2={node.to.y}
            stroke={node.color}
            stroke-width="1"
          />
          <path d={polygonToPath(arrowHead(node.from, node.to))} fill={node.color} />
          {#if node.label !== undefined}
            <text
              x={(node.from.x + node.to.x) / 2}
              y={(node.from.y + node.to.y) / 2 - 8}
              fill="var(--mng-text-60)"
              font-family="var(--font-mono)"
              font-size="13"
              text-anchor="middle"
            >
              {node.label}
            </text>
          {/if}
        {:else if node.kind === "math"}
          <text
            x={node.x}
            y={node.y}
            fill={node.color}
            font-family="var(--font-mono)"
            font-size={node.size}>{node.tex}</text
          >
        {/if}

        <!-- Selection is a hairline frame, never a glow. -->
        {#if figure.selectedId === node.id}
          {@const bounds = nodeBounds(node as FigureNode)}
          <rect
            x={bounds.x - 3}
            y={bounds.y - 3}
            width={bounds.width + 6}
            height={bounds.height + 6}
            fill="none"
            stroke="var(--mng-accent)"
            stroke-width="1"
            stroke-dasharray="3 3"
          />
        {/if}
      {/each}

      {#if figure.draft.length > 1}
        <path
          d={strokeToPath({
            kind: "stroke",
            id: "draft",
            points: figure.draft,
            size: 2.5,
            color: "var(--mng-text)",
          })}
          fill="var(--mng-text)"
        />
      {/if}

      {#if figure.dragStart !== null && figure.dragCurrent !== null}
        <line
          x1={figure.dragStart.x}
          y1={figure.dragStart.y}
          x2={figure.dragCurrent.x}
          y2={figure.dragCurrent.y}
          stroke="var(--mng-accent)"
          stroke-width="1"
          stroke-dasharray="4 4"
        />
      {/if}

      {#if figure.activeSnap !== null}
        <rect
          x={figure.activeSnap.x - 2}
          y={figure.activeSnap.y - 2}
          width="4"
          height="4"
          fill="var(--mng-accent)"
        />
      {/if}
    </svg>

    <p
      class="pointer-events-none absolute bottom-2 left-4 m-0 font-mono text-[10px] text-text-muted"
    >
      {figure.nodeCount}
      {figure.nodeCount === 1 ? "node" : "nodes"} in {figure.path}
    </p>
  </div>
</div>
