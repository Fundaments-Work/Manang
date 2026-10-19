<script lang="ts">
  import { onMount, tick, untrack } from "svelte";

  import { keymap } from "$lib/commands/keymap.svelte";
  import { formatShortcut } from "$lib/commands/shortcut";
  import SharePopover from "$lib/components/SharePopover.svelte";
  import ContextMenu, { type MenuItem } from "$lib/components/ui/ContextMenu.svelte";
  import { SLOT_COLOURS, SLOT_COUNT, SLOT_STROKES } from "$lib/editor/live/remote";
  import { live } from "$lib/state/live.svelte";
  import { share } from "$lib/state/share.svelte";
  import { sharePopover } from "$lib/state/sharePopover.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { initials } from "$lib/workbench/live";
  import { detectPlatform } from "$lib/util/keyboard";
  import Icon from "$lib/components/ui/Icon.svelte";
  import {
    closeOtherTabs,
    closeTab,
    closeTabsToRight,
    isTabDirty,
    toggleSlides,
  } from "$lib/state/actions";
  import { build } from "$lib/state/build.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { visualMode } from "$lib/state/visual.svelte";
  import { pluginIdOf } from "$lib/workbench/market";
  import { engineOf } from "$lib/workbench/paths";
  import { tabLabels, type Tab } from "$lib/workbench/tabs";

  /**
   * The tab strip: WAI-ARIA tabs with automatic activation (every tab's content
   * is already in memory, so switching has no latency). Left and Right move and
   * activate, Home and End jump, Delete closes, Shift+F10 opens the menu. Tabs
   * can be closed with the middle button and reordered by dragging; the strip
   * scrolls sideways when there are more tabs than fit.
   */

  let strip: HTMLElement | undefined = $state();
  let menu = $state<{ x: number; y: number; tab: Tab } | null>(null);
  let dragId = $state<string | null>(null);
  let dropAt = $state<number | null>(null);
  let shareButton: HTMLButtonElement | undefined = $state();
  const platform = detectPlatform();
  // The one lime action of the view, only while updates are waiting (and not
  // while the Versions view, which has its own lime Get updates, is showing).
  const pending = $derived(
    share.updatesWaiting && layout.sideView !== "source-control",
  );
  const shareKey = $derived.by(() => {
    const first = keymap.shortcuts("share.open")[0];
    return first === undefined ? "" : formatShortcut(first, platform);
  });

  // Presence: at most four marks, the rest folded into "+N" (names in its tooltip).
  const maxMarks = $derived(layout.narrow ? 2 : 4);
  const shownPeers = $derived(live.peers.slice(0, maxMarks));
  const hiddenPeers = $derived(live.peers.slice(maxMarks));
  const slotOf = (n: number): number => ((n % SLOT_COUNT) + SLOT_COUNT) % SLOT_COUNT;

  const labels = $derived(tabLabels(tabs.tabs));
  const visualAvailable = $derived(
    tabs.active?.kind === "document" &&
      tabs.active.view === "text" &&
      visualMode.available(tabs.active.path),
  );
  const visualOn = $derived(
    visualAvailable && tabs.active !== undefined && visualMode.isOn(tabs.active.path),
  );
  const visualKey = $derived.by(() => {
    const first = keymap.shortcuts("view.visual")[0];
    return first === undefined ? "" : formatShortcut(first, platform);
  });
  const withKey = (label: string): string =>
    visualKey === "" ? label : `${label} (${visualKey})`;
  const activeTypst = $derived(
    tabs.active?.kind === "document" && engineOf(tabs.active.path) === "typst",
  );
  // The way back to a closed preview: only for a document that has one (its
  // own, or the project main it belongs to), shown as text, while the editor
  // stands alone. The preview target is the one the preview pane would show.
  const previewClosed = $derived(
    tabs.active?.kind === "document" &&
      tabs.active.view === "text" &&
      layout.effectivePreview === "editor" &&
      build.previewTarget() !== null,
  );

  // Learn whether updates are waiting, so the Share button can say so.
  $effect(() => {
    const root = workspace.project?.root ?? "";
    untrack(() => void share.load(root));
  });
  onMount(() => share.listen());

  // Keep the active tab visible when it changes, including after a keyboard jump.
  $effect(() => {
    const id = tabs.active?.id;
    if (id === undefined || strip === undefined) return;
    strip
      .querySelector<HTMLElement>(`[data-tab="${CSS.escape(id)}"]`)
      ?.scrollIntoView({ inline: "nearest", block: "nearest" });
  });

  async function focusTab(id: string): Promise<void> {
    await tick();
    strip?.querySelector<HTMLElement>(`[data-tab="${CSS.escape(id)}"]`)?.focus();
  }

  function onkeydown(event: KeyboardEvent, tab: Tab): void {
    const list = tabs.tabs;
    const at = list.findIndex((t) => t.id === tab.id);
    let target: Tab | undefined;
    switch (event.key) {
      case "ArrowRight":
        target = list[(at + 1) % list.length];
        break;
      case "ArrowLeft":
        target = list[(at - 1 + list.length) % list.length];
        break;
      case "Home":
        target = list[0];
        break;
      case "End":
        target = list[list.length - 1];
        break;
      case "Delete":
        event.preventDefault();
        void closeTab(tab.id).then(() => {
          const next = tabs.active;
          if (next !== undefined) void focusTab(next.id);
        });
        return;
      case "Enter":
      case " ":
        event.preventDefault();
        tabs.activate(tab.id);
        return;
      case "ContextMenu":
        event.preventDefault();
        openMenu(
          tab,
          (event.currentTarget as HTMLElement).getBoundingClientRect().left,
          (event.currentTarget as HTMLElement).getBoundingClientRect().bottom,
        );
        return;
      case "F10":
        if (!event.shiftKey) return;
        event.preventDefault();
        openMenu(
          tab,
          (event.currentTarget as HTMLElement).getBoundingClientRect().left,
          (event.currentTarget as HTMLElement).getBoundingClientRect().bottom,
        );
        return;
      default:
        return;
    }
    event.preventDefault();
    if (target !== undefined) {
      tabs.activate(target.id);
      void focusTab(target.id);
    }
  }

  function openMenu(tab: Tab, x: number, y: number): void {
    menu = { x, y, tab };
  }

  function menuItems(tab: Tab): MenuItem[] {
    const items: MenuItem[] = [
      { label: "Close", shortcut: "Ctrl+W", onselect: () => void closeTab(tab.id) },
      {
        label: "Close others",
        disabled: tabs.tabs.length < 2,
        onselect: () => void closeOtherTabs(tab.id),
      },
      {
        label: "Close to the right",
        disabled: tabs.tabs.findIndex((t) => t.id === tab.id) === tabs.tabs.length - 1,
        onselect: () => void closeTabsToRight(tab.id),
      },
    ];
    if (tab.kind === "document" && engineOf(tab.path) === "typst") {
      items.push({
        label: tab.view === "slides" ? "Show as text" : "Show as slides",
        separated: true,
        onselect: () => {
          tabs.activate(tab.id);
          toggleSlides();
        },
      });
    }
    items.push({
      label: tab.kind === "plugin" ? "Copy plugin id" : "Copy path",
      separated: true,
      onselect: () =>
        void navigator.clipboard
          ?.writeText(tab.kind === "plugin" ? pluginIdOf(tab.path) : tab.path)
          .catch(() => undefined),
    });
    return items;
  }

  function onwheel(event: WheelEvent): void {
    if (strip === undefined || Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
    strip.scrollLeft += event.deltaY;
  }

  function dropIndex(event: DragEvent, index: number): number {
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    return event.clientX > box.left + box.width / 2 ? index + 1 : index;
  }
</script>

<div class="flex h-9 shrink-0 items-stretch border-b border-line bg-surface">
  <div
    bind:this={strip}
    role="tablist"
    aria-label="Open files"
    class="no-scrollbar flex min-w-0 flex-1 overflow-x-auto"
    {onwheel}
  >
    {#each tabs.tabs as tab, index (tab.id)}
      {@const label = labels.get(tab.id)}
      {@const active = tab.id === tabs.active?.id}
      {@const dirty = isTabDirty(tab.path)}
      <div
        role="presentation"
        class="group relative flex shrink-0 items-stretch border-r border-line {active
          ? 'bg-bg'
          : 'bg-surface hover:bg-surface-hi'}"
        draggable="true"
        ondragstart={(event) => {
          dragId = tab.id;
          event.dataTransfer?.setData("text/plain", tab.path);
        }}
        ondragover={(event) => {
          if (dragId === null) return;
          event.preventDefault();
          dropAt = dropIndex(event, index);
        }}
        ondrop={(event) => {
          event.preventDefault();
          if (dragId !== null && dropAt !== null) {
            const from = tabs.tabs.findIndex((t) => t.id === dragId);
            tabs.move(dragId, dropAt > from ? dropAt - 1 : dropAt);
          }
          dragId = null;
          dropAt = null;
        }}
        ondragend={() => {
          dragId = null;
          dropAt = null;
        }}
      >
        {#if active}
          <span class="absolute inset-x-0 top-0 h-0.5 bg-accent" aria-hidden="true"
          ></span>
        {/if}
        {#if dropAt === index && dragId !== null && dragId !== tab.id}
          <span class="absolute inset-y-0 left-0 w-px bg-text" aria-hidden="true"
          ></span>
        {/if}
        <div
          role="tab"
          id="tab-{index}"
          data-tab={tab.id}
          aria-selected={active}
          aria-controls="editor-area"
          aria-label="{label?.name ?? tab.path}{dirty ? ', unsaved changes' : ''}"
          tabindex={active ? 0 : -1}
          class="inset-focus flex max-w-56 min-w-0 cursor-default items-center gap-1.5 pr-1 pl-3 text-xs {active
            ? 'text-text'
            : 'text-text-60 group-hover:text-text'}"
          title={tab.path}
          onclick={() => tabs.activate(tab.id)}
          onkeydown={(event) => onkeydown(event, tab)}
          onauxclick={(event) => {
            if (event.button === 1) {
              event.preventDefault();
              void closeTab(tab.id);
            }
          }}
          onmousedown={(event) => {
            // Middle-click would otherwise start autoscroll, or on Linux paste
            // the primary selection into the focused editor.
            if (event.button === 1) event.preventDefault();
          }}
          onmouseup={(event) => {
            if (event.button === 1) event.preventDefault();
          }}
          oncontextmenu={(event) => {
            event.preventDefault();
            openMenu(tab, event.clientX, event.clientY);
          }}
        >
          {#if tab.kind === "figure"}
            <Icon name="figures" size={13} class="shrink-0 opacity-70" />
          {:else if tab.kind === "plugin"}
            <Icon name="plugins" size={13} class="shrink-0 opacity-70" />
          {:else if tab.view === "slides"}
            <Icon name="slides" size={13} class="shrink-0 opacity-70" />
          {/if}
          <span class="truncate">{label?.name ?? tab.path}</span>
          {#if label?.hint !== undefined && label.hint !== ""}
            <span class="truncate text-text-muted">{label.hint}</span>
          {/if}
        </div>
        <button
          type="button"
          tabindex="-1"
          class="mng-icon-btn h-full w-7 {active || dirty
            ? ''
            : 'opacity-0 group-hover:opacity-100'} focus-visible:opacity-100"
          aria-label="Close {label?.name ?? tab.path}"
          title="Close"
          onclick={() => void closeTab(tab.id)}
        >
          {#if dirty}
            <!-- Unsaved: a small square that becomes the close mark on hover. -->
            <span class="h-1.5 w-1.5 bg-text group-hover:hidden" aria-hidden="true"
            ></span>
            <Icon name="close" size={12} class="hidden group-hover:block" />
          {:else}
            <Icon name="close" size={12} />
          {/if}
        </button>
      </div>
    {/each}
  </div>

  {#if live.active}
    <ul
      class="m-0 flex shrink-0 list-none items-center gap-1 border-l border-line px-2"
      aria-label="People in this session"
      data-testid="presence"
    >
      {#each shownPeers as peer (peer.peer)}
        {@const n = slotOf(peer.slot)}
        <li
          class="inline-flex h-5 min-w-5 items-center justify-center border border-line px-0.5 text-[10px] text-text"
          style="border-bottom: 3px {SLOT_STROKES[n]} {SLOT_COLOURS[n]}"
          title={peer.name}
          aria-label={peer.name}
          data-peer={peer.peer}
        >
          {initials(peer.name)}
        </li>
      {/each}
      {#if hiddenPeers.length > 0}
        <li
          class="inline-flex h-5 min-w-5 items-center justify-center border border-line px-0.5 text-[10px] text-text-60"
          title={hiddenPeers.map((p) => p.name).join(", ")}
          aria-label="{hiddenPeers.length} more: {hiddenPeers
            .map((p) => p.name)
            .join(', ')}"
        >
          +{hiddenPeers.length}
        </li>
      {/if}
      {#if live.peers.length === 0}
        <li class="px-1 text-[11px] text-text-muted max-md:hidden">Only you</li>
      {/if}
    </ul>
  {/if}

  {#if visualAvailable && tabs.active !== undefined}
    {@const path = tabs.active.path}
    <div
      role="radiogroup"
      aria-label="Editing mode"
      tabindex="0"
      data-testid="mode-toggle"
      class="flex shrink-0 items-center gap-px border-l border-line px-2 focus-visible:outline-none"
      onkeydown={(e) => {
        if (e.key === "ArrowRight" || e.key === "ArrowDown") {
          e.preventDefault();
          if (!visualOn) void visualMode.set(path, true);
        } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
          e.preventDefault();
          if (visualOn) void visualMode.set(path, false);
        }
      }}
    >
      <button
        type="button"
        role="radio"
        aria-checked={!visualOn}
        aria-keyshortcuts={visualKey}
        tabindex={!visualOn ? 0 : -1}
        data-testid="mode-source"
        title={withKey("Source")}
        class="mng-btn inset-focus h-6 px-2"
        onclick={() => visualOn && void visualMode.set(path, false)}>Source</button
      >
      <button
        type="button"
        role="radio"
        aria-checked={visualOn}
        aria-keyshortcuts={visualKey}
        tabindex={visualOn ? 0 : -1}
        data-testid="mode-visual"
        title={withKey("Visual")}
        class="mng-btn inset-focus h-6 px-2"
        onclick={() => !visualOn && void visualMode.set(path, true)}>Visual</button
      >
    </div>
  {/if}

  {#if activeTypst && tabs.active !== undefined}
    {@const slidesOn = tabs.active.view === "slides"}
    <div
      role="radiogroup"
      aria-label="Document view"
      tabindex="0"
      class="flex shrink-0 items-center gap-px border-l border-line px-2 focus-visible:outline-none"
      onkeydown={(e) => {
        if (e.key === "ArrowRight" || e.key === "ArrowDown") {
          e.preventDefault();
          if (!slidesOn) toggleSlides();
        } else if (e.key === "ArrowLeft" || e.key === "ArrowUp") {
          e.preventDefault();
          if (slidesOn) toggleSlides();
        }
      }}
    >
      <button
        type="button"
        role="radio"
        aria-checked={!slidesOn}
        tabindex={!slidesOn ? 0 : -1}
        class="mng-btn inset-focus h-6 px-2"
        onclick={() => slidesOn && toggleSlides()}>Text</button
      >
      <button
        type="button"
        role="radio"
        aria-checked={slidesOn}
        tabindex={slidesOn ? 0 : -1}
        class="mng-btn inset-focus h-6 px-2"
        onclick={() => !slidesOn && toggleSlides()}>Slides</button
      >
    </div>
  {/if}

  {#if previewClosed}
    <div class="flex shrink-0 items-center border-l border-line px-1">
      <button
        type="button"
        class="mng-icon-btn"
        aria-label="Show preview"
        title="Show preview"
        onclick={() => layout.showPreview()}
      >
        <Icon name={layout.narrow ? "preview" : "split"} size={14} />
      </button>
    </div>
  {/if}

  <div class="relative flex shrink-0 items-center border-l border-line px-1">
    <button
      bind:this={shareButton}
      type="button"
      class="mng-lime-hover inset-focus inline-flex h-7 items-center gap-1.5 px-2 text-xs {sharePopover.open
        ? 'bg-surface-hi text-text'
        : pending
          ? 'border border-accent bg-accent text-accent-ink'
          : 'text-text-60'}"
      data-pending={pending}
      data-testid="share-button"
      aria-haspopup="true"
      aria-expanded={sharePopover.open}
      aria-controls="share-popover"
      aria-keyshortcuts={shareKey}
      title={shareKey === "" ? "Share" : `Share (${shareKey})`}
      onclick={() => sharePopover.toggle()}
    >
      <Icon name="share" size={14} />
      <span class="max-md:sr-only">Share</span>
    </button>
    <SharePopover anchor={shareButton} />
  </div>
</div>

{#if menu !== null}
  <ContextMenu
    x={menu.x}
    y={menu.y}
    label="Tab actions"
    items={menuItems(menu.tab)}
    onclose={() => (menu = null)}
  />
{/if}
