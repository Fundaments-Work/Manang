<script lang="ts">
  import {
    formatField,
    parseField,
    scrubValue,
    stepValue,
    type Bounds,
  } from "$lib/figure/fields";

  /**
   * A numeric inspector field with its label inside it. Drag the label to
   * scrub (Shift for ten times the step); ArrowUp and ArrowDown step the
   * value (Shift for ten); Enter or leaving the field applies it, Escape puts
   * back what was there. Typing accepts a unit and short sums (`120+16`). A
   * selection that disagrees shows "Mixed" until something is typed.
   */
  interface Props {
    /** The short label drawn inside the field ("X", "W", "Stroke"). */
    label: string;
    /** The full accessible name ("X position"). */
    name: string;
    value: number | undefined;
    mixed?: boolean;
    unit?: string;
    step?: number;
    min?: number;
    max?: number;
    disabled?: boolean;
    oncommit: (value: number) => void;
  }

  const {
    label,
    name,
    value,
    mixed = false,
    unit = "",
    step = 1,
    min,
    max,
    disabled = false,
    oncommit,
  }: Props = $props();

  const bounds = $derived<Bounds>({
    ...(min === undefined ? {} : { min }),
    ...(max === undefined ? {} : { max }),
  });

  let input: HTMLInputElement | undefined = $state();
  let focused = $state(false);
  /** What is typed while the field has focus. */
  let draft = $state("");
  /** The value while the label is being dragged. */
  let scrubbing = $state<number | null>(null);

  const shown = $derived(
    scrubbing !== null
      ? formatField(scrubbing)
      : focused
        ? draft
        : value === undefined || mixed
          ? ""
          : formatField(value),
  );
  const placeholder = $derived(mixed ? "Mixed" : value === undefined ? "" : "");
  const current = $derived(scrubbing ?? (mixed ? undefined : value));

  function begin(): void {
    draft = value === undefined || mixed ? "" : formatField(value);
  }

  function commit(): void {
    const text = draft.trim();
    if (text === "") return;
    const parsed = parseField(text, bounds);
    if (parsed === undefined) return;
    if (value === undefined || mixed || Math.abs(parsed - value) >= 0.005) {
      oncommit(parsed);
    }
  }

  function revert(): void {
    begin();
  }

  function onfocus(): void {
    focused = true;
    begin();
    input?.select();
  }

  function onblur(): void {
    if (focused) commit();
    focused = false;
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      commit();
      begin();
      input?.select();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      revert();
      input?.blur();
    } else if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      event.stopPropagation();
      const base = parseField(draft, bounds) ?? value ?? 0;
      draft = formatField(
        stepValue(base, event.key === "ArrowUp" ? 1 : -1, step, event.shiftKey, bounds),
      );
      input?.select();
    } else {
      // Typing stays in the field: the canvas shortcuts must not see it.
      event.stopPropagation();
    }
  }

  let drag: { readonly x: number; readonly from: number; moved: boolean } | null = null;

  function labelDown(event: PointerEvent): void {
    if (disabled || event.button !== 0) return;
    event.preventDefault();
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    drag = { x: event.clientX, from: value ?? 0, moved: false };
  }

  function labelMove(event: PointerEvent): void {
    if (drag === null) return;
    const dx = event.clientX - drag.x;
    if (!drag.moved && Math.abs(dx) < 3) return;
    drag.moved = true;
    scrubbing = scrubValue(drag.from, dx, step, event.shiftKey, bounds);
  }

  function labelUp(event: PointerEvent): void {
    const el = event.currentTarget as HTMLElement;
    if (el.hasPointerCapture(event.pointerId))
      el.releasePointerCapture(event.pointerId);
    const d = drag;
    drag = null;
    if (d === null) return;
    if (!d.moved) {
      input?.focus();
      return;
    }
    const next = scrubbing;
    scrubbing = null;
    if (next !== null && Math.abs(next - d.from) >= 0.005) oncommit(next);
  }

  function labelCancel(): void {
    drag = null;
    scrubbing = null;
  }
</script>

<div
  class="mng-num"
  data-field={name}
  data-disabled={disabled ? "" : undefined}
  data-mixed={mixed ? "" : undefined}
>
  <span
    class="mng-num-label"
    aria-hidden="true"
    title="Drag to change (Shift: ten times faster)"
    onpointerdown={labelDown}
    onpointermove={labelMove}
    onpointerup={labelUp}
    onpointercancel={labelCancel}>{label}</span
  >
  <input
    bind:this={input}
    type="text"
    inputmode="decimal"
    role="spinbutton"
    class="selectable"
    aria-label={name}
    aria-valuenow={current}
    aria-valuemin={min}
    aria-valuemax={max}
    aria-valuetext={mixed ? "Mixed" : undefined}
    autocomplete="off"
    spellcheck="false"
    {disabled}
    {placeholder}
    value={shown}
    oninput={(e) => (draft = (e.currentTarget as HTMLInputElement).value)}
    {onfocus}
    {onblur}
    {onkeydown}
  />
  {#if unit !== "" && !mixed}<span class="mng-num-unit" aria-hidden="true">{unit}</span
    >{/if}
</div>

<style>
  .mng-num {
    display: flex;
    height: 24px;
    min-width: 0;
    align-items: center;
    border: 1px solid var(--mng-line);
    background: var(--mng-bg);
    font-family: var(--font-mono);
    font-size: 11px;
  }
  .mng-num:focus-within {
    border-color: var(--mng-accent);
  }
  .mng-num[data-disabled] {
    color: var(--mng-text-muted);
  }
  .mng-num-label {
    display: flex;
    min-width: 22px;
    height: 100%;
    flex-shrink: 0;
    align-items: center;
    justify-content: center;
    padding: 0 4px;
    color: var(--mng-text-60);
    cursor: ew-resize;
    touch-action: none;
    user-select: none;
  }
  .mng-num-label:hover {
    color: var(--mng-accent);
  }
  .mng-num[data-disabled] .mng-num-label {
    color: var(--mng-text-muted);
    cursor: default;
    pointer-events: none;
  }
  input {
    min-width: 0;
    flex: 1;
    height: 100%;
    border: 0;
    background: transparent;
    padding: 0 2px;
    color: var(--mng-text);
    font: inherit;
    outline: none;
    text-align: right;
  }
  input::placeholder {
    color: var(--mng-text-60);
  }
  .mng-num-unit {
    padding: 0 4px 0 1px;
    color: var(--mng-text-muted);
    user-select: none;
  }
</style>
