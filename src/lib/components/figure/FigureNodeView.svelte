<script lang="ts">
  import { nodeVisual } from "$lib/figure/render";
  import { createScene, type FigureNode } from "$lib/figure/scene";
  import type { FigureTheme } from "$lib/figure/palette";
  import { mermaidRenders } from "$lib/state/figures.svelte";

  /**
   * One scene node on the canvas, drawn from the same primitives the export
   * writes (`render.ts`). A connector also receives the shapes it is bound to,
   * so it redraws when they move and nothing else does.
   */
  interface Props {
    node: FigureNode;
    /** The shapes a connector's ends are bound to (passed one by one so an
     * unchanged target keeps the connector from redrawing). */
    startTarget?: FigureNode | undefined;
    endTarget?: FigureNode | undefined;
    theme: FigureTheme;
    /** Dimmed (marked by the eraser). */
    dim?: boolean;
    /** Hide the label while the inline editor covers it. */
    hideText?: boolean;
  }

  const {
    node,
    startTarget,
    endTarget,
    theme,
    dim = false,
    hideText = false,
  }: Props = $props();

  const visual = $derived(
    nodeVisual(
      {
        ...createScene(1, 1),
        nodes: [startTarget, endTarget].filter((n) => n !== undefined),
      },
      node,
      theme,
    ),
  );

  const mermaid = $derived(
    node.kind === "mermaid" && node.source !== undefined
      ? mermaidRenders.get(node.source, theme === "print" ? "print" : theme)
      : undefined,
  );
</script>

{#if visual !== undefined}
  <g
    data-node-id={node.id}
    data-kind={node.kind}
    transform={visual.rotate === undefined
      ? undefined
      : `rotate(${visual.rotate.angle} ${visual.rotate.cx} ${visual.rotate.cy})`}
    opacity={dim ? 0.3 : visual.opacity}
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
          vector-effect="non-scaling-stroke"
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
        {#if mermaid?.url !== undefined}
          <image
            href={mermaid.url}
            x={item.x}
            y={item.y}
            width={item.width}
            height={item.height}
            preserveAspectRatio="xMidYMid meet"
          />
        {:else}
          <rect
            x={item.x}
            y={item.y}
            width={item.width}
            height={item.height}
            fill="none"
            stroke="var(--mng-line)"
            stroke-width="1"
            vector-effect="non-scaling-stroke"
            stroke-dasharray="4 4"
          />
          <text
            x={item.x + 8}
            y={item.y + 18}
            fill="var(--mng-text-60)"
            font-family="var(--font-mono)"
            font-size="12"
            >{mermaid?.error === undefined
              ? "Rendering Mermaid…"
              : `Mermaid: ${mermaid.error.slice(0, 80)}`}</text
          >
        {/if}
      {:else if item.type === "text" && !hideText}
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
