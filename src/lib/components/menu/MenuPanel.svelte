<script lang="ts">
  import type { Menu } from "$lib/commands/menu";

  /**
   * One open menu: a flat panel with a hairline border, items in groups split
   * by a rule, each with its current shortcut. Disabled items stay listed
   * (so people learn what exists) but cannot be chosen.
   */

  interface Props {
    menu: Menu;
    /** The menu's index in the bar, for focus lookups. */
    index: number;
    /** Highlighted item index across sections, or -1. */
    highlighted: number;
    /** Opened from the keyboard: mark the highlighted item with the accent. */
    keyboard: boolean;
    placement: "below" | "side";
    onhover: (item: number) => void;
    onrun: (item: number) => void;
  }

  const { menu, index, highlighted, keyboard, placement, onhover, onrun }: Props =
    $props();

  /** Each section's first flat index, so items can be addressed across sections. */
  const offsets = $derived(
    menu.sections.reduce<number[]>(
      (acc, _section, i) => [
        ...acc,
        i === 0 ? 0 : (acc[i - 1] ?? 0) + (menu.sections[i - 1]?.length ?? 0),
      ],
      [],
    ),
  );
</script>

<div
  id="menu-{menu.id}"
  role="menu"
  aria-label={menu.label}
  data-menu-panel={index}
  class="absolute z-50 min-w-[260px] border border-line bg-surface py-1 {placement ===
  'below'
    ? 'top-full left-0'
    : 'top-[-5px] left-full'}"
>
  {#if menu.sections.length === 0}
    <p class="m-0 px-3 py-1.5 text-[13px] text-text-muted">Nothing here yet</p>
  {/if}
  {#each menu.sections as section, s (s)}
    {#if s > 0}
      <div role="separator" class="my-1 h-px bg-line-subtle"></div>
    {/if}
    {#each section as item, k (item.id)}
      {@const flat = (offsets[s] ?? 0) + k}
      {@const on = flat === highlighted}
      <button
        type="button"
        role="menuitem"
        tabindex="-1"
        data-menu-item={flat}
        data-command={item.id}
        aria-disabled={item.enabled ? undefined : "true"}
        aria-keyshortcuts={item.ariaKeys ?? undefined}
        class="relative flex h-7 w-full items-center justify-between gap-8 px-3 text-left text-[13px] focus:outline-none {item.enabled
          ? 'text-text'
          : 'cursor-default text-text-muted'} {on ? 'bg-surface-hi' : ''}"
        onpointerenter={() => onhover(flat)}
        onclick={() => onrun(flat)}
      >
        {#if on && keyboard}
          <span aria-hidden="true" class="absolute inset-y-1 left-0 w-0.5 bg-accent"
          ></span>
        {/if}
        <span class="truncate">{item.label}</span>
        {#if item.shortcutText !== null}
          <span class="shrink-0 font-mono text-[11px] text-text-60"
            >{item.shortcutText}</span
          >
        {/if}
      </button>
    {/each}
  {/each}
</div>
