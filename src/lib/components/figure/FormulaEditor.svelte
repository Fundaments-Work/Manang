<script lang="ts">
  import { nodeRect } from "$lib/figure/geometry";
  import { errorPlace } from "$lib/figure/mathRender";
  import { strokeColour } from "$lib/figure/palette";
  import { nodeById, styleDefaults } from "$lib/figure/scene";
  import { toScreen } from "$lib/figure/viewport";
  import { kernel, reportError } from "$lib/kernel";
  import { writingApi, type SymbolHit } from "$lib/kernel/api";
  import type { FigureState } from "$lib/state/figure.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { mathRenders } from "$lib/state/mathRenders.svelte";

  /**
   * The formula editor: a small panel anchored under the formula it edits,
   * with a Typst | LaTeX switch, a multi-line field in DM Mono and a live
   * preview on the canvas itself (the node re-renders as you type, after a
   * short pause). Enter inserts a newline; Ctrl+Enter or clicking away
   * applies; Escape drops the draft.
   */
  interface Props {
    figure: FigureState;
    hostWidth: number;
    hostHeight: number;
  }

  const { figure, hostWidth, hostHeight }: Props = $props();

  const writing = writingApi(kernel);
  const f = $derived(figure.formula);
  const node = $derived(f === null ? undefined : nodeById(figure.display, f.id));

  const PANEL_WIDTH = 340;
  const place = $derived.by(() => {
    if (node === undefined) return null;
    const r = nodeRect(node);
    const tl = toScreen(figure.view, { x: r.x, y: r.y });
    const z = figure.view.zoom;
    const bottom = tl.y + r.height * z;
    const left = Math.min(Math.max(tl.x, 8), Math.max(hostWidth - PANEL_WIDTH - 8, 8));
    const below = bottom + 8;
    // Under the formula unless that runs off the canvas; then above it.
    const top = below + 230 > hostHeight ? Math.max(tl.y - 238, 8) : below;
    return { left, top };
  });

  /** The rendering of what the canvas shows (the debounced draft). */
  const render = $derived.by(() => {
    const shown = figure.formulaShown;
    if (shown === null || node?.kind !== "math") return undefined;
    return mathRenders.get(
      shown.language,
      shown.source,
      node.size,
      strokeColour(styleDefaults.stroke(node.style ?? {}), layout.theme),
    );
  });

  let area: HTMLTextAreaElement | undefined = $state();
  let panel: HTMLDivElement | undefined = $state();

  function focusOnMount(el: HTMLTextAreaElement): void {
    el.focus();
    el.select();
  }

  function keydown(event: KeyboardEvent): void {
    event.stopPropagation();
    if (event.key === "Escape") {
      event.preventDefault();
      figure.cancelFormula();
    } else if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      figure.commitFormula();
    }
  }

  function focusout(event: FocusEvent): void {
    const next = event.relatedTarget;
    if (next instanceof Node && panel?.contains(next)) return;
    // Clicking the canvas commits through its pointer handler; this covers
    // tabbing away and clicking other controls.
    figure.commitFormula();
  }

  /* The optional symbol picker: the writing plugin's `writing.symbols`. */
  let query = $state("");
  let hits = $state.raw<readonly SymbolHit[]>([]);
  let searching: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    const q = query.trim();
    const syntax = f?.language ?? "typst";
    clearTimeout(searching);
    if (q === "") {
      hits = [];
      return;
    }
    searching = setTimeout(() => {
      writing
        .symbols(q, syntax, 8)
        .then((r) => (hits = r.symbols))
        .catch((e: unknown) => reportError("figure.symbols", e));
    }, 120);
    return () => clearTimeout(searching);
  });

  function insert(hit: SymbolHit): void {
    const el = area;
    if (el === undefined || f === null) return;
    const from = el.selectionStart;
    const to = el.selectionEnd;
    const next = el.value.slice(0, from) + hit.insert + el.value.slice(to);
    figure.setFormulaSource(next);
    query = "";
    hits = [];
    queueMicrotask(() => {
      el.focus();
      const at = from + hit.insert.length;
      el.setSelectionRange(at, at);
    });
  }

  const error = $derived(render?.error);
</script>

{#if f !== null && place !== null}
  <div
    bind:this={panel}
    class="absolute z-20 flex flex-col gap-1.5 border border-line bg-surface p-2 text-[11px] text-text-60"
    style="left: {place.left}px; top: {place.top}px; width: {PANEL_WIDTH}px"
    role="group"
    aria-label="Formula editor"
    data-formula-editor
    onfocusout={focusout}
  >
    <div class="flex items-center justify-between gap-2">
      <div class="flex" role="group" aria-label="Formula language">
        {#each [["typst", "Typst"], ["latex", "LaTeX"]] as const as [id, name] (id)}
          <button
            type="button"
            class="mng-btn h-6 justify-center px-2.5 first:border-r-0"
            aria-pressed={f.language === id}
            data-language={id}
            onclick={() => figure.setFormulaLanguage(id)}>{name}</button
          >
        {/each}
      </div>
      <span class="text-[10px] text-text-muted">Ctrl+Enter applies, Esc cancels</span>
    </div>

    <textarea
      bind:this={area}
      use:focusOnMount
      class="mng-field selectable h-20 resize-y p-1.5 font-mono text-[13px] leading-snug"
      value={f.source}
      aria-label="Formula source"
      aria-invalid={error !== undefined}
      spellcheck="false"
      oninput={(e) =>
        figure.setFormulaSource((e.currentTarget as HTMLTextAreaElement).value)}
      onkeydown={keydown}
    ></textarea>

    {#if error !== undefined}
      <p class="m-0 font-mono text-[11px] text-text" role="alert" data-formula-error>
        {error.message}{errorPlace(error) === "" ? "" : ` (${errorPlace(error)})`}
      </p>
    {:else}
      <p class="m-0 font-mono text-[10px] text-text-muted" data-formula-help>
        {f.language === "typst"
          ? "Typst: integral_0^1 x^2 dif x = 1/3"
          : "LaTeX: \\int_0^1 x^2\\,dx = \\frac13"}
      </p>
    {/if}

    <div class="flex flex-col gap-1">
      <input
        class="mng-field selectable h-6 px-1.5 text-[11px]"
        bind:value={query}
        placeholder="Insert symbol: alpha, sum, arrow…"
        aria-label="Search symbols"
        onkeydown={(e) => e.stopPropagation()}
      />
      {#if hits.length > 0}
        <div class="flex flex-wrap gap-0.5" role="group" aria-label="Symbols">
          {#each hits as hit (hit.name)}
            <button
              type="button"
              class="mng-btn h-6 gap-1 px-1.5"
              title="{hit.name}: {hit.insert}"
              onclick={() => insert(hit)}
            >
              <span>{hit.preview}</span>
              <span class="font-mono text-[10px] text-text-muted">{hit.insert}</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}
