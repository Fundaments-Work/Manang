<script lang="ts" generics="T extends string | number">
  /**
   * A few exclusive choices as one joined row of text buttons. The chosen one
   * is `aria-pressed` (a tonal step and a heavier border); when a selection
   * disagrees nothing is pressed.
   */
  interface Option {
    readonly value: T;
    readonly label: string;
    readonly title?: string;
  }
  interface Props {
    label: string;
    options: readonly Option[];
    value: T | undefined;
    mixed?: boolean;
    disabled?: boolean;
    onpick: (value: T) => void;
  }

  const {
    label,
    options,
    value,
    mixed = false,
    disabled = false,
    onpick,
  }: Props = $props();
</script>

<div class="flex" role="group" aria-label={label} data-segmented={label}>
  {#each options as o (o.value)}
    <button
      type="button"
      class="mng-btn h-6 min-w-0 flex-1 justify-center px-1 text-[11px] not-first:-ml-px"
      aria-pressed={!mixed && value === o.value}
      title={o.title ?? o.label}
      {disabled}
      onclick={() => onpick(o.value)}>{o.label}</button
    >
  {/each}
</div>
