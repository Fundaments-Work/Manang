<script lang="ts">
  import { onMount, tick } from "svelte";

  import { menuItems, mnemonicIndex, type Menu } from "$lib/commands/menu";
  import {
    CLOSED,
    navigate,
    type NavEvent,
    type NavKey,
    type NavMenus,
    type NavState,
  } from "$lib/commands/menuNav";
  import { dialog } from "$lib/state/dialog.svelte";
  import { palette } from "$lib/state/palette.svelte";
  import { slides } from "$lib/state/slides.svelte";
  import { switcher } from "$lib/state/switcher.svelte";

  import MenuPanel from "./MenuPanel.svelte";
  import { menuModel } from "./menuModel.svelte";

  /**
   * The menu bar in the title bar: File, Edit, View, Go, Build, Help, VS Code
   * style. Every item is a command (see `commands/menu.ts`); navigation is
   * the `menuNav` state machine. Below 900 px (`folded`) the six menus fold
   * into one "Menu" button that opens the same model.
   *
   * The bar's buttons are not drag regions, so the title bar's empty space
   * still moves the window.
   */

  interface Props {
    folded: boolean;
  }

  const { folded }: Props = $props();

  let nav = $state<NavState>(CLOSED);
  /** Bumped on opening, so enabled states that read focus are fresh. */
  let opened = $state(0);
  let root: HTMLElement | undefined = $state();
  /** Where focus was before the menu took it, to give it back. */
  let returnFocus: HTMLElement | null = null;

  const menus: Menu[] = $derived.by(() => {
    void opened;
    return menuModel.build();
  });
  const navMenus: NavMenus = {
    get count() {
      return menus.length;
    },
    enabled: (i) => {
      const menu = menus[i];
      return menu === undefined ? [] : menuItems(menu).map((item) => item.enabled);
    },
  };

  onMount(() => menuModel.load());

  $effect(() => {
    const want = folded;
    if (want !== nav.folded)
      nav = navigate(nav, { type: "fold", folded: want }, navMenus).state;
  });

  function restoreFocus(): void {
    const target = returnFocus;
    returnFocus = null;
    if (target !== null && target.isConnected) target.focus();
    else if (root?.contains(document.activeElement) === true)
      (document.activeElement as HTMLElement).blur();
  }

  function send(event: NavEvent): void {
    const before = nav;
    const { state, effect } = navigate(nav, event, navMenus);
    if (before.mode === "closed" && state.mode !== "closed") {
      const active = document.activeElement;
      returnFocus =
        active instanceof HTMLElement && root?.contains(active) !== true
          ? active
          : null;
      opened += 1;
    }
    // Read the item before the model can change under it.
    const runMenu = effect?.type === "run" ? menus[effect.menu] : undefined;
    const item =
      effect?.type === "run" && runMenu !== undefined
        ? menuItems(runMenu)[effect.item]
        : undefined;
    nav = state;
    if (effect?.type === "restore-focus") restoreFocus();
    else if (effect?.type === "run") {
      restoreFocus();
      item?.run();
    } else if (before.mode !== "closed" && state.mode === "closed") returnFocus = null;
    void moveFocus();
  }

  /** Roving focus: the highlighted menu or item holds DOM focus while open. */
  async function moveFocus(): Promise<void> {
    if (nav.mode === "closed") return;
    await tick();
    const selector =
      nav.item >= 0 && (!nav.folded || nav.submenu)
        ? `[data-menu-panel="${nav.active}"] [data-menu-item="${nav.item}"]`
        : nav.folded
          ? `[data-menu-row="${nav.active}"]`
          : `[data-menu-bar="${nav.active}"]`;
    root?.querySelector<HTMLElement>(selector)?.focus();
  }

  const NAV_KEYS: ReadonlySet<string> = new Set([
    "ArrowLeft",
    "ArrowRight",
    "ArrowUp",
    "ArrowDown",
    "Home",
    "End",
    "Enter",
    " ",
    "Escape",
    "Tab",
  ]);

  function onkeydown(event: KeyboardEvent): void {
    if (nav.mode === "closed") return;
    if (NAV_KEYS.has(event.key)) {
      event.preventDefault();
      event.stopPropagation();
      send({ type: "key", key: event.key as NavKey });
      return;
    }
    // In an open menu a bare mnemonic letter switches menus (bar only).
    if (!event.ctrlKey && !event.metaKey && event.key.length === 1) {
      const index = mnemonicIndex(menus, event.key);
      if (index >= 0) {
        event.preventDefault();
        event.stopPropagation();
        send({ type: "mnemonic", index });
      }
    }
  }

  /** Something modal is up: the menu bar's keys stand down. */
  function blocked(): boolean {
    return (
      dialog.request !== null || palette.open || switcher.open || slides.presenting
    );
  }

  function onWindowKeydown(event: KeyboardEvent): void {
    if (event.key === "Alt") {
      if (!blocked()) send({ type: "alt", down: true });
      return;
    }
    if (blocked() || event.defaultPrevented) return;
    if (
      event.key === "F10" &&
      !event.altKey &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.shiftKey
    ) {
      event.preventDefault();
      send({ type: "f10" });
      return;
    }
    if (event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
      const index = event.key.length === 1 ? mnemonicIndex(menus, event.key) : -1;
      if (index < 0) return;
      event.preventDefault();
      event.stopPropagation();
      send({ type: "mnemonic", index });
    }
  }

  function onWindowKeyup(event: KeyboardEvent): void {
    if (event.key === "Alt") send({ type: "alt", down: false });
  }

  function onWindowPointer(event: PointerEvent): void {
    if (nav.mode === "closed") return;
    if (event.target instanceof Node && root?.contains(event.target) === true) return;
    send({ type: "outside" });
  }

  function onWindowBlur(): void {
    if (nav.mnemonics && nav.mode === "closed") send({ type: "alt", down: false });
  }

  const expanded = (i: number): boolean =>
    nav.mode === "open" && nav.active === i && (!nav.folded || nav.submenu);
</script>

<svelte:window
  onkeydowncapture={onWindowKeydown}
  onkeyup={onWindowKeyup}
  onpointerdowncapture={onWindowPointer}
  onblur={onWindowBlur}
/>

{#snippet label(menu: Menu)}
  {#if nav.mnemonics && menu.mnemonicIndex >= 0}
    {menu.label.slice(0, menu.mnemonicIndex)}<span class="underline underline-offset-2"
      >{menu.label[menu.mnemonicIndex]}</span
    >{menu.label.slice(menu.mnemonicIndex + 1)}
  {:else}
    {menu.label}
  {/if}
{/snippet}

<div
  bind:this={root}
  data-menu-root
  class="relative flex h-full shrink-0 items-stretch"
>
  {#if folded}
    <div
      role="menubar"
      aria-label="Application menu"
      tabindex="-1"
      class="flex"
      {onkeydown}
    >
      <button
        type="button"
        role="menuitem"
        aria-haspopup="menu"
        aria-expanded={nav.mode === "open"}
        aria-controls="menu-folded-list"
        tabindex="0"
        data-menu-fold
        class="inset-focus flex h-full items-center px-2.5 text-[13px] text-text-60 hover:bg-surface-hi hover:text-text {nav.mode ===
        'open'
          ? 'bg-surface-hi text-text'
          : ''}"
        onclick={() => send({ type: "click-fold" })}
      >
        Menu
      </button>
      {#if nav.mode === "open"}
        <div
          id="menu-folded-list"
          role="menu"
          aria-label="Menus"
          class="absolute top-full left-0 z-50 min-w-[180px] border border-line bg-surface py-1"
        >
          {#each menus as menu, i (menu.id)}
            <div class="relative">
              <button
                type="button"
                role="menuitem"
                aria-haspopup="menu"
                aria-expanded={expanded(i)}
                tabindex={nav.active === i ? 0 : -1}
                data-menu-row={i}
                data-menu={menu.id}
                class="inset-focus flex h-7 w-full items-center justify-between gap-6 px-3 text-left text-[13px] text-text {nav.active ===
                i
                  ? 'bg-surface-hi'
                  : ''}"
                onclick={() => send({ type: "click-bar", index: i })}
                onpointerenter={() => send({ type: "hover-bar", index: i })}
              >
                <span>{@render label(menu)}</span>
                <span aria-hidden="true" class="text-text-muted">›</span>
              </button>
              {#if expanded(i)}
                <MenuPanel
                  {menu}
                  index={i}
                  highlighted={nav.item}
                  keyboard={nav.mnemonics}
                  placement="side"
                  onhover={(item) => send({ type: "hover-item", index: item })}
                  onrun={(item) => send({ type: "click-item", index: item })}
                />
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {:else}
    <div
      role="menubar"
      aria-label="Application menu"
      tabindex="-1"
      class="flex items-stretch"
      {onkeydown}
    >
      {#each menus as menu, i (menu.id)}
        <div class="relative flex">
          <button
            type="button"
            role="menuitem"
            id="menubar-{menu.id}"
            aria-haspopup="menu"
            aria-expanded={expanded(i)}
            aria-controls={expanded(i) ? `menu-${menu.id}` : undefined}
            aria-keyshortcuts="Alt+{menu.mnemonic.toUpperCase()}"
            tabindex={nav.active === i ? 0 : -1}
            data-menu-bar={i}
            data-menu={menu.id}
            class="inset-focus flex h-full items-center px-2 text-[13px] hover:bg-surface-hi hover:text-text {expanded(
              i,
            ) ||
            (nav.mode === 'bar' && nav.active === i)
              ? 'bg-surface-hi text-text'
              : 'text-text-60'}"
            onclick={() => send({ type: "click-bar", index: i })}
            onpointerenter={() => send({ type: "hover-bar", index: i })}
          >
            {@render label(menu)}
          </button>
          {#if expanded(i)}
            <MenuPanel
              {menu}
              index={i}
              highlighted={nav.item}
              keyboard={nav.mnemonics}
              placement="below"
              onhover={(item) => send({ type: "hover-item", index: item })}
              onrun={(item) => send({ type: "click-item", index: item })}
            />
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
