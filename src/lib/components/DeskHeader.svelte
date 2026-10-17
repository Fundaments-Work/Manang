<script lang="ts">
  import HairButton from "./ui/HairButton.svelte";
  import Mark from "./Mark.svelte";
  import { desk } from "$lib/state/desk.svelte";
  import { document } from "$lib/state/document.svelte";

  /**
   * Desk header — DESIGN.md §3.1.
   *
   * h-10, px-4, 1px hairline along the bottom. Mark and wordmark left,
   * breadcrumb centre, responsive actions right.
   */
  const parts = $derived(document.meta.path.split("/"));
</script>

<header class="flex h-10 shrink-0 items-center gap-4 border-b border-line px-4">
  <div class="flex items-center gap-2">
    <Mark class="text-text" />
    <span
      class="font-sans text-[15px] font-light text-text"
      style="letter-spacing: var(--tracking-headline)"
    >
      Manang
    </span>
  </div>

  <nav aria-label="Breadcrumb" class="min-w-0 flex-1">
    <ol class="flex items-center gap-1.5 font-mono text-xs text-text-60">
      {#each parts as part, index (part)}
        <li class="flex min-w-0 items-center gap-1.5">
          {#if index > 0}
            <span aria-hidden="true" class="text-text-muted">/</span>
          {/if}
          <span class="truncate" class:text-text={index === parts.length - 1}>
            {part}
          </span>
        </li>
      {/each}
    </ol>
  </nav>

  <div class="flex items-center gap-1.5">
    <HairButton
      label="Library"
      pressed={desk.libraryOpen}
      onactivate={() => (desk.libraryOpen = !desk.libraryOpen)}
      icon={ShelfIcon}
    />
    <HairButton
      label="Draw"
      pressed={desk.canvasOpen}
      onactivate={desk.toggleCanvas}
      icon={PenIcon}
    />
    <HairButton
      label="Slides"
      pressed={desk.slideMode}
      onactivate={() => (desk.slideMode = !desk.slideMode)}
      icon={SlidesIcon}
    />
    <HairButton
      label="Split"
      pressed={desk.pane === "split"}
      onactivate={desk.togglePane}
      icon={SplitIcon}
    />
    <HairButton
      label="Zen"
      pressed={desk.zen}
      onactivate={desk.toggleZen}
      icon={ZenIcon}
    />
  </div>
</header>

{#snippet ShelfIcon()}
  <svg
    viewBox="0 0 16 16"
    width="12"
    height="12"
    fill="none"
    stroke="currentColor"
    stroke-width="1.25"
    aria-hidden="true"
    focusable="false"
  >
    <path d="M2.5 3.5 H6" />
    <path d="M2.5 8 H6" />
    <path d="M2.5 12.5 H6" />
    <path d="M8.5 3.5 H13.5" />
    <path d="M8.5 8 H13.5" />
    <path d="M8.5 12.5 H13.5" />
  </svg>
{/snippet}

{#snippet SlidesIcon()}
  <svg
    viewBox="0 0 16 16"
    width="12"
    height="12"
    fill="none"
    stroke="currentColor"
    stroke-width="1.25"
    aria-hidden="true"
    focusable="false"
  >
    <path d="M2.5 3 H13.5 V11 H2.5 Z" />
    <path d="M8 11 V13.5" />
    <path d="M5.5 13.5 H10.5" />
  </svg>
{/snippet}

{#snippet PenIcon()}
  <svg
    viewBox="0 0 16 16"
    width="12"
    height="12"
    fill="none"
    stroke="currentColor"
    stroke-width="1.25"
    aria-hidden="true"
    focusable="false"
  >
    <path d="M2.5 13.5 L3.5 10 L10.5 3 L13 5.5 L6 12.5 Z" />
    <path d="M9 4.5 L11.5 7" />
  </svg>
{/snippet}

{#snippet SplitIcon()}
  <svg
    viewBox="0 0 16 16"
    width="12"
    height="12"
    fill="none"
    stroke="currentColor"
    stroke-width="1.25"
    aria-hidden="true"
    focusable="false"
  >
    <path d="M2.5 2.5 H13.5 V13.5 H2.5 Z" />
    <path d="M8 2.5 V13.5" />
  </svg>
{/snippet}

{#snippet ZenIcon()}
  <svg
    viewBox="0 0 16 16"
    width="12"
    height="12"
    fill="none"
    stroke="currentColor"
    stroke-width="1.25"
    aria-hidden="true"
    focusable="false"
  >
    <path d="M2.5 6 V2.5 H6" />
    <path d="M10 2.5 H13.5 V6" />
    <path d="M13.5 10 V13.5 H10" />
    <path d="M6 13.5 H2.5 V10" />
  </svg>
{/snippet}
