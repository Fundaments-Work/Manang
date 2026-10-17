<script lang="ts">
  import Icon, { type IconName } from "$lib/components/ui/Icon.svelte";
  import Mark from "./Mark.svelte";
  import { formatShortcut } from "$lib/commands/shortcut";
  import { PALETTE_COMMANDS_SHORTCUTS } from "$lib/commands/workbench";
  import { exportPdf } from "$lib/state/actions";
  import { build } from "$lib/state/build.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { palette } from "$lib/state/palette.svelte";
  import { preview } from "$lib/state/preview.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { detectPlatform } from "$lib/util/keyboard";
  import type { PreviewMode } from "$lib/workbench/layout";
  import { engineOf } from "$lib/workbench/paths";

  /**
   * Top bar: the mark, a breadcrumb of the open file, a field that opens the
   * command palette, and three plain actions: preview layout, export and zen.
   * Nothing else lives here; everything else is in the palette or a panel.
   */

  const platform = detectPlatform();
  const paletteHint = formatShortcut(
    PALETTE_COMMANDS_SHORTCUTS[0] ?? { key: "k", mod: true },
    platform,
  );

  const crumbs = $derived.by(() => {
    const project = workspace.project;
    const active = tabs.active;
    if (project === null) return [];
    const parts = active === undefined ? [] : active.path.split("/");
    return [project.name, ...parts];
  });

  const isDocument = $derived(
    tabs.active?.kind === "document" && tabs.active.view === "text",
  );
  const canExport = $derived(
    tabs.active !== undefined &&
      engineOf(tabs.active.path) === "typst" &&
      (build.status === "ok" || build.status === "failed"),
  );

  const modes: readonly { mode: PreviewMode; label: string; icon: IconName }[] = [
    { mode: "split", label: "Split", icon: "split" },
    { mode: "editor", label: "Editor", icon: "editor" },
    { mode: "preview", label: "Preview", icon: "preview" },
  ];

  function onModeKey(event: KeyboardEvent): void {
    const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (step === 0) return;
    event.preventDefault();
    const at = modes.findIndex((m) => m.mode === layout.preview);
    const next = modes[(at + step + modes.length) % modes.length];
    if (next === undefined) return;
    layout.setPreview(next.mode);
    (event.currentTarget as HTMLElement)
      .querySelector<HTMLElement>(`[data-mode="${next.mode}"]`)
      ?.focus();
  }
</script>

<header class="flex h-10 shrink-0 items-center gap-3 border-b border-line bg-bg px-3">
  <div class="flex shrink-0 items-center gap-2">
    <Mark class="text-text" />
    <span
      class="hidden text-[15px] font-light text-text min-[900px]:inline"
      style="letter-spacing: var(--tracking-headline)">Manang</span
    >
  </div>

  <nav aria-label="Breadcrumb" class="selectable min-w-0 flex-1 basis-0">
    <ol
      class="m-0 flex min-w-0 list-none items-center gap-1.5 p-0 text-xs text-text-60"
    >
      {#each crumbs as crumb, index (index)}
        {@const last = index === crumbs.length - 1}
        <li
          class="min-w-0 truncate {last ? 'text-text' : ''} {!last &&
          index < crumbs.length - 2
            ? 'max-[900px]:hidden'
            : ''}"
          aria-current={last ? "page" : undefined}
        >
          {crumb}
        </li>
        {#if !last}
          <li
            aria-hidden="true"
            class="text-text-muted {index < crumbs.length - 2
              ? 'max-[900px]:hidden'
              : ''}"
          >
            /
          </li>
        {/if}
      {/each}
    </ol>
  </nav>

  <button
    type="button"
    class="flex h-7 w-[min(360px,34vw)] shrink-0 items-center gap-2 border border-line bg-surface px-2.5 text-left text-xs text-text-60 hover:bg-surface-hi max-[900px]:w-7 max-[900px]:justify-center max-[900px]:px-0"
    aria-haspopup="dialog"
    aria-label="Search commands"
    aria-keyshortcuts="Control+K"
    onclick={() => palette.show()}
  >
    <Icon name="search" size={14} class="shrink-0" />
    <span class="flex-1 truncate max-[900px]:hidden">Search commands</span>
    <span class="mng-kbd max-[900px]:hidden">{paletteHint}</span>
  </button>

  <div class="flex min-w-0 flex-1 basis-0 items-center justify-end gap-2">
    {#if workspace.project !== null}
      {#if isDocument}
        <div
          role="radiogroup"
          aria-label="Preview layout"
          class="flex shrink-0 gap-px"
          onkeydown={onModeKey}
          tabindex="-1"
        >
          {#each modes as item (item.mode)}
            {@const on = layout.effectivePreview === item.mode}
            {@const tooWide = item.mode === "split" && layout.narrow}
            <button
              type="button"
              role="radio"
              data-mode={item.mode}
              aria-checked={on}
              aria-disabled={tooWide}
              tabindex={on ? 0 : -1}
              title={tooWide ? "Split needs a wider window" : item.label}
              class="mng-btn px-2 max-[1100px]:px-1.5 {tooWide ? 'opacity-50' : ''}"
              onclick={() => !tooWide && layout.setPreview(item.mode)}
            >
              <Icon name={item.icon} size={14} />
              <span class="max-[1100px]:sr-only">{item.label}</span>
            </button>
          {/each}
        </div>
      {/if}

      <button
        type="button"
        class="mng-btn shrink-0 px-2 max-[1100px]:px-1.5"
        disabled={!canExport || preview.exporting}
        title="Export PDF"
        onclick={() => exportPdf()}
      >
        <Icon name="export" size={14} />
        <span class="max-[1100px]:sr-only">Export PDF</span>
      </button>

      <button
        type="button"
        class="mng-btn shrink-0 px-2 max-[1100px]:px-1.5"
        disabled={!isDocument}
        title="Zen mode ({formatShortcut({ key: '\\', mod: true }, platform)})"
        onclick={() => layout.toggleZen()}
      >
        <Icon name="zen" size={14} />
        <span class="max-[1100px]:sr-only">Zen</span>
      </button>
    {/if}
  </div>
</header>
