<script lang="ts">
  import type { Snippet } from "svelte";

  /**
   * A small dropdown for the Agent header and composer: a trigger button and a hairline menu
   * under (or over) it. Escape and a click outside close it; Escape returns focus to the
   * trigger. The menu items are plain buttons with `role="menuitem"` in the `children` snippet,
   * which receives `close`.
   */

  interface Props {
    label: string;
    /** Which edge the menu lines up with. */
    align?: "left" | "right";
    /** Open upwards (the composer sits at the bottom). */
    up?: boolean;
    triggerClass?: string;
    onopen?: () => void;
    trigger: Snippet;
    children: Snippet<[() => void]>;
  }

  const {
    label,
    align = "left",
    up = false,
    triggerClass = "",
    onopen,
    trigger,
    children,
  }: Props = $props();

  let open = $state(false);
  let root: HTMLElement | undefined = $state();
  let button: HTMLButtonElement | undefined = $state();

  function close(): void {
    open = false;
  }

  function toggle(): void {
    open = !open;
    if (open) onopen?.();
  }

  $effect(() => {
    if (!open) return;
    const away = (event: MouseEvent): void => {
      if (root !== undefined && !root.contains(event.target as Node)) open = false;
    };
    document.addEventListener("mousedown", away);
    return () => document.removeEventListener("mousedown", away);
  });

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && open) {
      event.preventDefault();
      event.stopPropagation();
      open = false;
      button?.focus();
      return;
    }
    if (!open) return;
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (step === 0) return;
    event.preventDefault();
    const items = [
      ...(root?.querySelectorAll<HTMLElement>('[role="menuitem"]:not(:disabled)') ??
        []),
    ];
    const at = items.indexOf(document.activeElement as HTMLElement);
    items[(at + step + items.length) % items.length]?.focus();
  }
</script>

<span class="relative inline-flex" bind:this={root} {onkeydown} role="presentation">
  <button
    bind:this={button}
    type="button"
    class="inset-focus mng-lime-hover flex h-7 items-center gap-1 text-text-60 {triggerClass}"
    aria-label={label}
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={toggle}
  >
    {@render trigger()}
  </button>
  {#if open}
    <div
      role="menu"
      aria-label={label}
      class="absolute z-30 max-h-72 min-w-48 max-w-[20rem] overflow-y-auto border border-line bg-surface-hi py-1 {up
        ? 'bottom-full mb-1'
        : 'top-full mt-1'} {align === 'right' ? 'right-0' : 'left-0'}"
    >
      {@render children(close)}
    </div>
  {/if}
</span>
