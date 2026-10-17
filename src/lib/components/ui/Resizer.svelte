<script lang="ts">
  /**
   * A draggable divider, also operable from the keyboard.
   *
   * WAI-ARIA window splitter: `role="separator"`, focusable, with
   * `aria-valuenow/min/max`. Arrow keys move it by a step, Shift by a larger
   * step, Home and End jump to the limits, and double-click resets it. The value
   * is in whatever unit the parent chooses; `scale` converts pixels to that unit
   * (1 for pixels, 1 / containerWidth for a ratio).
   */
  interface Props {
    orientation: "vertical" | "horizontal";
    value: number;
    min: number;
    max: number;
    /** Value change per pixel dragged. */
    scale?: number;
    /** Value change per arrow key press. */
    step?: number;
    /** Dragging towards the start grows the value (a panel on the right or bottom). */
    reverse?: boolean;
    label: string;
    onchange: (value: number) => void;
    onreset?: () => void;
  }

  const {
    orientation,
    value,
    min,
    max,
    scale = 1,
    step = 16,
    reverse = false,
    label,
    onchange,
    onreset,
  }: Props = $props();

  let dragging = $state(false);
  let origin = 0;
  let startValue = 0;

  const vertical = $derived(orientation === "vertical");
  const sign = $derived(reverse ? -1 : 1);

  function clamp(next: number): number {
    return Math.min(max, Math.max(min, next));
  }

  function onpointerdown(event: PointerEvent): void {
    if (event.button !== 0) return;
    event.preventDefault();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    dragging = true;
    origin = vertical ? event.clientX : event.clientY;
    startValue = value;
  }

  function onpointermove(event: PointerEvent): void {
    if (!dragging) return;
    const delta = (vertical ? event.clientX : event.clientY) - origin;
    onchange(clamp(startValue + delta * scale * sign));
  }

  function onpointerup(event: PointerEvent): void {
    if (!dragging) return;
    dragging = false;
    (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
  }

  function onkeydown(event: KeyboardEvent): void {
    const grow = vertical ? "ArrowRight" : "ArrowDown";
    const shrink = vertical ? "ArrowLeft" : "ArrowUp";
    const amount = event.shiftKey ? step * 4 : step;
    let next: number | undefined;
    if (event.key === grow) next = value + amount * sign;
    else if (event.key === shrink) next = value - amount * sign;
    else if (event.key === "Home") next = min;
    else if (event.key === "End") next = max;
    if (next === undefined) return;
    event.preventDefault();
    onchange(clamp(next));
  }
</script>

<!-- A 1px rule with a wider invisible hit area, so it is easy to grab without
     looking heavy. The rule turns lime while dragged or focused. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  role="separator"
  tabindex="0"
  aria-label={label}
  aria-orientation={orientation}
  aria-valuenow={Math.round(value * (scale === 1 ? 1 : 100))}
  aria-valuemin={Math.round(min * (scale === 1 ? 1 : 100))}
  aria-valuemax={Math.round(max * (scale === 1 ? 1 : 100))}
  class="group relative z-10 shrink-0 touch-none outline-none {vertical
    ? 'w-px cursor-col-resize'
    : 'h-px cursor-row-resize'} bg-line"
  class:bg-accent={dragging}
  {onpointerdown}
  {onpointermove}
  {onpointerup}
  onpointercancel={onpointerup}
  {onkeydown}
  ondblclick={() => onreset?.()}
>
  <span
    class="absolute {vertical
      ? 'inset-y-0 -inset-x-1'
      : 'inset-x-0 -inset-y-1'} group-hover:bg-text-muted/20 group-focus-visible:outline group-focus-visible:outline-1 group-focus-visible:outline-accent"
    class:bg-text-muted={dragging}
    class:opacity-40={dragging}
    aria-hidden="true"
  ></span>
</div>
