<script lang="ts">
  import ContextMenu, { type MenuItem } from "$lib/components/ui/ContextMenu.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { formatShortcut } from "$lib/commands/shortcut";
  import type { PdfView } from "$lib/state/pdfView.svelte";
  import { detectPlatform } from "$lib/util/keyboard";
  import { parsePageInput } from "$lib/workbench/pdf/navigation";
  import {
    MAX_ZOOM,
    MIN_ZOOM,
    ZOOM_MENU,
    isCurrentMode,
    zoomLabel,
    zoomOf,
    zoomPercent,
  } from "$lib/workbench/pdf/zoom";

  /**
   * The PDF controls of the preview header, in one row: page box, zoom
   * (−, a value that opens the presets, +), find, outline and dark pages.
   * Items give way as the pane narrows (container queries on the header), the
   * least used first; the keyboard reaches all of them anyway.
   */
  interface Props {
    view: PdfView;
    /** The pages count, when the viewer has not reported it yet. */
    pages: number;
    ondarkpages: (dark: boolean) => void;
  }

  const { view, pages, ondarkpages }: Props = $props();

  const platform = detectPlatform();
  const keys = {
    in: formatShortcut({ key: "=", mod: true }, platform),
    out: formatShortcut({ key: "-", mod: true }, platform),
    fit: formatShortcut({ key: "0", mod: true }, platform),
    find: formatShortcut({ key: "f", mod: true }, platform),
  };

  let typed = $state<string | null>(null);
  const shownPage = $derived(typed ?? (pages > 0 ? String(view.page + 1) : ""));

  function commitPage(input: HTMLInputElement): void {
    const page = parsePageInput(input.value, pages);
    typed = null;
    if (page !== null) view.go(page);
  }

  function onPageKey(event: KeyboardEvent): void {
    const input = event.currentTarget as HTMLInputElement;
    if (event.key === "Enter") {
      commitPage(input);
      input.select();
    } else if (event.key === "Escape") {
      typed = null;
      input.blur();
    } else return;
    event.preventDefault();
    event.stopPropagation();
  }

  let menu = $state<{ x: number; y: number } | null>(null);
  let zoomButton: HTMLButtonElement | undefined = $state();

  const zoomValue = $derived(zoomOf(view.scale));
  const menuItems: MenuItem[] = $derived([
    {
      label: "Zoom in",
      shortcut: keys.in,
      disabled: zoomValue >= MAX_ZOOM - 0.001,
      onselect: () => view.zoomIn(),
    },
    {
      label: "Zoom out",
      shortcut: keys.out,
      disabled: zoomValue <= MIN_ZOOM + 0.001,
      onselect: () => view.zoomOut(),
    },
    ...ZOOM_MENU.map((entry, i) => ({
      label: entry.label,
      ...(entry.mode === "page-width" ? { shortcut: keys.fit } : {}),
      separated: i === 0 || i === 2,
      checked: isCurrentMode(entry.mode, view.zoom),
      onselect: () => view.setZoom(entry.mode),
    })),
  ]);

  function openMenu(): void {
    const rect = zoomButton?.getBoundingClientRect();
    if (rect === undefined) return;
    menu = { x: rect.left, y: rect.bottom + 2 };
  }
</script>

<button
  type="button"
  class="mng-icon-btn @max-[520px]:hidden"
  aria-label="Previous page"
  disabled={view.page <= 0 || pages === 0}
  onclick={() => view.go(view.page - 1)}
>
  <Icon name="arrowUp" size={14} />
</button>
<button
  type="button"
  class="mng-icon-btn @max-[520px]:hidden"
  aria-label="Next page"
  disabled={view.page >= pages - 1}
  onclick={() => view.go(view.page + 1)}
>
  <Icon name="arrowDown" size={14} />
</button>
<label
  class="flex shrink-0 items-center gap-1 px-1 font-mono text-[11px] whitespace-nowrap text-text-60 @max-[200px]:hidden"
  title="Type a page number and press Enter"
>
  <span class="sr-only">Page</span>
  <input
    class="h-6 w-9 border border-line bg-transparent px-1 text-right font-mono text-[11px] text-text outline-none focus:border-text-60"
    type="text"
    inputmode="numeric"
    value={shownPage}
    aria-label="Page number, of {pages}"
    data-page-input
    disabled={pages === 0}
    oninput={(event) => (typed = (event.currentTarget as HTMLInputElement).value)}
    onkeydown={onPageKey}
    onfocus={(event) => (event.currentTarget as HTMLInputElement).select()}
    onblur={() => (typed = null)}
  />
  <span aria-hidden="true">/ {pages}</span>
</label>

<span class="mx-1 h-4 w-px shrink-0 bg-line @max-[230px]:hidden" aria-hidden="true"
></span>

<button
  type="button"
  class="mng-icon-btn @max-[400px]:hidden"
  aria-label="Zoom out"
  title="Zoom out ({keys.out})"
  disabled={zoomValue <= MIN_ZOOM + 0.001}
  onclick={() => view.zoomOut()}
>
  <Icon name="clear" size={14} />
</button>
<button
  bind:this={zoomButton}
  type="button"
  class="mng-btn h-6 min-w-12 shrink-0 justify-center border-transparent bg-transparent px-1.5 font-mono text-[11px] text-text-60 @max-[230px]:hidden"
  aria-label="Zoom {zoomLabel(view.zoom, view.scale)}. Choose a zoom"
  aria-haspopup="menu"
  aria-expanded={menu !== null}
  title="Zoom presets"
  data-zoom-menu
  onclick={openMenu}
  onkeydown={(event) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      openMenu();
    }
  }}
>
  {zoomPercent(view.scale)}
</button>
<button
  type="button"
  class="mng-icon-btn @max-[400px]:hidden"
  aria-label="Zoom in"
  title="Zoom in ({keys.in})"
  disabled={zoomValue >= MAX_ZOOM - 0.001}
  onclick={() => view.zoomIn()}
>
  <Icon name="plus" size={14} />
</button>

<span class="mx-1 h-4 w-px shrink-0 bg-line @max-[260px]:hidden" aria-hidden="true"
></span>

<button
  type="button"
  class="mng-icon-btn @max-[260px]:hidden"
  aria-label="Find in PDF"
  title="Find in PDF ({keys.find})"
  aria-pressed={view.findOpen}
  aria-keyshortcuts="Control+F"
  onclick={() => (view.findOpen ? view.closeFind() : view.openFind())}
>
  <Icon name="search" size={14} />
</button>
{#if view.outline.length > 0}
  <button
    type="button"
    class="mng-icon-btn @max-[340px]:hidden"
    aria-label="PDF outline"
    title="Outline"
    aria-pressed={view.outlineOpen}
    onclick={() => view.toggleOutline()}
  >
    <Icon name="outline" size={14} />
  </button>
{/if}
<button
  type="button"
  class="mng-icon-btn @max-[460px]:hidden"
  aria-label="Dark pages"
  title="Dark pages: invert the pages for reading at night"
  aria-pressed={view.darkPages}
  onclick={() => ondarkpages(!view.darkPages)}
>
  <Icon name="invert" size={14} />
</button>

{#if menu !== null}
  <ContextMenu
    x={menu.x}
    y={menu.y}
    label="Zoom"
    items={menuItems}
    onclose={() => (menu = null)}
  />
{/if}
