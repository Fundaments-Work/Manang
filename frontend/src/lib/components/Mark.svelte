<script lang="ts">
  /**
   * The Manang mark `[⩓]` — DESIGN.md §2.1.
   *
   * Two upright pilasters bridged by an apex chevron: an open book and an ink
   * nib. Built on a strict 24-unit grid with zero corner radius, drawn as
   * geometry rather than typeset as a glyph so it stays crisp at any size.
   */
  interface Props {
    size?: number;
    class?: string;
  }

  const { size = 15, class: className = "" }: Props = $props();
</script>

<svg
  viewBox="0 0 24 24"
  width={size}
  height={size}
  fill="none"
  stroke="currentColor"
  stroke-width="1.5"
  stroke-linecap="butt"
  class={className}
  aria-hidden="true"
  focusable="false"
>
  <!-- Apex chevron -->
  <path d="M4 10.5 L12 4.5 L20 10.5" />
  <!-- Pilasters -->
  <path d="M6 10.5 V19.5" />
  <path d="M18 10.5 V19.5" />
  <!-- Ink nib -->
  <path d="M12 4.5 V19.5" />
</svg>
