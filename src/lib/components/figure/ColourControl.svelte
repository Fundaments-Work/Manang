<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import { hslToHex } from "$lib/figure/colour";
  import {
    colourName,
    isCustomColour,
    normalizeColour,
    tokenValue,
    type FigureTheme,
  } from "$lib/figure/palette";
  import { recentColours } from "$lib/state/recentColours.svelte";

  import ColourPicker from "./ColourPicker.svelte";

  /**
   * One colour setting: a header with the name and the current value, then
   * one tidy row of swatches (the palette tokens first: they follow the theme
   * and print well), the user's recent custom colours (marked: they do not
   * follow the theme), and a "+" that opens the picker as a popover beside
   * the inspector. Picking a custom colour also puts it in the recent row.
   */
  interface Props {
    label: string;
    /** The stored value: `none`, a token or `#rrggbb`; undefined when unset. */
    value: string | undefined;
    /** What a missing value means, shown as pressed (e.g. `ink`, `none`). */
    fallback: string;
    tokens: readonly string[];
    theme: FigureTheme;
    onpick: (value: string) => void;
    /** Offered when the value is set and can go back to following something else. */
    onclear?: (() => void) | undefined;
    clearLabel?: string;
    /** The selection holds different values: nothing shows as pressed. */
    mixed?: boolean;
  }

  const {
    label,
    value,
    fallback,
    tokens,
    theme,
    onpick,
    onclear,
    clearLabel = "Auto",
    mixed = false,
  }: Props = $props();

  let open = $state(false);
  let trigger: HTMLButtonElement | undefined = $state();
  let pos = $state({ left: 0, top: 0 });

  const POPOVER_W = 224;
  const POPOVER_H = 190;

  const current = $derived(normalizeColour(value ?? fallback));
  const customs = $derived.by(() => {
    const list = [...recentColours.list];
    if (isCustomColour(current) && !list.includes(current)) list.unshift(current);
    return list;
  });
  const start = $derived(
    isCustomColour(current)
      ? current
      : (() => {
          const resolved = tokenValue(current, theme);
          return resolved ?? hslToHex({ h: 0, s: 0, l: 50 });
        })(),
  );

  function swatch(token: string): string {
    return token === "none"
      ? "background: transparent"
      : `background: ${tokenValue(token, theme) ?? "transparent"}`;
  }

  function pickCustom(hex: string): void {
    recentColours.use(hex);
    onpick(hex);
  }

  /** Beside the inspector when there is room, else under the button. */
  function place(): void {
    const t = trigger?.getBoundingClientRect();
    if (t === undefined) return;
    const aside = trigger?.closest("aside")?.getBoundingClientRect();
    const edge = aside?.left ?? t.left;
    const left =
      edge >= POPOVER_W + 8
        ? edge - POPOVER_W - 8
        : Math.max(8, Math.min(t.left, window.innerWidth - POPOVER_W - 8));
    const top = Math.max(8, Math.min(t.top, window.innerHeight - POPOVER_H - 8));
    pos = {
      left,
      top:
        edge >= POPOVER_W + 8
          ? top
          : Math.min(t.bottom + 4, window.innerHeight - POPOVER_H - 8),
    };
  }

  function toggle(): void {
    if (open) {
      close(false);
      return;
    }
    place();
    open = true;
  }

  function close(refocus = true): void {
    open = false;
    if (refocus) trigger?.focus();
  }

  function onwindowkey(event: KeyboardEvent): void {
    if (open && event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
    }
  }

  function onwindowdown(event: PointerEvent): void {
    if (!open) return;
    const el = event.target;
    if (
      el instanceof Element &&
      (el.closest("[data-colour-picker]") !== null || trigger?.contains(el) === true)
    ) {
      return;
    }
    open = false;
  }

  const SWATCH =
    "relative h-4 w-4 shrink-0 border border-line p-0 aria-pressed:outline aria-pressed:outline-1 aria-pressed:outline-offset-1 aria-pressed:outline-text";
</script>

<svelte:window onkeydowncapture={onwindowkey} onpointerdowncapture={onwindowdown} />

<div class="flex flex-col gap-1" data-colour-control={label}>
  <div class="flex h-5 items-center justify-between text-[11px]">
    <span class="text-text-60">{label}</span>
    <span class="flex min-w-0 items-center gap-1.5">
      {#if onclear !== undefined && value !== undefined}
        <button
          type="button"
          class="mng-lime-hover bg-transparent p-0 text-[11px] text-text-muted"
          onclick={onclear}>{clearLabel}</button
        >
      {/if}
      <span class="truncate font-mono text-[11px] text-text-muted"
        >{mixed ? "Mixed" : colourName(current)}</span
      >
    </span>
  </div>
  <div class="flex flex-wrap gap-1" role="group" aria-label="{label} colour">
    {#each tokens as token (token)}
      <button
        type="button"
        class={SWATCH}
        style={swatch(token)}
        aria-label="{label} {token}"
        aria-pressed={!mixed && current === token}
        title={token === "none" ? "No fill" : token}
        data-token={token}
        onclick={() => onpick(token)}
      >
        {#if token === "none"}<Icon name="close" size={12} />{/if}
      </button>
    {/each}
    <button
      bind:this={trigger}
      type="button"
      class="mng-icon-btn h-4 w-4 border border-line"
      aria-label="{label} custom colour"
      aria-expanded={open}
      aria-haspopup="dialog"
      title="Custom colour"
      onclick={toggle}
    >
      <Icon name="plus" size={10} />
    </button>
  </div>
  {#if customs.length > 0}
    <div class="flex flex-wrap gap-1" role="group" aria-label="{label} recent colours">
      {#each customs as hex (hex)}
        <button
          type="button"
          class="{SWATCH} mng-custom"
          style="background: {hex}"
          aria-label="{label} {colourName(hex)}"
          aria-pressed={!mixed && current === hex}
          title="{colourName(hex)} (custom: does not follow the theme)"
          data-custom={hex}
          onclick={() => pickCustom(hex)}
        ></button>
      {/each}
    </div>
  {/if}
  {#if open}
    <div
      class="fixed z-30"
      style="left: {pos.left}px; top: {pos.top}px; width: {POPOVER_W}px"
    >
      <ColourPicker {label} {start} onpick={pickCustom} onclose={() => close()} />
    </div>
  {/if}
</div>

<style>
  .mng-custom::after {
    content: "";
    position: absolute;
    right: 1px;
    bottom: 1px;
    width: 4px;
    height: 4px;
    border: 1px solid var(--mng-text);
    background: var(--mng-bg);
  }
</style>
