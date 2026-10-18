<script lang="ts">
  import Icon, { type IconName } from "$lib/components/ui/Icon.svelte";
  import { mergeValues, normaliseAngle } from "$lib/figure/fields";
  import {
    canRotate,
    kindName,
    layoutBox,
    type LayoutField,
  } from "$lib/figure/inspect";
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
  import { figureUi } from "$lib/state/figureUi.svelte";
  import { mathRenders } from "$lib/state/mathRenders.svelte";

  import ColourControl from "./ColourControl.svelte";
  import NumberField from "./NumberField.svelte";
  import Section from "./Section.svelte";
  import Segmented from "./Segmented.svelte";

  /**
   * The inspector, docked to the right of the canvas. It takes its own width
   * (nothing is covered) and collapses to a strip; in a narrow window it is
   * an overlay the toolbar opens. With nothing selected it shows the figure
   * (canvas size, background, grid and snapping, export); with a selection,
   * sections in a fixed order (Layout, Appearance, Fill, Stroke, Text,
   * Effects, Arrange), each collapsible and remembered. Every change is one
   * plugin operation (one undo step) applied to every selected node.
   */
  interface Props {
    figure: FigureState;
    theme: FigureTheme;
    /** A narrow window: shown over the canvas when opened from the toolbar. */
    overlay?: boolean;
  }

  const { figure, theme, overlay = false }: Props = $props();

  const nodes = $derived(figure.selectedNodes);
  const one = $derived(figure.selected);
  const connectors = $derived(nodes.filter(isConnector));
  const open = $derived(overlay ? figureUi.inspectorOverlay : figure.panelOpen);

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
  const textColourable = $derived(
    nodes.some((n) => isShape(n) && n.kind !== "image" && n.kind !== "mermaid"),
  );
  /** Images and Mermaid blocks have no stroke or text style of their own. */
  const styled = $derived(
    nodes.some((n) => n.kind !== "image" && n.kind !== "mermaid"),
  );
  const onlyMath = $derived(nodes.length > 0 && nodes.every((n) => n.kind === "math"));
  const strokable = $derived(styled && !onlyMath);
  const locked = $derived(nodes.length > 0 && nodes.every((n) => n.locked === true));
  const math = $derived(one?.kind === "math" ? one : undefined);
  const mermaid = $derived(one?.kind === "mermaid" ? one : undefined);
  const label = $derived(
    one === undefined || one.kind === "math" || one.kind === "freedraw"
      ? ""
      : (one.label ?? ""),
  );

  /** What a setting holds across the selection (effective values, so unset equals default). */
  function across<T>(pick: (n: FigureNode) => T | undefined) {
    return mergeValues(nodes.map(pick));
  }
  const strokeNow = $derived(across((n) => styleDefaults.stroke(n.style ?? {})));
  const fillNow = $derived(across((n) => styleDefaults.fill(n.style ?? {})));
  const labelColourNow = $derived(across((n) => n.style?.labelColor));
  const widthNow = $derived(across((n) => styleDefaults.strokeWidth(n.style ?? {})));
  const dashNow = $derived(across((n) => styleDefaults.strokeStyle(n.style ?? {})));
  const opacityNow = $derived(
    across((n) => Math.round(styleDefaults.opacity(n.style ?? {}) * 100)),
  );
  const roughNow = $derived(across((n) => styleDefaults.roughness(n.style ?? {})));
  const fontNow = $derived(across((n) => styleDefaults.fontFamily(n.style ?? {})));
  const sizeNow = $derived(across((n) => styleDefaults.fontSize(n.style ?? {})));
  const alignNow = $derived(
    across(
      (n) => n.style?.textAlign ?? (n.kind === "text" ? ("left" as const) : "center"),
    ),
  );
  const fillValue = $derived(normalizeColour(fillNow.value ?? "none"));
  const fillOpacityNow = $derived(
    across((n) => {
      const s: Style = n.style ?? {};
      const f = normalizeColour(styleDefaults.fill(s));
      return s.fillOpacity ?? (HUE_TOKENS.includes(f) ? 20 : 100);
    }),
  );
  const angleNow = $derived(
    across((n) => (isShape(n) ? Math.round((n.angle ?? 0) * 100) / 100 : 0)),
  );
  const box = $derived(layoutBox(figure.display, figure.selection));

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
  const TEXTS = STROKES;
  const FILLS = [
    "none",
    "surface",
    "paper",
    ...HUE_TOKENS.filter((t) => t !== "accent" && t !== "sage"),
  ];
  const BACKGROUNDS = FILLS.map((t) => t);
  const HEADS: Head[] = ["none", "arrow", "triangle", "dot", "bar"];
  const LANGUAGES = [
    { value: "typst", label: "Typst" },
    { value: "latex", label: "LaTeX" },
  ] as const;

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
    ["front", "Bring to front (Ctrl+Shift+])", "to-front"],
    ["forward", "Bring forward (Ctrl+])", "arrowUp"],
    ["backward", "Send backward (Ctrl+[)", "arrowDown"],
    ["back", "Send to back (Ctrl+Shift+[)", "to-back"],
  ];

  const ICON = "mng-icon-btn h-6 w-6";

  function layout(field: LayoutField, value: number): void {
    figure.setLayout(field, value);
  }

  function rotate(angle: number): void {
    const ids = nodes.filter((n) => isShape(n) && n.locked !== true).map((n) => n.id);
    if (ids.length > 0)
      figure.edit({ op: "rotate", ids, angle: normaliseAngle(angle) });
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
    if (mermaid !== undefined && value.trim() !== "" && value !== mermaid.source) {
      figure.update(mermaid.id, { source: value });
    }
  }

  /** Download the figure as an SVG file. */
  async function exportSvg(): Promise<void> {
    const svg = await figure.exportSvg();
    const url = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = `${(figure.path.split("/").pop() ?? "figure").replace(/\.figure(\.json)?$/, "")}.svg`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  const title = $derived(
    nodes.length === 0
      ? "Figure"
      : nodes.length === 1 && one !== undefined
        ? kindName(one)
        : `${nodes.length} selected`,
  );

  function onEscape(event: KeyboardEvent): void {
    if (overlay && event.key === "Escape") {
      event.stopPropagation();
      figureUi.inspectorOverlay = false;
    }
  }

  const SELECT =
    "flex h-6 min-w-0 flex-1 items-center border border-line bg-bg pl-1.5 text-[11px] text-text-60";
</script>

{#if open || !overlay}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <aside
    class="flex min-h-0 shrink-0 flex-col border-l border-line bg-surface text-[11px] text-text-60 {open
      ? 'w-60'
      : 'w-8'} {overlay ? 'absolute inset-y-0 right-0 z-20' : ''}"
    aria-label="Inspector"
    data-properties
    data-open={open}
    onkeydown={onEscape}
  >
    <div
      class="flex h-8 shrink-0 items-center justify-between border-b border-line pr-1 pl-2"
    >
      {#if open}
        <h2 class="m-0 truncate text-[12px] font-normal text-text" data-inspector-title>
          {title}
        </h2>
      {/if}
      <button
        type="button"
        class="mng-icon-btn h-6 w-6 {open ? '' : '-ml-2'}"
        aria-label={open ? "Collapse inspector" : "Expand inspector"}
        aria-expanded={open}
        title={open ? "Collapse inspector" : "Inspector"}
        data-properties-toggle
        onclick={() =>
          overlay
            ? (figureUi.inspectorOverlay = false)
            : figure.setPanelOpen(!figure.panelOpen)}
      >
        <Icon name={open ? "chevron-right" : "sidebar"} size={14} />
      </button>
    </div>

    {#if open}
      <div class="min-h-0 flex-1 overflow-y-auto" role="presentation">
        {#if nodes.length === 0}
          <div data-page-settings>
            <Section id="canvas" title="Canvas">
              <div class="grid grid-cols-2 gap-1">
                <NumberField
                  label="W"
                  name="Canvas width"
                  unit="px"
                  value={figure.scene.width}
                  min={16}
                  max={20000}
                  oncommit={(v) => figure.setCanvasSize(v, figure.scene.height)}
                />
                <NumberField
                  label="H"
                  name="Canvas height"
                  unit="px"
                  value={figure.scene.height}
                  min={16}
                  max={20000}
                  oncommit={(v) => figure.setCanvasSize(figure.scene.width, v)}
                />
              </div>
              <ColourControl
                label="Background"
                value={figure.scene.background}
                fallback="none"
                tokens={BACKGROUNDS}
                {theme}
                onpick={(v) => figure.setBackground(v === "none" ? undefined : v)}
              />
            </Section>
            <Section id="view" title="Grid and snapping">
              <div class="grid grid-cols-2 gap-1">
                <button
                  type="button"
                  class="mng-btn h-6 justify-center px-1 text-[11px]"
                  aria-pressed={figureUi.grid}
                  title="Show a grid behind the page"
                  onclick={() => figureUi.setGrid(!figureUi.grid)}>Grid</button
                >
                <button
                  type="button"
                  class="mng-btn h-6 justify-center px-1 text-[11px]"
                  aria-pressed={figureUi.snap}
                  title="Snap to the edges and centres of other shapes (hold Alt to skip)"
                  onclick={() => figureUi.setSnap(!figureUi.snap)}>Snap</button
                >
              </div>
            </Section>
            <Section id="export" title="Export">
              <button
                type="button"
                class="mng-btn h-6 w-full justify-center px-2 text-[11px]"
                title="Download this figure as an SVG file"
                onclick={() => void exportSvg()}
              >
                <Icon name="export" size={12} /> Export SVG
              </button>
              <p class="m-0 text-[11px] text-text-muted">
                None as background keeps exports transparent.
              </p>
            </Section>
          </div>
        {:else}
          <div data-properties-selection>
            <Section id="layout" title="Layout">
              <div class="grid grid-cols-[1fr_1fr_24px] gap-1">
                <NumberField
                  label="X"
                  name="X position"
                  unit="px"
                  value={box.x}
                  step={1}
                  disabled={locked}
                  oncommit={(v) => layout("x", v)}
                />
                <NumberField
                  label="Y"
                  name="Y position"
                  unit="px"
                  value={box.y}
                  disabled={locked}
                  oncommit={(v) => layout("y", v)}
                />
                <span></span>
                <NumberField
                  label="W"
                  name="Width"
                  unit="px"
                  value={box.width}
                  min={1}
                  disabled={locked}
                  oncommit={(v) => layout("width", v)}
                />
                <NumberField
                  label="H"
                  name="Height"
                  unit="px"
                  value={box.height}
                  min={1}
                  disabled={locked}
                  oncommit={(v) => layout("height", v)}
                />
                <button
                  type="button"
                  class="mng-icon-btn h-6 w-6"
                  aria-label="Keep proportions"
                  aria-pressed={figureUi.lockAspect}
                  title="Keep proportions when changing the size"
                  onclick={() => figureUi.setLockAspect(!figureUi.lockAspect)}
                >
                  <Icon name="ratio" size={13} />
                </button>
                {#if canRotate(nodes)}
                  <NumberField
                    label="R"
                    name="Rotation"
                    unit="°"
                    value={angleNow.value}
                    mixed={angleNow.mixed}
                    min={0}
                    max={359.99}
                    disabled={locked}
                    oncommit={rotate}
                  />
                {/if}
              </div>
            </Section>

            {#if styled}
              <Section id="appearance" title="Appearance">
                <NumberField
                  label="Opacity"
                  name="Opacity"
                  unit="%"
                  value={opacityNow.value}
                  mixed={opacityNow.mixed}
                  step={5}
                  min={0}
                  max={100}
                  oncommit={(v) => figure.setStyle({ opacity: v })}
                />
              </Section>
            {/if}

            {#if fillable}
              <Section id="fill" title="Fill">
                <ColourControl
                  label="Fill"
                  value={fillNow.value}
                  fallback="none"
                  tokens={FILLS}
                  {theme}
                  mixed={fillNow.mixed}
                  onpick={(v) => figure.setStyle({ fill: v })}
                />
                {#if fillValue !== "none" || fillNow.mixed}
                  <NumberField
                    label="Opacity"
                    name="Fill opacity"
                    unit="%"
                    value={fillOpacityNow.value}
                    mixed={fillOpacityNow.mixed}
                    step={5}
                    min={0}
                    max={100}
                    oncommit={(v) => figure.setStyle({ fillOpacity: v })}
                  />
                {/if}
              </Section>
            {/if}

            {#if styled}
              <Section id="stroke" title={onlyMath ? "Colour" : "Stroke"}>
                <ColourControl
                  label={onlyMath ? "Colour" : "Stroke"}
                  value={strokeNow.value}
                  fallback="ink"
                  tokens={STROKES}
                  {theme}
                  mixed={strokeNow.mixed}
                  onpick={(v) => figure.setStyle({ stroke: v })}
                />
                {#if strokable}
                  <NumberField
                    label="Width"
                    name="Stroke width"
                    unit="px"
                    value={widthNow.value}
                    mixed={widthNow.mixed}
                    step={0.5}
                    min={0.5}
                    max={32}
                    oncommit={(v) => figure.setStyle({ strokeWidth: v })}
                  />
                  <Segmented
                    label="Stroke style"
                    options={[
                      { value: "solid", label: "Solid" },
                      { value: "dashed", label: "Dashed" },
                      { value: "dotted", label: "Dotted" },
                    ]}
                    value={dashNow.value}
                    mixed={dashNow.mixed}
                    onpick={(v) => figure.setStyle({ strokeStyle: v })}
                  />
                {/if}
                {#if connectors.length > 0}
                  {@const c = connectors[0]}
                  <Segmented
                    label="Arrow routing"
                    options={[
                      { value: "straight", label: "Straight" },
                      { value: "elbow", label: "Elbow" },
                      { value: "curved", label: "Curved" },
                    ]}
                    value={mergeValues(connectors.map((k) => k.routing ?? "straight"))
                      .value}
                    mixed={mergeValues(connectors.map((k) => k.routing ?? "straight"))
                      .mixed}
                    onpick={(v) => setConnectors({ routing: v as Routing })}
                  />
                  <div class="flex gap-1">
                    <label class={SELECT}>
                      Start
                      <select
                        class="h-full min-w-0 flex-1 border-0 bg-transparent px-1 text-right text-[11px] text-text outline-none"
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
                    </label>
                    <label class={SELECT}>
                      End
                      <select
                        class="h-full min-w-0 flex-1 border-0 bg-transparent px-1 text-right text-[11px] text-text outline-none"
                        aria-label="End head"
                        value={c?.heads?.end ??
                          (c?.kind === "arrow" ? "arrow" : "none")}
                        onchange={(e) =>
                          heads(
                            "end",
                            (e.currentTarget as HTMLSelectElement).value as Head,
                          )}
                      >
                        {#each HEADS as h (h)}<option value={h}>{h}</option>{/each}
                      </select>
                    </label>
                  </div>
                {/if}
              </Section>
            {/if}

            {#if hasText || math !== undefined || mermaid !== undefined}
              <Section
                id="text"
                title={math !== undefined
                  ? "Formula"
                  : mermaid !== undefined
                    ? "Source"
                    : "Text"}
              >
                {#if one !== undefined && hasText && math === undefined}
                  <input
                    class="mng-field selectable h-6 text-[11px]"
                    value={label}
                    aria-label="Label"
                    placeholder={one.kind === "text" ? "Text" : "Label"}
                    onchange={commitLabel}
                    onkeydown={(e) => {
                      e.stopPropagation();
                      if (e.key === "Enter")
                        (e.currentTarget as HTMLInputElement).blur();
                    }}
                  />
                {/if}

                {#if math !== undefined}
                  <div data-formula-properties class="flex flex-col gap-1.5">
                    <Segmented
                      label="Formula language"
                      options={LANGUAGES}
                      value={languageOf(math)}
                      onpick={(id: MathLanguage) =>
                        void figure.setFormula(math.id, { language: id })}
                    />
                    <textarea
                      class="mng-field selectable h-16 resize-y p-1.5 font-mono text-[11px]"
                      value={math.source}
                      aria-label="Formula source"
                      aria-invalid={mathRender?.error !== undefined}
                      spellcheck="false"
                      onchange={commitFormula}
                      onkeydown={(e) => e.stopPropagation()}
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
                    <div class="grid grid-cols-2 gap-1">
                      <NumberField
                        label="Size"
                        name="Formula size"
                        unit="px"
                        value={math.size}
                        min={4}
                        max={400}
                        oncommit={(v) => void figure.setFormula(math.id, { size: v })}
                      />
                      <button
                        type="button"
                        class="mng-btn h-6 justify-center px-2 text-[11px]"
                        onclick={() => figure.openFormula(math.id, false)}>Edit…</button
                      >
                    </div>
                  </div>
                {/if}

                {#if mermaid !== undefined}
                  <textarea
                    class="mng-field selectable h-28 resize-y py-1 font-mono text-[11px]"
                    value={mermaid.source ?? ""}
                    aria-label="Mermaid source"
                    onchange={commitSource}
                    onkeydown={(e) => e.stopPropagation()}
                  ></textarea>
                {/if}

                {#if hasText}
                  <Segmented
                    label="Font"
                    options={[
                      { value: "sans", label: "Sans" },
                      { value: "mono", label: "Mono" },
                    ]}
                    value={fontNow.value}
                    mixed={fontNow.mixed}
                    onpick={(v) => figure.setStyle({ fontFamily: v })}
                  />
                  <div class="grid grid-cols-2 gap-1">
                    <NumberField
                      label="Size"
                      name="Font size"
                      unit="px"
                      value={sizeNow.value}
                      mixed={sizeNow.mixed}
                      min={6}
                      max={200}
                      oncommit={(v) => figure.setStyle({ fontSize: v })}
                    />
                  </div>
                  <Segmented
                    label="Text alignment"
                    options={[
                      { value: "left", label: "Left" },
                      { value: "center", label: "Centre" },
                      { value: "right", label: "Right" },
                    ]}
                    value={alignNow.value}
                    mixed={alignNow.mixed}
                    onpick={(v) => figure.setStyle({ textAlign: v })}
                  />
                {/if}
                {#if textColourable}
                  <ColourControl
                    label="Text"
                    value={labelColourNow.value}
                    fallback={strokeNow.value ?? "ink"}
                    tokens={TEXTS}
                    {theme}
                    mixed={labelColourNow.mixed}
                    onpick={(v) => figure.setStyle({ labelColor: v })}
                    onclear={() => figure.clearLabelColour()}
                    clearLabel="Follows stroke"
                  />
                {/if}
              </Section>
            {/if}

            {#if strokable}
              <Section id="effects" title="Effects">
                <Segmented
                  label="Roughness"
                  options={[
                    { value: 0, label: "Clean" },
                    { value: 1, label: "Sketchy" },
                    { value: 2, label: "Rough", title: "Very sketchy" },
                  ]}
                  value={roughNow.value}
                  mixed={roughNow.mixed}
                  onpick={(v) => figure.setStyle({ roughness: v })}
                />
              </Section>
            {/if}

            <Section id="arrange" title="Arrange">
              <div class="flex gap-0.5" role="group" aria-label="Align">
                {#each ALIGN as [edge, name, icon] (edge)}
                  <button
                    type="button"
                    class={ICON}
                    aria-label={name}
                    title={name}
                    disabled={nodes.length < 2}
                    onclick={() => figure.align(edge)}
                  >
                    <Icon name={icon} size={13} />
                  </button>
                {/each}
                {#each DISTRIBUTE as [axis, name, icon] (axis)}
                  <button
                    type="button"
                    class={ICON}
                    aria-label={name}
                    title={name}
                    disabled={nodes.length < 3}
                    onclick={() => figure.distribute(axis)}
                  >
                    <Icon name={icon} size={13} />
                  </button>
                {/each}
              </div>
              <div class="flex gap-0.5" role="group" aria-label="Order and group">
                {#each ORDER as [to, name, icon] (to)}
                  <button
                    type="button"
                    class={ICON}
                    aria-label={name.replace(/ \(.*\)$/, "")}
                    title={name}
                    onclick={() => figure.reorder(to)}
                  >
                    <Icon name={icon} size={13} />
                  </button>
                {/each}
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
                <button
                  type="button"
                  class={ICON}
                  aria-label={locked ? "Unlock" : "Lock"}
                  aria-pressed={locked}
                  title="{locked ? 'Unlock' : 'Lock'} (Ctrl+Shift+L)"
                  onclick={() => figure.toggleLock()}
                >
                  <Icon name={locked ? "lock" : "unlock"} size={13} />
                </button>
                <button
                  type="button"
                  class={ICON}
                  aria-label="Delete"
                  title="Delete (Del)"
                  onclick={() => figure.deleteSelected()}
                >
                  <Icon name="trash" size={13} />
                </button>
              </div>
            </Section>
          </div>
        {/if}
      </div>
    {/if}
  </aside>
{/if}
