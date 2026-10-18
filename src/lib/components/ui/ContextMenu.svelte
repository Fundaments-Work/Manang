<script lang="ts" module>
  export interface MenuItem {
    readonly label: string;
    readonly shortcut?: string;
    readonly disabled?: boolean;
    /** Draws a rule above this item. */
    readonly separated?: boolean;
    /** Set for a choice among options: shows as a radio item, checked or not. */
    readonly checked?: boolean;
    readonly onselect: () => void;
  }
</script>

<script lang="ts">
  import { onMount } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";

  /**
   * Context menu (WAI-ARIA menu pattern): `role="menu"` with `menuitem`s, roving
   * focus, arrows / Home / End, Enter or Space to choose, Esc to close with focus
   * returned to where it was. Opens at the pointer, or at an element for keyboard
   * invocation (Shift+F10 or the Menu key).
   */
  interface Props {
    x: number;
    y: number;
    label: string;
    items: readonly MenuItem[];
    onclose: () => void;
  }

  const { x, y, label, items, onclose }: Props = $props();

  let menu: HTMLDivElement | undefined = $state();
  let current = $state(0);
  const returnTo =
    document.activeElement instanceof HTMLElement ? document.activeElement : null;

  const enabled = $derived(
    items
      .map((item, index) => (item.disabled === true ? -1 : index))
      .filter((i) => i >= 0),
  );

  // Keep the menu inside the window.
  const position = $derived.by(() => {
    const width = 224;
    const height = items.length * 28 + 8;
    return {
      left: Math.max(4, Math.min(x, window.innerWidth - width - 4)),
      top: Math.max(4, Math.min(y, window.innerHeight - height - 4)),
    };
  });

  onMount(() => {
    current = enabled[0] ?? 0;
    menu?.focus();
    const outside = (event: PointerEvent): void => {
      if (menu !== undefined && !menu.contains(event.target as Node)) close(false);
    };
    window.addEventListener("pointerdown", outside, true);
    window.addEventListener("blur", () => close(false));
    return () => window.removeEventListener("pointerdown", outside, true);
  });

  function close(restore: boolean): void {
    onclose();
    if (restore) returnTo?.focus();
  }

  function move(delta: number): void {
    const at = enabled.indexOf(current);
    const next = enabled[(at + delta + enabled.length) % enabled.length];
    if (next !== undefined) current = next;
  }

  function choose(item: MenuItem): void {
    if (item.disabled === true) return;
    close(true);
    item.onselect();
  }

  function onkeydown(event: KeyboardEvent): void {
    switch (event.key) {
      case "ArrowDown":
        move(1);
        break;
      case "ArrowUp":
        move(-1);
        break;
      case "Home":
        current = enabled[0] ?? 0;
        break;
      case "End":
        current = enabled[enabled.length - 1] ?? 0;
        break;
      case "Enter":
      case " ": {
        const item = items[current];
        if (item !== undefined) choose(item);
        break;
      }
      case "Escape":
      case "Tab":
        close(true);
        break;
      default:
        return;
    }
    event.preventDefault();
    event.stopPropagation();
  }

  $effect(() => {
    // Focus follows the roving highlight so screen readers announce it.
    menu?.querySelector<HTMLElement>(`[data-index="${current}"]`)?.focus();
  });
</script>

<div
  bind:this={menu}
  role="menu"
  aria-label={label}
  tabindex="-1"
  class="fixed z-50 w-56 border border-line bg-surface py-1 outline-none"
  style="left: {position.left}px; top: {position.top}px"
  {onkeydown}
>
  {#each items as item, index (item.label)}
    {#if item.separated === true}
      <div role="separator" class="my-1 h-px bg-line"></div>
    {/if}
    <button
      type="button"
      role={item.checked === undefined ? "menuitem" : "menuitemradio"}
      aria-checked={item.checked}
      data-index={index}
      tabindex={index === current ? 0 : -1}
      disabled={item.disabled === true}
      class="inset-focus flex h-7 w-full items-center gap-3 px-3 text-left text-xs disabled:text-text-muted {index ===
      current
        ? 'bg-surface-hi text-text'
        : 'text-text-60'}"
      onmouseenter={() => (current = index)}
      onclick={() => choose(item)}
    >
      {#if item.checked !== undefined}
        <span class="flex w-3.5 shrink-0 justify-center" aria-hidden="true">
          {#if item.checked}<Icon name="check" size={12} />{/if}
        </span>
      {/if}
      <span class="flex-1 truncate">{item.label}</span>
      {#if item.shortcut !== undefined}
        <span class="mng-kbd">{item.shortcut}</span>
      {/if}
    </button>
  {/each}
</div>
