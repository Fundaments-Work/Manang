<script lang="ts">
  import type { EntryKind } from "$lib/state/library.svelte";

  /**
   * Entry glyph.
   *
   * Drawn as geometry rather than set as an emoji so it inherits the system's
   * hairline weight and stays crisp at 12px on every platform.
   */
  interface Props {
    kind: EntryKind;
    active?: boolean;
  }

  const { kind, active = false }: Props = $props();

  const stroke = "currentColor";
</script>

<span
  class="shrink-0"
  style="color: {active ? 'var(--mng-accent)' : 'var(--mng-text-60)'}"
  aria-hidden="true"
>
  {#if kind === "document"}
    <svg
      viewBox="0 0 12 12"
      width="11"
      height="11"
      fill="none"
      {stroke}
      stroke-width="1"
    >
      <path d="M2.5 1.5 H8 L9.5 3 V10.5 H2.5 Z" />
      <path d="M8 1.5 V3 H9.5" />
      <path d="M4 5.5 H8" />
      <path d="M4 7.5 H8" />
    </svg>
  {:else if kind === "figure"}
    <svg
      viewBox="0 0 12 12"
      width="11"
      height="11"
      fill="none"
      {stroke}
      stroke-width="1"
    >
      <path d="M1.5 8.5 L4.5 4 L7 7 L9 2.5 L10.5 8.5 Z" />
      <path d="M1.5 10.5 H10.5" />
    </svg>
  {:else if kind === "bibliography"}
    <svg
      viewBox="0 0 12 12"
      width="11"
      height="11"
      fill="none"
      {stroke}
      stroke-width="1"
    >
      <path d="M2 2.5 H4 V10.5 H2 Z" />
      <path d="M5.5 2.5 H7.5 V10.5 H5.5 Z" />
      <path d="M9 2.5 L10.5 3.5 V10.5 H9 Z" />
    </svg>
  {:else}
    <svg
      viewBox="0 0 12 12"
      width="11"
      height="11"
      fill="none"
      {stroke}
      stroke-width="1"
    >
      <path d="M1.5 3.5 H4 L5 5 H10.5 V10.5 H1.5 Z" />
    </svg>
  {/if}
</span>
