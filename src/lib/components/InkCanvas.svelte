<script lang="ts">
  import PenTray from "./PenTray.svelte";
  import {
    arrowHead,
    nodeBounds,
    polygonToPath,
    strokeToPath,
  } from "$lib/ink/geometry";
  import { ink, type PenTool } from "$lib/state/ink.svelte";
  import type { FigureNode } from "$lib/ink/scene";

  /**
   * The vector ink canvas — DESIGN.md §4.
   *
   * Native SVG, one element per scene node. Rendering through the framework is
   * affordable here precisely because the scene is small and the pointer work
   * is imperative: strokes are sampled on pointer move, and only the committed
   * scene is reactive. The manuscript, by contrast, never re-renders at all.
   */
  interface Props {
    scale?: number;
  }

  const { scale = 1 }: Props = $props();

  let surface: SVGSVGElement | undefined = $state();

  /** Pointer position in scene coordinates. */
  function toScene(event: PointerEvent): { x: number; y: number } {
    const svg = surface;
    if (svg === undefined) return { x: 0, y: 0 };
    const box = svg.getBoundingClientRect();
    return {
      x: (event.clientX - box.left) / scale,
      y: (event.clientY - box.top) / scale,
    };
  }

  /**
   * Pointer pressure.
   *
   * Mice report 0.5 for every event, and true pressure only exists on a tablet.
   * `perfect-freehand` compensates for the flat signal by simulating pressure
   * from velocity, so a constant value still yields a tapered stroke.
   */
  function pressureOf(event: PointerEvent): number {
    return event.pointerType === "pen" ? event.pressure : 0.5;
  }

  function onpointerdown(event: PointerEvent): void {
    if (ink.tool === "select") return;
    surface?.setPointerCapture(event.pointerId);
    ink.beginStroke(toScene(event), pressureOf(event));
  }

  function onpointermove(event: PointerEvent): void {
    if (!ink.isDrawing) return;
    const point = toScene(event);
    ink.extend(point, pressureOf(event));
  }

  function onpointerup(event: PointerEvent): void {
    if (!ink.isDrawing) return;
    surface?.releasePointerCapture(event.pointerId);
    ink.endStroke();
  }

  function onkeydown(event: KeyboardEvent): void {
    // Ignore shortcuts aimed at a text field, so the library filter and the
    // palette keep their own Delete and letter keys while the canvas is open.
    const target = event.target;
    if (
      target instanceof HTMLElement &&
      (target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable)
    ) {
      return;
    }

    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      ink.deleteSelected();
      return;
    }

    const tools: Record<string, PenTool> = {
      p: "select",
      b: "freehand",
      g: "geometry",
      a: "arrow",
      m: "math",
    };
    const next = tools[event.key];
    if (next !== undefined) {
      event.preventDefault();
      ink.setTool(next);
    }
  }

  function isSelected(node: FigureNode): boolean {
    return ink.selectedId === node.id;
  }
</script>

<!-- The canvas is modal, so its shortcuts are window-scoped: this component
     only exists while the overlay is open. -->
<svelte:window {onkeydown} />

<div class="relative flex-1 overflow-hidden bg-bg">
  <svg
    bind:this={surface}
    role="application"
    aria-label="Figure canvas — drag to draw; arrow endpoints snap to node anchors"
    viewBox="0 0 {ink.scene.width} {ink.scene.height}"
    width="100%"
    height="100%"
    preserveAspectRatio="xMidYMid meet"
    class="block touch-none select-none"
    style="cursor: {ink.tool === 'select' ? 'default' : 'crosshair'}"
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    onpointercancel={onpointerup}
  >
    <!-- The frame the figure is drawn on. Zero radius, 1px hairline. -->
    <rect
      x="0.5"
      y="0.5"
      width={ink.scene.width - 1}
      height={ink.scene.height - 1}
      fill="none"
      stroke="var(--mng-line)"
      stroke-width="1"
    />

    {#each ink.scene.nodes as node (node.id)}
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
          font-size={node.size}
        >
          {node.tex}
        </text>
      {/if}

      <!-- Selection is a hairline frame, never a glow. -->
      {#if isSelected(node)}
        {@const bounds = nodeBounds(node)}
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

    <!-- The stroke in progress. -->
    {#if ink.draft.length > 1}
      <path
        d={strokeToPath({
          kind: "stroke",
          id: "draft",
          points: ink.draft,
          size: 2.5,
          color: "var(--mng-text)",
        })}
        fill="var(--mng-text)"
      />
    {/if}

    <!-- The gesture in progress: dashed, so it reads as provisional. -->
    {#if ink.dragStart !== null && ink.dragCurrent !== null}
      <line
        x1={ink.dragStart.x}
        y1={ink.dragStart.y}
        x2={ink.dragCurrent.x}
        y2={ink.dragCurrent.y}
        stroke="var(--mng-accent)"
        stroke-width="1"
        stroke-dasharray="4 4"
      />
    {/if}

    <!-- Snap indicator: a 4px square on the anchor that caught the magnet. -->
    {#if ink.activeSnap !== null}
      <rect
        x={ink.activeSnap.x - 2}
        y={ink.activeSnap.y - 2}
        width="4"
        height="4"
        fill="var(--mng-accent)"
      />
    {/if}
  </svg>

  <PenTray />

  <p
    class="pointer-events-none absolute bottom-3 left-4 font-mono text-[10px] text-text-muted"
  >
    {ink.nodeCount} nodes · figures/state_lattice.figure.json
  </p>
</div>
