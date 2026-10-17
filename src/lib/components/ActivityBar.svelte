<script lang="ts">
  import Icon, { type IconName } from "$lib/components/ui/Icon.svelte";
  import { formatShortcut } from "$lib/commands/shortcut";
  import { layout } from "$lib/state/layout.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { sidePanelVisible, type SideView } from "$lib/workbench/layout";
  import { detectPlatform } from "$lib/util/keyboard";

  /**
   * Activity bar: the narrow icon rail on the left. Each icon opens its side
   * panel; clicking the open one collapses it. It is a vertical toolbar, so Tab
   * enters it once and the arrow keys move between icons.
   */

  interface Item {
    readonly view: SideView;
    readonly label: string;
    readonly icon: IconName;
    readonly shortcut?: string;
  }

  const platform = detectPlatform();
  const top: readonly Item[] = [
    {
      view: "files",
      label: "Files",
      icon: "files",
      shortcut: formatShortcut({ key: "e", mod: true, shift: true }, platform),
    },
    {
      view: "search",
      label: "Search",
      icon: "search",
      shortcut: formatShortcut({ key: "f", mod: true, shift: true }, platform),
    },
    { view: "source-control", label: "Source Control", icon: "source-control" },
    { view: "outline", label: "Outline", icon: "outline" },
    { view: "figures", label: "Figures", icon: "figures" },
    { view: "references", label: "References", icon: "references" },
    {
      view: "plugins",
      label: "Plugins",
      icon: "plugins",
      shortcut: formatShortcut({ key: "x", mod: true, shift: true }, platform),
    },
  ];
  const bottom: Item = {
    view: "settings",
    label: "Settings",
    icon: "settings",
    shortcut: formatShortcut({ key: ",", mod: true }, platform),
  };
  const all = [...top, bottom];

  let focused = $state<SideView>("files");
  let bar: HTMLElement | undefined = $state();

  const changes = $derived(workspace.git?.changes.length ?? 0);

  function onkeydown(event: KeyboardEvent): void {
    const delta = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    const edge = event.key === "Home" ? 0 : event.key === "End" ? all.length - 1 : -1;
    if (delta === 0 && edge === -1) return;
    event.preventDefault();
    const at = all.findIndex((item) => item.view === focused);
    const next = all[edge === -1 ? (at + delta + all.length) % all.length : edge];
    if (next === undefined) return;
    focused = next.view;
    bar?.querySelector<HTMLElement>(`[data-view="${next.view}"]`)?.focus();
  }
</script>

{#snippet button(item: Item)}
  {@const open =
    layout.sideView === item.view &&
    sidePanelVisible(layout.sideView, workspace.project !== null)}
  {@const needsProject =
    workspace.project === null && !sidePanelVisible(item.view, false)}
  <button
    type="button"
    data-view={item.view}
    class="group relative flex h-12 w-12 shrink-0 items-center justify-center inset-focus {needsProject
      ? 'opacity-40'
      : ''} {open ? 'text-text' : 'text-text-60 hover:text-text'}"
    aria-label={item.label}
    aria-pressed={open}
    aria-disabled={needsProject}
    title={needsProject ? `${item.label}: open a folder first` : undefined}
    tabindex={focused === item.view ? 0 : -1}
    onfocus={() => (focused = item.view)}
    onclick={() => !needsProject && layout.toggleSide(item.view)}
  >
    {#if open}
      <span class="absolute inset-y-0 left-0 w-0.5 bg-accent" aria-hidden="true"></span>
    {/if}
    <Icon name={item.icon} size={20} />
    {#if item.view === "files" && changes > 0}
      <span
        class="absolute right-1.5 bottom-1.5 flex h-3.5 min-w-3.5 items-center justify-center bg-surface-hi px-0.5 font-mono text-[9px] leading-none text-text"
        title="{changes} changed {changes === 1 ? 'file' : 'files'}"
      >
        {changes > 99 ? "99+" : changes}
      </span>
    {/if}
    <!-- Tooltip: instant, to the right of the rail, label then shortcut. -->
    <span
      class="pointer-events-none absolute top-1/2 left-full z-30 ml-1 hidden -translate-y-1/2 items-center gap-2 border border-line bg-surface-hi px-2 py-1 text-xs whitespace-nowrap text-text group-hover:flex group-focus-visible:flex"
      aria-hidden="true"
    >
      {item.label}
      {#if item.shortcut !== undefined}<span class="mng-kbd">{item.shortcut}</span>{/if}
    </span>
  </button>
{/snippet}

<nav
  bind:this={bar}
  class="flex w-12 shrink-0 flex-col border-r border-line bg-bg"
  aria-label="Workbench views"
>
  <div
    role="toolbar"
    aria-orientation="vertical"
    aria-label="Views"
    tabindex="-1"
    class="flex flex-1 flex-col"
    {onkeydown}
  >
    {#each top as item (item.view)}
      {@render button(item)}
    {/each}
    <div class="mt-auto">
      {@render button(bottom)}
    </div>
  </div>
</nav>
