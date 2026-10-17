<script lang="ts">
  import type { Snippet } from "svelte";
  import { fade, fly } from "svelte/transition";

  /**
   * Overlay surface.
   *
   * DESIGN.md §4.1: overlays glide into view without modal borders or backdrop
   * darkness. The transition uses the system's single curve and its `normal`
   * duration — motion confirms that state changed, it never decorates.
   */
  interface Props {
    /** Short edge the panel enters from. */
    side?: "left" | "right" | "top" | "bottom" | "none";
    duration?: number;
    labelledby?: string;
    children: Snippet;
  }

  const {
    side = "none",
    duration = 220,
    labelledby = undefined,
    children,
  }: Props = $props();

  /** The system's one easing curve, in the form Svelte transitions expect. */
  const easing = (t: number): number => 1 - Math.pow(1 - t, 3);

  const offset = $derived({
    x: side === "left" ? -24 : side === "right" ? 24 : 0,
    y: side === "top" ? -16 : side === "bottom" ? 16 : 0,
  });
</script>

<div class="absolute inset-0 z-20 flex" transition:fade={{ duration: duration / 2 }}>
  <div
    role="dialog"
    aria-modal="true"
    aria-labelledby={labelledby}
    class="absolute inset-0 flex"
    in:fly={{ ...offset, duration, easing }}
  >
    {@render children()}
  </div>
</div>
