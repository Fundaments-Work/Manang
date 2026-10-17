<script lang="ts">
  import { onMount } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";
  import type { ProjectEntry } from "$lib/kernel/types";
  import { home } from "$lib/state/home.svelte";
  import { loadThumbnail } from "$lib/state/thumbCache";
  import { ENGINE_LABEL, relativeTime } from "$lib/workbench/home";

  /**
   * One project, as a card in the grid or a row in the list. The main button
   * opens it; pin and the menu are separate buttons beside it (a button cannot
   * hold buttons). Right-click, the Menu key or Shift+F10 open the same menu.
   * The thumbnail loads only once the card is on screen.
   */
  interface Props {
    entry: ProjectEntry;
    view: "grid" | "list";
    current: boolean;
    now: number;
    onopen: () => void;
    onmenu: (x: number, y: number) => void;
    onpin: () => void;
  }

  const { entry, view, current, now, onopen, onmenu, onpin }: Props = $props();

  let root: HTMLElement | undefined = $state();
  let visible = $state(false);
  let url = $state<string | null>(null);

  const version = $derived(home.thumbVersions.get(entry.path) ?? 0);
  const when = $derived(relativeTime(entry.last_opened, now));

  onMount(() => {
    if (root === undefined || typeof IntersectionObserver === "undefined") {
      visible = true;
      return;
    }
    const observer = new IntersectionObserver((hits) => {
      if (hits.some((h) => h.isIntersecting)) {
        visible = true;
        observer.disconnect();
      }
    });
    observer.observe(root);
    return () => observer.disconnect();
  });

  $effect(() => {
    if (!visible || !entry.has_thumbnail || !entry.exists) {
      url = null;
      return;
    }
    let cancelled = false;
    void loadThumbnail(entry.path, version).then((loaded) => {
      if (!cancelled) url = loaded;
    });
    return () => {
      cancelled = true;
    };
  });

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) {
      event.preventDefault();
      const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
      onmenu(box.left + 24, box.top + 24);
    }
  }

  function oncontextmenu(event: MouseEvent): void {
    event.preventDefault();
    onmenu(event.clientX, event.clientY);
  }

  function menuFromButton(event: MouseEvent): void {
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    onmenu(box.left, box.bottom + 2);
  }
</script>

{#snippet placeholder()}
  <div
    class="flex h-full w-full flex-col justify-between bg-surface-hi p-3"
    aria-hidden="true"
  >
    <span class="font-mono text-[11px] text-text-muted"
      >{ENGINE_LABEL[entry.engine]}</span
    >
    <div class="flex flex-col gap-1.5">
      <span class="block h-px w-3/5 bg-line"></span>
      <span class="block h-px w-4/5 bg-line"></span>
      <span class="block h-px w-2/5 bg-line"></span>
    </div>
  </div>
{/snippet}

{#snippet pinButton()}
  <button
    type="button"
    class="mng-icon-btn inset-focus {entry.pinned
      ? 'bg-surface text-text'
      : 'text-text-muted opacity-0 group-focus-within:opacity-100 group-hover:opacity-100'}"
    aria-pressed={entry.pinned}
    aria-label="{entry.pinned ? 'Unpin' : 'Pin'} {entry.name}"
    title={entry.pinned ? "Unpin" : "Pin"}
    onclick={onpin}
  >
    <Icon name={entry.pinned ? "pin-filled" : "pin"} size={14} />
  </button>
{/snippet}

{#snippet menuButton()}
  <button
    type="button"
    class="mng-icon-btn inset-focus text-text-muted opacity-0 group-focus-within:opacity-100 group-hover:opacity-100"
    aria-haspopup="menu"
    aria-label="Actions for {entry.name}"
    title="Actions"
    onclick={menuFromButton}
  >
    <Icon name="more" size={14} />
  </button>
{/snippet}

{#if view === "grid"}
  <article
    bind:this={root}
    class="group relative flex flex-col border border-line bg-surface hover:bg-surface-hi"
    data-project={entry.path}
    aria-label={entry.name}
  >
    <button
      type="button"
      class="inset-focus flex flex-col text-left disabled:cursor-default"
      disabled={!entry.exists}
      aria-current={current ? "true" : undefined}
      {onkeydown}
      {oncontextmenu}
      onclick={onopen}
    >
      <span
        class="block h-36 w-full overflow-hidden border-b border-line bg-surface-hi"
      >
        {#if url !== null}
          <img
            src={url}
            alt="First page of {entry.name}"
            class="h-full w-full object-cover object-top"
            style="background: var(--mng-paper)"
            draggable="false"
          />
        {:else}
          {@render placeholder()}
        {/if}
      </span>
      <span class="flex min-w-0 flex-col gap-1 px-3 pt-2.5 pb-3">
        <span class="truncate text-[13px] text-text">{entry.name}</span>
        <span class="flex items-center gap-2 text-[11px] text-text-60">
          <span>{ENGINE_LABEL[entry.engine]}</span>
          <span aria-hidden="true" class="text-text-muted">/</span>
          {#if entry.exists}
            <span class="truncate">{current ? "Open now" : when}</span>
          {:else}
            <span class="truncate text-text-muted">Folder not found</span>
          {/if}
        </span>
        {#if entry.tags.length > 0}
          <span class="flex flex-wrap gap-1 pt-0.5">
            {#each entry.tags.slice(0, 3) as tag (tag)}
              <span
                class="border border-line px-1.5 text-[11px] leading-[18px] text-text-60"
                >{tag}</span
              >
            {/each}
            {#if entry.tags.length > 3}
              <span class="text-[11px] leading-[18px] text-text-muted"
                >+{entry.tags.length - 3}</span
              >
            {/if}
          </span>
        {/if}
      </span>
    </button>
    <div
      class="absolute top-1.5 right-1.5 flex group-focus-within:bg-surface group-hover:bg-surface"
    >
      {@render pinButton()}
      {@render menuButton()}
    </div>
  </article>
{:else}
  <div
    bind:this={root}
    class="group relative flex items-center border-b border-line hover:bg-surface"
    data-project={entry.path}
  >
    <button
      type="button"
      class="inset-focus flex min-w-0 flex-1 items-center gap-4 py-2 pr-2 pl-3 text-left disabled:cursor-default"
      disabled={!entry.exists}
      aria-current={current ? "true" : undefined}
      {onkeydown}
      {oncontextmenu}
      onclick={onopen}
    >
      <span class="block h-9 w-7 shrink-0 overflow-hidden border border-line">
        {#if url !== null}
          <img
            src={url}
            alt=""
            class="h-full w-full object-cover object-top"
            style="background: var(--mng-paper)"
            draggable="false"
          />
        {:else}
          <span class="block h-full w-full bg-surface-hi"></span>
        {/if}
      </span>
      <span
        class="w-[200px] shrink-0 truncate text-[13px] text-text max-[800px]:w-auto max-[800px]:flex-1"
        >{entry.name}</span
      >
      <span class="w-16 shrink-0 text-[11px] text-text-60 max-[800px]:hidden"
        >{ENGINE_LABEL[entry.engine]}</span
      >
      <span
        class="min-w-0 flex-1 truncate font-mono text-[11px] text-text-muted max-[800px]:hidden"
        >{entry.path}</span
      >
      {#if entry.tags.length > 0}
        <span class="shrink-0 truncate text-[11px] text-text-60 max-[1000px]:hidden"
          >{entry.tags.join(", ")}</span
        >
      {/if}
      <span
        class="w-28 shrink-0 text-right text-[11px] text-text-60 max-[800px]:hidden"
      >
        {#if entry.exists}{current ? "Open now" : when}{:else}Folder not found{/if}
      </span>
    </button>
    <div class="flex shrink-0 pr-1.5">
      {@render pinButton()}
      {@render menuButton()}
    </div>
  </div>
{/if}
