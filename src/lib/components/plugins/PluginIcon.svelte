<script lang="ts">
  import { initials } from "$lib/workbench/market";

  /**
   * A plugin's icon: plugins carry none, so a square with its initials stands
   * in (hairline, tonal, no colour of its own).
   */

  interface Props {
    name: string;
    size?: number;
  }

  const { name, size = 28 }: Props = $props();
</script>

<span
  class="inline-flex shrink-0 items-center justify-center border border-line bg-surface-hi font-light text-text-60 select-none"
  style="width: {size}px; height: {size}px; font-size: {Math.round(size * 0.38)}px"
  aria-hidden="true"
  data-plugin-icon>{initials(name)}</span
>
