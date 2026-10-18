<script lang="ts">
  import FigureToolbar from "./FigureToolbar.svelte";
  import { freedrawPath, nodeBounds, normalizeRect } from "$lib/figure/geometry";
  import { nodeVisual } from "$lib/figure/render";
  import { figures } from "$lib/state/figures.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import type { FigureState, PenTool } from "$lib/state/figure.svelte";

  import { FORMULA_FIELD } from "./FigureToolbar.svelte";

  /**
   * The figure editor, shown inside a tab.
   *
   * Native SVG, one element per scene node. Rendering through the framework is
   * affordable because the scene is small and the pointer work is imperative:
   * strokes are sampled on pointer move and only the committed scene is
   * reactive. The figure plugin owns the scene: edits, hit tests, undo and
   * saving (`*.figure.json` plus the SVG documents embed) go through it.
   */
  interface Props {
    figure: FigureState;
  }

  const { figure }: Props = $props();

  /** Figure tokens drawn with the app theme's values (files use `print`). */
  const theme = $derived(layout.theme);

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

  /** Double-click (or Enter) on a selected formula edits its text in the toolbar. */
  function editFormula(): void {
    if (figure.selected?.kind !== "math") return;
    const field = document.getElementById(FORMULA_FIELD);
    if (field instanceof HTMLInputElement) {
      field.focus();
      field.select();
    }
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
    e: "ellipse",
    a: "arrow",
    m: "math",
  };

  /** Arrow keys move the selection: 1 unit, or 10 with Shift. */
  const NUDGE: Record<string, { x: number; y: number }> = {
    ArrowLeft: { x: -1, y: 0 },
    ArrowRight: { x: 1, y: 0 },
    ArrowUp: { x: 0, y: -1 },
    ArrowDown: { x: 0, y: 1 },
  };

  function onkeydown(event: KeyboardEvent): void {
    const mod = event.ctrlKey || event.metaKey;
    if (mod && !event.altKey && event.key.toLowerCase() === "z") {
      event.preventDefault();
      void (event.shiftKey ? figure.redo() : figure.undo());
      return;
    }
    if (mod && !event.altKey && event.key.toLowerCase() === "y") {
      event.preventDefault();
      void figure.redo();
      return;
    }
    if (mod || event.altKey) return;
    const step = NUDGE[event.key];
    if (step !== undefined && figure.selectedId !== null) {
      event.preventDefault();
      const factor = event.shiftKey ? 10 : 1;
      figure.nudge({ x: step.x * factor, y: step.y * factor });
      return;
    }
    if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      figure.deleteSelected();
      return;
    }
    if (event.key === "Enter" && figure.selected?.kind === "math") {
      event.preventDefault();
      editFormula();
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
      aria-label="Figure canvas. Drag to draw. Tool keys: P select, B freehand, G rectangle, E ellipse, A arrow, M formula. Drag a selected shape or use the arrow keys to move it. Delete removes the selection. Double-click or Enter edits a formula. Ctrl+Z undoes."
      viewBox="0 0 {figure.scene.width} {figure.scene.height}"
      width="100%"
      height="100%"
      preserveAspectRatio="xMidYMid meet"
      class="block touch-none p-6 select-none"
      style="cursor: {figure.tool === 'select'
        ? figure.selectedId !== null
          ? 'move'
          : 'default'
        : 'crosshair'}"
      {onpointerdown}
      {onpointermove}
      {onpointerup}
      onpointercancel={onpointerup}
      {onkeydown}
      ondblclick={editFormula}
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
        {@const visual = nodeVisual(figure.scene, node, theme)}
        {#if visual !== undefined}
          <g
            data-node={node.id}
            transform={visual.rotate === undefined
              ? undefined
              : `rotate(${visual.rotate.angle} ${visual.rotate.cx} ${visual.rotate.cy})`}
            opacity={visual.opacity}
          >
            {#each visual.items as item, i (i)}
              {#if item.type === "path"}
                <path
                  d={item.d}
                  fill={item.fill}
                  fill-opacity={item.fillOpacity}
                  stroke={item.stroke}
                  stroke-width={item.strokeWidth}
                  stroke-linecap={item.linecap}
                  stroke-linejoin={item.linejoin}
                  stroke-dasharray={item.dash}
                />
              {:else if item.type === "rect"}
                <rect
                  x={item.x}
                  y={item.y}
                  width={item.width}
                  height={item.height}
                  fill={item.fill}
                  fill-opacity={item.fillOpacity}
                  stroke={item.stroke}
                  stroke-width="1"
                  stroke-dasharray={item.dash}
                />
              {:else if item.type === "circle"}
                <circle cx={item.cx} cy={item.cy} r={item.r} fill={item.fill} />
              {:else if item.type === "image"}
                <image
                  href={item.href}
                  x={item.x}
                  y={item.y}
                  width={item.width}
                  height={item.height}
                  preserveAspectRatio="xMidYMid meet"
                />
              {:else if item.type === "embed"}
                <rect
                  x={item.x}
                  y={item.y}
                  width={item.width}
                  height={item.height}
                  fill="none"
                  stroke="var(--mng-line)"
                  stroke-width="1"
                  stroke-dasharray="4 4"
                />
              {:else if item.type === "text"}
                <text
                  fill={item.fill}
                  font-family={item.family}
                  font-size={item.size}
                  text-anchor={item.anchor}
                  stroke={item.halo}
                  stroke-width={item.halo === undefined ? undefined : 4}
                  stroke-linejoin={item.halo === undefined ? undefined : "round"}
                  paint-order={item.halo === undefined ? undefined : "stroke"}
                  >{#each item.lines as line, j (j)}<tspan x={line.x} y={line.y}
                      >{line.text}</tspan
                    >{/each}</text
                >
              {/if}
            {/each}
          </g>
        {/if}

        <!-- Selection is a hairline frame, never a glow. -->
        {#if figure.selectedId === node.id}
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

      {#if figure.draft.length > 1}
        <path
          d={freedrawPath({ points: figure.draft, size: 2.5 })}
          fill="var(--mng-text)"
        />
      {/if}

      {#if figure.tool === "ellipse" && figure.dragStart !== null && figure.dragCurrent !== null}
        <ellipse
          cx={(figure.dragStart.x + figure.dragCurrent.x) / 2}
          cy={(figure.dragStart.y + figure.dragCurrent.y) / 2}
          rx={Math.abs(figure.dragCurrent.x - figure.dragStart.x) / 2}
          ry={Math.abs(figure.dragCurrent.y - figure.dragStart.y) / 2}
          fill="none"
          stroke="var(--mng-accent)"
          stroke-width="1"
          stroke-dasharray="4 4"
        />
      {:else if figure.tool === "geometry" && figure.dragStart !== null && figure.dragCurrent !== null}
        {@const box = normalizeRect(
          figure.dragStart.x,
          figure.dragStart.y,
          figure.dragCurrent.x - figure.dragStart.x,
          figure.dragCurrent.y - figure.dragStart.y,
        )}
        <rect
          x={box.x}
          y={box.y}
          width={box.width}
          height={box.height}
          fill="none"
          stroke="var(--mng-accent)"
          stroke-width="1"
          stroke-dasharray="4 4"
        />
      {:else if figure.tool === "arrow" && figure.dragStart !== null && figure.dragCurrent !== null}
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
