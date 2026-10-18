<script lang="ts">
  import Icon, { type IconName } from "$lib/components/ui/Icon.svelte";
  import type { AlignEdge, Axis, ZOrder } from "$lib/figure/ops";
  import { HUE_TOKENS, tokenValue, type FigureTheme } from "$lib/figure/palette";
  import {
    isConnector,
    isShape,
    type FigureNode,
    type Head,
    type Routing,
    type Style,
  } from "$lib/figure/scene";
  import type { FigureState } from "$lib/state/figure.svelte";

  /**
   * The context properties panel: shown only while something is selected,
   * it edits the selection's style, text and arrangement. Every change is
   * one plugin operation (one undo step).
   */
  interface Props {
    figure: FigureState;
    theme: FigureTheme;
  }

  const { figure, theme }: Props = $props();

  const nodes = $derived(figure.selectedNodes);
  const first = $derived<FigureNode | undefined>(nodes[0]);
  const style = $derived<Style>(first?.style ?? {});
  const one = $derived(figure.selected);
  const connectors = $derived(nodes.filter(isConnector));
  const fillable = $derived(
    nodes.some(
      (n) =>
        isShape(n) && n.kind !== "text" && n.kind !== "image" && n.kind !== "mermaid",
    ),
  );
  const hasText = $derived(
    nodes.some(
      (n) =>
        (isShape(n) && n.kind !== "image" && n.kind !== "mermaid") || isConnector(n),
    ),
  );
  /** Images and Mermaid blocks have no stroke or text style of their own. */
  const styled = $derived(
    nodes.some((n) => n.kind !== "image" && n.kind !== "mermaid"),
  );
  const locked = $derived(nodes.length > 0 && nodes.every((n) => n.locked === true));

  const STROKES = ["ink", "muted", ...HUE_TOKENS.filter((t) => t !== "sage")];
  const FILLS = [
    "none",
    "surface",
    "paper",
    ...HUE_TOKENS.filter((t) => t !== "accent" && t !== "sage"),
  ];
  const WIDTHS = [1, 1.5, 2.5, 4];
  const SIZES = [12, 14, 16, 20, 28, 36];
  const HEADS: Head[] = ["none", "arrow", "triangle", "dot", "bar"];

  const ALIGN: readonly [AlignEdge, string, IconName][] = [
    ["left", "Align left", "align-left"],
    ["center", "Align centre", "align-center"],
    ["right", "Align right", "align-right"],
    ["top", "Align top", "align-top"],
    ["middle", "Align middle", "align-middle"],
    ["bottom", "Align bottom", "align-bottom"],
  ];
  const DISTRIBUTE: readonly [Axis, string, IconName][] = [
    ["horizontal", "Distribute horizontally", "distribute-h"],
    ["vertical", "Distribute vertically", "distribute-v"],
  ];
  const ORDER: readonly [ZOrder, string, IconName][] = [
    ["front", "Bring to front", "to-front"],
    ["forward", "Bring forward", "arrowUp"],
    ["backward", "Send backward", "arrowDown"],
    ["back", "Send to back", "to-back"],
  ];

  const CHIP =
    "mng-btn h-6 min-w-6 justify-center border-transparent bg-transparent px-1 text-text-60 hover:text-text aria-pressed:border-text-60 aria-pressed:text-text";
  const ICON = `${CHIP} w-6 px-0`;

  function swatch(token: string): string {
    if (token === "none") return "background: transparent";
    return `background: ${tokenValue(token, theme) ?? "transparent"}`;
  }

  function setConnectors(set: Record<string, unknown>): void {
    if (connectors.length === 0) return;
    figure.edit({
      op: "batch",
      ops: connectors.map((c) => ({ op: "update" as const, id: c.id, set })),
    });
  }

  function heads(end: "start" | "end", head: Head): void {
    for (const c of connectors) {
      const next = { ...(c.heads ?? {}), [end]: head };
      figure.update(c.id, { heads: next });
    }
  }

  const label = $derived(
    one === undefined
      ? ""
      : one.kind === "math"
        ? one.tex
        : one.kind === "freedraw"
          ? ""
          : (one.label ?? ""),
  );

  function commitLabel(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    if (one === undefined) return;
    if (one.kind === "math") figure.setMathText(one.id, value);
    else figure.setLabel(one.id, value);
  }

  function commitSource(event: Event): void {
    const value = (event.currentTarget as HTMLTextAreaElement).value;
    if (one?.kind === "mermaid" && value.trim() !== "" && value !== one.source) {
      figure.update(one.id, { source: value });
    }
  }
</script>

{#if nodes.length > 0}
  <aside
    class="absolute top-2 left-2 z-10 flex max-h-[calc(100%-2.5rem)] w-64 flex-col gap-3 overflow-y-auto border border-line bg-surface p-3 text-[11px] text-text-60"
    aria-label="Shape properties"
  >
    <p class="m-0 font-mono text-[10px] text-text-muted">
      {nodes.length === 1 && first !== undefined
        ? `${first.kind} ${first.id}`
        : `${nodes.length} selected`}
    </p>

    {#if one !== undefined && (hasText || one.kind === "math")}
      <label class="flex flex-col gap-1">
        {one.kind === "math" ? "Formula" : one.kind === "text" ? "Text" : "Label"}
        <input
          class="mng-field selectable h-7 text-[12px] {one.kind === 'math'
            ? 'font-mono'
            : ''}"
          value={label}
          aria-label={one.kind === "math" ? "Formula text" : "Label"}
          onchange={commitLabel}
          onkeydown={(e) => {
            if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur();
          }}
        />
      </label>
    {/if}

    {#if one?.kind === "mermaid"}
      <label class="flex flex-col gap-1">
        Mermaid source
        <textarea
          class="mng-field selectable h-28 resize-y py-1 font-mono text-[11px]"
          value={one.source ?? ""}
          aria-label="Mermaid source"
          onchange={commitSource}
        ></textarea>
      </label>
    {/if}

    {#if styled}
      <div class="flex flex-col gap-1">
        Stroke
        <div class="flex flex-wrap gap-0.5" role="group" aria-label="Stroke colour">
          {#each STROKES as token (token)}
            <button
              type="button"
              class="h-5 w-5 border border-line p-0 aria-pressed:outline aria-pressed:outline-1 aria-pressed:outline-offset-1 aria-pressed:outline-text"
              style={swatch(token)}
              aria-label="Stroke {token}"
              aria-pressed={(style.stroke ?? "ink") === token}
              title={token}
              onclick={() => figure.setStyle({ stroke: token })}
            ></button>
          {/each}
        </div>
      </div>

      {#if fillable}
        <div class="flex flex-col gap-1">
          Fill
          <div class="flex flex-wrap gap-0.5" role="group" aria-label="Fill colour">
            {#each FILLS as token (token)}
              <button
                type="button"
                class="h-5 w-5 border border-line p-0 aria-pressed:outline aria-pressed:outline-1 aria-pressed:outline-offset-1 aria-pressed:outline-text"
                style={swatch(token)}
                aria-label="Fill {token}"
                aria-pressed={(style.fill ?? "none") === token}
                title={token === "none"
                  ? "No fill"
                  : `${token}${HUE_TOKENS.includes(token) ? " (tint)" : ""}`}
                onclick={() => figure.setStyle({ fill: token })}
              >
                {#if token === "none"}<Icon name="close" size={12} />{/if}
              </button>
            {/each}
          </div>
        </div>
      {/if}

      <div class="grid grid-cols-[3.5rem_1fr] items-center gap-x-1.5 gap-y-1.5">
        <span>Width</span>
        <div class="flex gap-0.5" role="group" aria-label="Stroke width">
          {#each WIDTHS as w (w)}
            <button
              type="button"
              class={CHIP}
              aria-pressed={(style.strokeWidth ?? 1.5) === w}
              aria-label="Stroke width {w}"
              onclick={() => figure.setStyle({ strokeWidth: w })}>{w}</button
            >
          {/each}
        </div>

        <span>Line</span>
        <div class="flex gap-0.5" role="group" aria-label="Stroke style">
          {#each ["solid", "dashed", "dotted"] as const as s (s)}
            <button
              type="button"
              class={CHIP}
              aria-pressed={(style.strokeStyle ?? "solid") === s}
              onclick={() => figure.setStyle({ strokeStyle: s })}>{s}</button
            >
          {/each}
        </div>

        <span>Style</span>
        <div class="flex gap-0.5" role="group" aria-label="Roughness">
          {#each [[0, "clean"], [1, "sketchy"], [2, "rough"]] as const as [r, name] (r)}
            <button
              type="button"
              class={CHIP}
              aria-pressed={(style.roughness ?? 0) === r}
              title={r === 2 ? "Very sketchy" : undefined}
              onclick={() => figure.setStyle({ roughness: r })}>{name}</button
            >
          {/each}
        </div>

        <span>Opacity</span>
        <input
          type="range"
          class="accent-text-60"
          min="10"
          max="100"
          step="10"
          value={style.opacity ?? 100}
          aria-label="Opacity"
          onchange={(e) =>
            figure.setStyle({
              opacity: Number((e.currentTarget as HTMLInputElement).value),
            })}
        />

        {#if hasText}
          <span>Font</span>
          <div class="flex gap-0.5" role="group" aria-label="Font">
            {#each ["sans", "mono"] as const as f (f)}
              <button
                type="button"
                class={CHIP}
                aria-pressed={(style.fontFamily ?? "sans") === f}
                onclick={() => figure.setStyle({ fontFamily: f })}>{f}</button
              >
            {/each}
            <select
              class="mng-field h-6 w-14 px-1 text-[11px]"
              aria-label="Font size"
              value={String(style.fontSize ?? 16)}
              onchange={(e) =>
                figure.setStyle({
                  fontSize: Number((e.currentTarget as HTMLSelectElement).value),
                })}
            >
              {#each SIZES as s (s)}<option value={String(s)}>{s}</option>{/each}
            </select>
          </div>

          <span>Align</span>
          <div class="flex gap-0.5" role="group" aria-label="Text alignment">
            {#each ["left", "center", "right"] as const as a (a)}
              <button
                type="button"
                class={CHIP}
                aria-pressed={style.textAlign === a}
                aria-label="Text {a}"
                onclick={() => figure.setStyle({ textAlign: a })}
                >{a === "center" ? "centre" : a}</button
              >
            {/each}
          </div>
        {/if}

        {#if connectors.length > 0}
          {@const c = connectors[0]}
          <span>Route</span>
          <div class="flex gap-0.5" role="group" aria-label="Arrow routing">
            {#each ["straight", "elbow", "curved"] as const as r (r)}
              <button
                type="button"
                class={CHIP}
                aria-pressed={(c?.routing ?? "straight") === r}
                onclick={() => setConnectors({ routing: r as Routing })}>{r}</button
              >
            {/each}
          </div>
          <span>Heads</span>
          <div class="flex gap-1">
            <select
              class="mng-field h-6 flex-1 px-1 text-[11px]"
              aria-label="Start head"
              value={c?.heads?.start ?? "none"}
              onchange={(e) =>
                heads("start", (e.currentTarget as HTMLSelectElement).value as Head)}
            >
              {#each HEADS as h (h)}<option value={h}>{h}</option>{/each}
            </select>
            <select
              class="mng-field h-6 flex-1 px-1 text-[11px]"
              aria-label="End head"
              value={c?.heads?.end ?? (c?.kind === "arrow" ? "arrow" : "none")}
              onchange={(e) =>
                heads("end", (e.currentTarget as HTMLSelectElement).value as Head)}
            >
              {#each HEADS as h (h)}<option value={h}>{h}</option>{/each}
            </select>
          </div>
        {/if}
      </div>
    {/if}

    <div class="flex flex-col gap-1">
      Arrange
      <div class="flex flex-wrap gap-0.5">
        {#each ORDER as [to, title, icon] (to)}
          <button
            type="button"
            class={ICON}
            aria-label={title}
            {title}
            onclick={() => figure.reorder(to)}
          >
            <Icon name={icon} size={13} />
          </button>
        {/each}
        <span class="mx-1 h-6 w-px bg-line" aria-hidden="true"></span>
        <button
          type="button"
          class={ICON}
          aria-label="Group"
          title="Group (Ctrl+G)"
          disabled={nodes.length < 2}
          onclick={() => figure.group()}
        >
          <Icon name="group" size={13} />
        </button>
        <button
          type="button"
          class={ICON}
          aria-label="Ungroup"
          title="Ungroup (Ctrl+Shift+G)"
          disabled={!nodes.some((n) => (n.groups ?? []).length > 0)}
          onclick={() => figure.ungroup()}
        >
          <Icon name="ungroup" size={13} />
        </button>
      </div>
      {#if nodes.length >= 2}
        <div class="flex flex-wrap gap-0.5" role="group" aria-label="Align">
          {#each ALIGN as [edge, title, icon] (edge)}
            <button
              type="button"
              class={ICON}
              aria-label={title}
              {title}
              onclick={() => figure.align(edge)}
            >
              <Icon name={icon} size={13} />
            </button>
          {/each}
          {#if nodes.length >= 3}
            {#each DISTRIBUTE as [axis, title, icon] (axis)}
              <button
                type="button"
                class={ICON}
                aria-label={title}
                {title}
                onclick={() => figure.distribute(axis)}
              >
                <Icon name={icon} size={13} />
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>

    <div class="flex gap-1 border-t border-line pt-2">
      <button
        type="button"
        class="mng-btn h-7 flex-1 justify-center px-2"
        aria-label={locked ? "Unlock" : "Lock"}
        aria-pressed={locked}
        title="Lock (Ctrl+Shift+L)"
        onclick={() => figure.toggleLock()}
      >
        <Icon name={locked ? "lock" : "unlock"} size={13} />
        {locked ? "Unlock" : "Lock"}
      </button>
      <button
        type="button"
        class="mng-btn h-7 flex-1 justify-center px-2"
        aria-label="Delete"
        title="Delete (Del)"
        onclick={() => figure.deleteSelected()}
      >
        <Icon name="trash" size={13} /> Delete
      </button>
    </div>
  </aside>
{/if}
