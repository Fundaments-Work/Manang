<script lang="ts">
  /**
   * The Manang mark: a mountain range stepping down into terraces, one
   * round-capped line (master file: branding/logo.svg). Drawn as geometry in
   * the current text colour, so it follows the theme and stays crisp at any
   * size. `size` is the height; the width follows the drawing's proportions.
   */
  interface Props {
    size?: number;
    class?: string;
  }

  const { size = 16, class: className = "" }: Props = $props();

  // The drawing's bounds with its 14-unit stroke overhang: 565 x 285.5.
  const WIDTH = 565;
  const HEIGHT = 285.5;
</script>

<svg
  viewBox="229 125.5 {WIDTH} {HEIGHT}"
  width={(size * WIDTH) / HEIGHT}
  height={size}
  fill="none"
  stroke="currentColor"
  stroke-width="28"
  stroke-linecap="round"
  stroke-linejoin="round"
  class={className}
  aria-hidden="true"
  focusable="false"
>
  <path
    d="M243 394 L338 258 Q360 226 382 250 L404 276 Q426 294 442 270 L505 162 Q535 118 566 160 L600 214 Q612 232 636 240 L685 240 L685 285 L735 285 L735 327 L780 327 L780 397 L560 397"
  />
</svg>
