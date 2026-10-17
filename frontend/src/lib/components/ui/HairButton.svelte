<script lang="ts">
  import type { Snippet } from "svelte";

  /**
   * Square hairline control — the only button shape in the system.
   *
   * Depth comes from a 1px hairline and a tonal surface step. No radius, no
   * shadow. State is never colour alone: an active control also swaps its
   * glyph and announces `pressed` to assistive technology.
   */
  interface Props {
    label: string;
    onactivate?: () => void;
    pressed?: boolean;
    /** Icon-only below 1024px; the label collapses but the tooltip remains. */
    icon: Snippet;
    disabled?: boolean;
  }

  const {
    label,
    onactivate,
    pressed = false,
    icon,
    disabled = false,
  }: Props = $props();
</script>

<button
  type="button"
  {disabled}
  aria-label={label}
  aria-pressed={pressed}
  title={label}
  onclick={onactivate}
  class="inline-flex h-7 items-center gap-1.5 border border-line bg-surface px-2 font-sans text-xs font-regular text-text-60 transition-colors duration-fast ease-manang hover:bg-surface-hi hover:text-text disabled:pointer-events-none disabled:text-text-muted aria-pressed:border-accent aria-pressed:bg-surface-hi aria-pressed:text-accent"
>
  {@render icon()}
  <span class="hidden leading-none min-[1024px]:inline">{label}</span>
</button>
