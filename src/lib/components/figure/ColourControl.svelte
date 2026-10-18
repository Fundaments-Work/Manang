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
   * One colour setting: the palette tokens first (they follow the theme and
   * print well), then the user's custom colours (marked: they do not follow
   * the theme), then a "+" that opens the picker. Picking a custom colour
   * also puts it in the per-user recent row.
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

  const SWATCH =
    "relative h-5 w-5 border border-line p-0 aria-pressed:outline aria-pressed:outline-1 aria-pressed:outline-offset-1 aria-pressed:outline-text";
</script>

<div class="flex flex-col gap-1" data-colour-control={label}>
  <div class="flex items-center justify-between">
    <span>{label}</span>
    {#if onclear !== undefined && value !== undefined}
      <button
        type="button"
        class="mng-lime-hover bg-transparent p-0 text-[10px] text-text-muted"
        onclick={onclear}>{clearLabel}</button
      >
    {/if}
  </div>
  <div class="flex flex-wrap gap-0.5" role="group" aria-label="{label} colour">
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
    <button
      type="button"
      class="mng-btn h-5 w-5 justify-center border-line bg-transparent p-0 text-text-60"
      aria-label="{label} custom colour"
      aria-expanded={open}
      title="Custom colour"
      onclick={() => (open = !open)}
    >
      <Icon name="plus" size={11} />
    </button>
  </div>
  {#if open}
    <ColourPicker {label} {start} onpick={pickCustom} onclose={() => (open = false)} />
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
