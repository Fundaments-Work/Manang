<script lang="ts">
  import Icon, { type IconName } from "$lib/components/ui/Icon.svelte";
  import { errorPlace } from "$lib/figure/mathRender";
  import type { AlignEdge, Axis, ZOrder } from "$lib/figure/ops";
  import {
    HUE_TOKENS,
    normalizeColour,
    strokeColour,
    type FigureTheme,
  } from "$lib/figure/palette";
  import {
    isConnector,
    isShape,
    languageOf,
    styleDefaults,
    type FigureNode,
    type Head,
    type MathLanguage,
    type Routing,
    type Style,
  } from "$lib/figure/scene";
  import type { FigureState } from "$lib/state/figure.svelte";
  import { mathRenders } from "$lib/state/mathRenders.svelte";

  import ColourControl from "./ColourControl.svelte";

  /**
   * The properties column, docked to the right edge of the canvas area: it
   * takes its own width (the canvas shrinks, nothing is covered) and
   * collapses to a strip. With nothing selected it holds the page settings
   * (the background); with a selection, the colours, text, line and
   * arrangement of the selection. Every change is one plugin operation (one
   * undo step), applied to every selected node at once.
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
  /** Text colour is its own setting for shapes, text nodes and frames. */
  const textColourable = $derived(
    nodes.some((n) => isShape(n) && n.kind !== "image" && n.kind !== "mermaid"),
  );
  /** Images and Mermaid blocks have no stroke or text style of their own. */
  const styled = $derived(
    nodes.some((n) => n.kind !== "image" && n.kind !== "mermaid"),
  );
  const locked = $derived(nodes.length > 0 && nodes.every((n) => n.locked === true));

  /** What a setting holds across the selection: one value, or mixed. */
  function across(pick: (s: Style) => string | undefined): {
    value: string | undefined;
    mixed: boolean;
  } {
    const values = nodes.map((n) => pick(n.style ?? {}));
    const [head] = values;
    return { value: head, mixed: values.some((v) => v !== head) };
  }
  const strokeNow = $derived(across((s) => s.stroke));
  const fillNow = $derived(across((s) => s.fill));
  const labelNow = $derived(across((s) => s.labelColor));
  const fillValue = $derived(normalizeColour(fillNow.value ?? "none"));
  const math = $derived(one?.kind === "math" ? one : undefined);
  const mathRender = $derived(
    math === undefined
      ? undefined
      : mathRenders.get(
          languageOf(math),
          math.source,
          math.size,
          strokeColour(styleDefaults.stroke(math.style ?? {}), theme),
        ),
  );

  const STROKES = ["ink", "muted", ...HUE_TOKENS.filter((t) => t !== "sage")];
  const TEXTS = ["ink", "muted", ...HUE_TOKENS.filter((t) => t !== "sage")];
  const FILLS = [
    "none",
    "surface",
    "paper",
    ...HUE_TOKENS.filter((t) => t !== "accent" && t !== "sage"),
  ];
  const BACKGROUNDS = [
    "none",
    "paper",
    "surface",
    ...HUE_TOKENS.filter((t) => t !== "accent" && t !== "sage"),
  ];
  const WIDTHS = [1, 1.5, 2.5, 4];
  const SIZES = [12, 14, 16, 20, 28, 36];
  const FORMULA_SIZES = [16, 20, 24, 32, 48, 64];
  const HEADS: Head[] = ["none", "arrow", "triangle", "dot", "bar"];
  const LANGUAGES: readonly (readonly [MathLanguage, string])[] = [
    ["typst", "Typst"],
    ["latex", "LaTeX"],
  ];

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
    one === undefined || one.kind === "math" || one.kind === "freedraw"
      ? ""
      : (one.label ?? ""),
  );

  function commitLabel(event: Event): void {
    const value = (event.currentTarget as HTMLInputElement).value;
    if (one !== undefined) figure.setLabel(one.id, value);
  }

  function commitFormula(event: Event): void {
    const value = (event.currentTarget as HTMLTextAreaElement).value;
    if (math !== undefined && value.trim() !== "" && value !== math.source) {
      void figure.setFormula(math.id, { source: value });
    }
  }

  function commitSource(event: Event): void {
    const value = (event.currentTarget as HTMLTextAreaElement).value;
    if (one?.kind === "mermaid" && value.trim() !== "" && value !== one.source) {
      figure.update(one.id, { source: value });
    }
  }

  /** Fill opacity shows the token tint's 20 until the user sets one. */
  const fillOpacityShown = $derived(
    style.fillOpacity ?? (HUE_TOKENS.includes(fillValue) ? 20 : 100),
  );
</script>

<aside
  class="flex min-h-0 shrink-0 flex-col border-l border-line bg-surface text-[11px] text-text-60 {figure.panelOpen
    ? 'w-64'
    : 'w-8'}"
  aria-label="Shape properties"
  data-properties
  data-open={figure.panelOpen}
>
  <div
    class="flex h-8 shrink-0 items-center justify-between border-b border-line pr-1 pl-2"
  >
    {#if figure.panelOpen}
      <p class="m-0 font-mono text-[10px] text-text-muted">
        {nodes.length === 0
          ? "Page"
          : nodes.length === 1 && first !== undefined
            ? `${first.kind} ${first.id}`
            : `${nodes.length} selected`}
      </p>
    {/if}
    <button
      type="button"
      class="mng-icon-btn mng-lime-hover h-6 w-6 {figure.panelOpen ? '' : '-ml-2'}"
      aria-label={figure.panelOpen ? "Collapse properties" : "Expand properties"}
      aria-expanded={figure.panelOpen}
      title={figure.panelOpen ? "Collapse properties" : "Properties"}
      data-properties-toggle
      onclick={() => figure.setPanelOpen(!figure.panelOpen)}
    >
      <Icon name={figure.panelOpen ? "chevron-right" : "sidebar"} size={14} />
    </button>
  </div>

  {#if figure.panelOpen}
    <div class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3">
      {#if nodes.length === 0}
        <div data-page-settings class="flex flex-col gap-3">
          <ColourControl
            label="Background"
            value={figure.scene.background}
            fallback="none"
            tokens={BACKGROUNDS}
            {theme}
            onpick={(v) => figure.setBackground(v === "none" ? undefined : v)}
          />
          <p class="m-0 text-[10px] text-text-muted">
            None keeps exports transparent. A background is drawn behind everything in
            the exported SVG and PDF.
          </p>
        </div>
      {:else}
        <div class="flex flex-col gap-3" data-properties-selection>
          {#if one !== undefined && hasText && one.kind !== "math"}
            <label class="flex flex-col gap-1">
              {one.kind === "text" ? "Text" : "Label"}
              <input
                class="mng-field selectable h-7 text-[12px]"
                value={label}
                aria-label="Label"
                onchange={commitLabel}
                onkeydown={(e) => {
                  if (e.key === "Enter") (e.currentTarget as HTMLInputElement).blur();
                }}
              />
            </label>
          {/if}

          {#if math !== undefined}
            <div class="flex flex-col gap-1.5" data-formula-properties>
              <div class="flex items-center justify-between">
                <span>Formula</span>
                <div class="flex" role="group" aria-label="Formula language">
                  {#each LANGUAGES as [id, name] (id)}
                    <button
                      type="button"
                      class="mng-btn h-6 justify-center px-2 first:border-r-0"
                      aria-pressed={languageOf(math) === id}
                      data-language={id}
                      onclick={() => void figure.setFormula(math.id, { language: id })}
                      >{name}</button
                    >
                  {/each}
                </div>
              </div>
              <textarea
                class="mng-field selectable h-16 resize-y p-1.5 font-mono text-[12px]"
                value={math.source}
                aria-label="Formula source"
                aria-invalid={mathRender?.error !== undefined}
                spellcheck="false"
                onchange={commitFormula}
              ></textarea>
              {#if mathRender?.error !== undefined}
                <p
                  class="m-0 font-mono text-[11px] text-text"
                  role="alert"
                  data-formula-error
                >
                  {mathRender.error.message}{errorPlace(mathRender.error) === ""
                    ? ""
                    : ` (${errorPlace(mathRender.error)})`}
                </p>
              {/if}
              <div class="flex items-center gap-1.5">
                <span>Size</span>
                <select
                  class="mng-field h-6 w-16 px-1 text-[11px]"
                  aria-label="Formula size"
                  value={String(math.size)}
                  onchange={(e) =>
                    void figure.setFormula(math.id, {
                      size: Number((e.currentTarget as HTMLSelectElement).value),
                    })}
                >
                  {#each FORMULA_SIZES.includes(math.size) ? FORMULA_SIZES : [...FORMULA_SIZES, math.size].sort((a, b) => a - b) as s (s)}
                    <option value={String(s)}>{s}</option>
                  {/each}
                </select>
                <button
                  type="button"
                  class="mng-btn ml-auto h-6 px-2"
                  onclick={() => figure.openFormula(math.id, false)}>Edit…</button
                >
              </div>
            </div>
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
            <ColourControl
              label={math === undefined ? "Stroke" : "Colour"}
              value={strokeNow.value}
              fallback="ink"
              tokens={STROKES}
              {theme}
              mixed={strokeNow.mixed}
              onpick={(v) => figure.setStyle({ stroke: v })}
            />

            {#if fillable}
              <ColourControl
                label="Fill"
                value={fillNow.value}
                fallback="none"
                tokens={FILLS}
                {theme}
                mixed={fillNow.mixed}
                onpick={(v) => figure.setStyle({ fill: v })}
              />
              {#if fillValue !== "none"}
                <label class="grid grid-cols-[4.5rem_1fr] items-center gap-x-1.5">
                  <span>Fill opacity</span>
                  <input
                    type="range"
                    class="accent-text-60"
                    min="10"
                    max="100"
                    step="10"
                    value={fillOpacityShown}
                    aria-label="Fill opacity"
                    title="Fill opacity, separate from the overall opacity"
                    onchange={(e) =>
                      figure.setStyle({
                        fillOpacity: Number(
                          (e.currentTarget as HTMLInputElement).value,
                        ),
                      })}
                  />
                </label>
              {/if}
            {/if}

            {#if textColourable}
              <ColourControl
                label="Text"
                value={labelNow.value}
                fallback={strokeNow.value ?? "ink"}
                tokens={TEXTS}
                {theme}
                mixed={labelNow.mixed}
                onpick={(v) => figure.setStyle({ labelColor: v })}
                onclear={() => figure.clearLabelColour()}
                clearLabel="Follows stroke"
              />
            {/if}

            <div class="grid grid-cols-[3.5rem_1fr] items-center gap-x-1.5 gap-y-1.5">
              {#if math === undefined}
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
              {/if}

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
                      onclick={() => setConnectors({ routing: r as Routing })}
                      >{r}</button
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
                      heads(
                        "start",
                        (e.currentTarget as HTMLSelectElement).value as Head,
                      )}
                  >
                    {#each HEADS as h (h)}<option value={h}>{h}</option>{/each}
                  </select>
                  <select
                    class="mng-field h-6 flex-1 px-1 text-[11px]"
                    aria-label="End head"
                    value={c?.heads?.end ?? (c?.kind === "arrow" ? "arrow" : "none")}
                    onchange={(e) =>
                      heads(
                        "end",
                        (e.currentTarget as HTMLSelectElement).value as Head,
                      )}
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
        </div>
      {/if}
    </div>
  {/if}
</aside>
