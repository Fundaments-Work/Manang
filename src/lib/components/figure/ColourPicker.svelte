<script lang="ts">
  import { untrack } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";
  import { hexToHsl, hslToHex, parseHexInput } from "$lib/figure/colour";

  /**
   * The in-house colour picker: three HSL sliders and a hex field, square,
   * hairline-bordered, no native colour input. Sliders apply when released
   * (one undo step each), the hex field on Enter or blur. The first colour is
   * whatever the control already shows, so the picker starts where you are.
   */
  interface Props {
    /** `#rrggbb` the picker opens on. */
    start: string;
    onpick: (hex: string) => void;
    onclose: () => void;
    label: string;
  }

  const { start, onpick, onclose, label }: Props = $props();

  // Seeded once: the picker starts on the colour the control shows.
  const seed = untrack(() => hexToHsl(start) ?? { h: 0, s: 0, l: 50 });
  let hue = $state(Math.round(seed.h));
  let sat = $state(Math.round(seed.s));
  let light = $state(Math.round(seed.l));

  const hex = $derived(hslToHex({ h: hue, s: sat, l: light }));
  /** The hex field: follows the sliders until it is typed into. */
  let hexText = $derived(hex);

  function apply(value: string): void {
    onpick(value);
  }

  function commitHex(): void {
    const parsed = parseHexInput(hexText);
    if (parsed === undefined) {
      hexText = hex;
      return;
    }
    const hsl = hexToHsl(parsed);
    if (hsl !== undefined) {
      hue = Math.round(hsl.h);
      sat = Math.round(hsl.s);
      light = Math.round(hsl.l);
    }
    apply(parsed);
  }

  const hueTrack =
    "linear-gradient(to right, hsl(0 100% 50%), hsl(60 100% 50%), hsl(120 100% 50%), hsl(180 100% 50%), hsl(240 100% 50%), hsl(300 100% 50%), hsl(360 100% 50%))";
  const satTrack = $derived(
    `linear-gradient(to right, hsl(${hue} 0% ${light}%), hsl(${hue} 100% ${light}%))`,
  );
  const lightTrack = $derived(
    `linear-gradient(to right, hsl(${hue} ${sat}% 0%), hsl(${hue} ${sat}% 50%), hsl(${hue} ${sat}% 100%))`,
  );
</script>

<div
  class="flex flex-col gap-1.5 border border-line bg-bg p-2"
  role="group"
  aria-label="{label} custom colour"
  data-colour-picker
>
  <div class="flex items-center gap-1.5">
    <span
      class="h-6 w-6 shrink-0 border border-line"
      style="background: {hex}"
      data-picker-preview
    ></span>
    <input
      class="mng-field selectable h-6 min-w-0 flex-1 px-1.5 font-mono text-[11px]"
      bind:value={hexText}
      aria-label="{label} hex"
      spellcheck="false"
      maxlength="7"
      onchange={commitHex}
      onkeydown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          commitHex();
        }
      }}
    />
    <button
      type="button"
      class="mng-icon-btn mng-lime-hover h-6 w-6"
      aria-label="Close colour picker"
      title="Close"
      onclick={onclose}><Icon name="close" size={12} /></button
    >
  </div>
  {#each [["Hue", 360, hueTrack, "h"], ["Saturation", 100, satTrack, "s"], ["Lightness", 100, lightTrack, "l"]] as const as [name, max, track, key] (key)}
    <label class="grid grid-cols-[4.5rem_1fr] items-center gap-1.5 text-[11px]">
      {name}
      {#if key === "h"}
        <input
          type="range"
          class="mng-slider"
          style="background: {track}"
          min="0"
          {max}
          step="1"
          bind:value={hue}
          aria-label="{label} hue"
          onchange={() => apply(hex)}
        />
      {:else if key === "s"}
        <input
          type="range"
          class="mng-slider"
          style="background: {track}"
          min="0"
          {max}
          step="1"
          bind:value={sat}
          aria-label="{label} saturation"
          onchange={() => apply(hex)}
        />
      {:else}
        <input
          type="range"
          class="mng-slider"
          style="background: {track}"
          min="0"
          {max}
          step="1"
          bind:value={light}
          aria-label="{label} lightness"
          onchange={() => apply(hex)}
        />
      {/if}
    </label>
  {/each}
</div>

<style>
  .mng-slider {
    appearance: none;
    height: 10px;
    width: 100%;
    margin: 0;
    border: 1px solid var(--mng-line);
    border-radius: 0;
    cursor: pointer;
  }
  .mng-slider::-webkit-slider-thumb {
    appearance: none;
    width: 6px;
    height: 16px;
    border: 1px solid var(--mng-text);
    border-radius: 0;
    background: var(--mng-bg);
  }
  .mng-slider::-moz-range-thumb {
    width: 6px;
    height: 16px;
    border: 1px solid var(--mng-text);
    border-radius: 0;
    background: var(--mng-bg);
  }
</style>
