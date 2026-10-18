<script lang="ts">
  import {
    freedrawPath,
    nodeBounds,
    nodeRect,
    normalizeRect,
  } from "$lib/figure/geometry";
  import { handlePositions, RESIZE_HANDLES } from "$lib/figure/handles";
  import { isConnector, isShape, nodeById } from "$lib/figure/scene";
  import { selectionBounds } from "$lib/figure/selection";
  import { sketchFor } from "$lib/figure/render";
  import { HANDLE_PX, type FigureState } from "$lib/state/figure.svelte";

  /**
   * Everything drawn over the scene in scene coordinates: selection frames
   * and handles, the marquee, snapping guides, shape previews while drawing
   * and the laser trail. Chrome lines are 1px hairlines at any zoom
   * (`non-scaling-stroke`); the accent is the selection signal.
   */
  interface Props {
    figure: FigureState;
  }

  const { figure }: Props = $props();

  const z = $derived(figure.view.zoom);
  const hs = $derived(HANDLE_PX / z);
  const g = $derived(figure.gesture);
  const one = $derived(
    figure.gesture === null || figure.gesture.kind === "move"
      ? figure.selected
      : undefined,
  );
  const multi = $derived(
    figure.selection.length > 1
      ? selectionBounds(figure.display, figure.selection)
      : undefined,
  );
  const target = $derived(
    g?.kind === "endpoint" && g.target !== null
      ? nodeById(figure.scene, g.target)
      : undefined,
  );
  const createBox = $derived(
    g?.kind === "create"
      ? normalizeRect(
          g.start.x,
          g.start.y,
          g.current.x - g.start.x,
          g.current.y - g.start.y,
        )
      : undefined,
  );
  const laserPath = $derived(
    figure.laser.length < 2
      ? ""
      : figure.laser.map((p, i) => `${i === 0 ? "M" : "L"}${p.x} ${p.y}`).join(""),
  );
</script>

<g data-overlay pointer-events="none">
  {#each figure.selectedNodes as node (node.id)}
    {@const b = nodeBounds(node)}
    <rect
      x={b.x - 3 / z}
      y={b.y - 3 / z}
      width={b.width + 6 / z}
      height={b.height + 6 / z}
      fill="none"
      stroke="var(--mng-accent)"
      stroke-width="1"
      stroke-dasharray={node.locked === true ? "2 3" : "4 3"}
      vector-effect="non-scaling-stroke"
      data-selected={node.id}
    />
  {/each}

  {#if multi !== undefined}
    <rect
      x={multi.x - 6 / z}
      y={multi.y - 6 / z}
      width={multi.width + 12 / z}
      height={multi.height + 12 / z}
      fill="none"
      stroke="var(--mng-accent)"
      stroke-width="1"
      vector-effect="non-scaling-stroke"
      data-selection-bounds
    />
    {@const mpos = handlePositions(multi, 0, z)}
    {#each RESIZE_HANDLES as h (h)}
      <rect
        data-handle={h}
        data-multi-handle
        x={mpos[h].x - hs / 2}
        y={mpos[h].y - hs / 2}
        width={hs}
        height={hs}
        fill="var(--mng-bg)"
        stroke="var(--mng-accent)"
        stroke-width="1"
        vector-effect="non-scaling-stroke"
      />
    {/each}
  {/if}

  {#if one !== undefined && one.locked !== true}
    {#if isShape(one)}
      {@const r = normalizeRect(one.x, one.y, one.width, one.height)}
      {@const pos = handlePositions(r, one.angle ?? 0, z)}
      {#each RESIZE_HANDLES as h (h)}
        <rect
          data-handle={h}
          x={pos[h].x - hs / 2}
          y={pos[h].y - hs / 2}
          width={hs}
          height={hs}
          fill="var(--mng-bg)"
          stroke="var(--mng-accent)"
          stroke-width="1"
          vector-effect="non-scaling-stroke"
        />
      {/each}
      <rect
        data-handle="rotate"
        x={pos.rotate.x - hs / 2}
        y={pos.rotate.y - hs / 2}
        width={hs}
        height={hs}
        fill="var(--mng-accent)"
        stroke="var(--mng-accent)"
        stroke-width="1"
        vector-effect="non-scaling-stroke"
        transform="rotate(45 {pos.rotate.x} {pos.rotate.y})"
      />
    {:else if one.kind === "math"}
      {@const fpos = handlePositions(nodeRect(one), 0, z)}
      {#each RESIZE_HANDLES as h (h)}
        <rect
          data-handle={h}
          x={fpos[h].x - hs / 2}
          y={fpos[h].y - hs / 2}
          width={hs}
          height={hs}
          fill="var(--mng-bg)"
          stroke="var(--mng-accent)"
          stroke-width="1"
          vector-effect="non-scaling-stroke"
        />
      {/each}
    {:else if isConnector(one)}
      {#each [["start", one.points[0]], ["end", one.points[one.points.length - 1]]] as const as [name, p] (name)}
        {#if p !== undefined}
          <rect
            data-handle={name}
            x={p.x - hs / 2}
            y={p.y - hs / 2}
            width={hs}
            height={hs}
            fill="var(--mng-bg)"
            stroke="var(--mng-accent)"
            stroke-width="1"
            vector-effect="non-scaling-stroke"
          />
        {/if}
      {/each}
    {/if}
  {/if}

  {#if target !== undefined}
    {@const b = nodeBounds(target)}
    <rect
      data-bind-target={target.id}
      x={b.x - 4 / z}
      y={b.y - 4 / z}
      width={b.width + 8 / z}
      height={b.height + 8 / z}
      fill="none"
      stroke="var(--mng-accent)"
      stroke-width="2"
      vector-effect="non-scaling-stroke"
    />
  {/if}

  {#each figure.guides as guide, i (i)}
    {#if guide.axis === "x"}
      <line
        data-guide
        x1={guide.x}
        y1={guide.from}
        x2={guide.x}
        y2={guide.to}
        stroke="var(--mng-accent)"
        stroke-width="1"
        stroke-dasharray="3 3"
        vector-effect="non-scaling-stroke"
      />
    {:else}
      <line
        data-guide
        x1={guide.from}
        y1={guide.y}
        x2={guide.to}
        y2={guide.y}
        stroke="var(--mng-accent)"
        stroke-width="1"
        stroke-dasharray="3 3"
        vector-effect="non-scaling-stroke"
      />
    {/if}
  {/each}

  {#if g?.kind === "marquee"}
    {@const m = normalizeRect(
      g.start.x,
      g.start.y,
      g.current.x - g.start.x,
      g.current.y - g.start.y,
    )}
    <rect
      data-marquee
      x={m.x}
      y={m.y}
      width={m.width}
      height={m.height}
      fill="var(--mng-accent)"
      fill-opacity="0.06"
      stroke="var(--mng-accent)"
      stroke-width="1"
      vector-effect="non-scaling-stroke"
    />
  {/if}

  {#if g?.kind === "create" && createBox !== undefined}
    {#if g.tool === "arrow" || g.tool === "line"}
      <line
        data-preview={g.tool}
        x1={g.start.x}
        y1={g.start.y}
        x2={g.current.x}
        y2={g.current.y}
        stroke="var(--mng-accent)"
        stroke-width="1"
        stroke-dasharray="4 4"
        vector-effect="non-scaling-stroke"
      />
    {:else}
      <path
        data-preview={g.tool}
        d={sketchFor(g.tool, createBox)}
        fill="none"
        stroke="var(--mng-accent)"
        stroke-width="1"
        stroke-dasharray="4 4"
        vector-effect="non-scaling-stroke"
      />
    {/if}
  {/if}

  {#if figure.draft.length > 1}
    <path
      d={freedrawPath({ points: figure.draft, size: 2.5 })}
      fill="var(--mng-text)"
    />
  {/if}

  {#if laserPath !== ""}
    <path
      data-laser
      d={laserPath}
      fill="none"
      stroke="var(--mng-accent)"
      stroke-width="3"
      stroke-linecap="round"
      stroke-linejoin="round"
      stroke-opacity="0.8"
      vector-effect="non-scaling-stroke"
    />
  {/if}
</g>
