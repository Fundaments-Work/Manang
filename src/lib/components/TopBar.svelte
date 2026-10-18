<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import Mark from "./Mark.svelte";
  import WindowControls from "./WindowControls.svelte";
  import MenuBar from "./menu/MenuBar.svelte";
  import NativeMenu from "./menu/NativeMenu.svelte";
  import { hasCustomTitleBar, osPlatform } from "$lib/kernel/native";
  import { formatShortcut } from "$lib/commands/shortcut";
  import { goHome, openFolder } from "$lib/state/actions";
  import { context } from "$lib/state/context.svelte";
  import { home } from "$lib/state/home.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { settingsPage } from "$lib/state/settingsPage.svelte";
  import { palette } from "$lib/state/palette.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { detectPlatform } from "$lib/util/keyboard";

  /**
   * Top bar, which is also the window's title bar: the mark (it goes Home), a
   * breadcrumb, a search field where there is something to search, the
   * surface's few actions, and the window controls.
   *
   * The menus (File ... Help) sit between the mark and the breadcrumb.
   *
   * There is one search and it searches what is on screen: projects on Home,
   * settings and plugins on the Settings page. In the editor the field is
   * gone to keep the screen quiet; Ctrl+K (commands), Ctrl+P (files) and
   * Ctrl+F (in the document) are always there.
   *
   * In the editor the bar carries no buttons: the preview's controls live in
   * the preview's own header, "Show preview" at the end of the tab strip, and
   * zen mode on its shortcut and in the command palette.
   */

  type Surface = typeof context.surface;
  const surface: Surface = $derived(context.surface);
  const PLACEHOLDER: Record<Surface, string> = {
    settings: "Search settings and plugins",
    home: "Search projects",
    workbench: "Search files and commands",
  };
  let field: HTMLInputElement | undefined = $state();

  // The window has no system title bar: this bar is the drag handle (every
  // element marked `data-tauri-drag-region`; controls stay clickable) and
  // carries the window controls, on the left on macOS as people expect there.
  const ownControls = hasCustomTitleBar();
  let controlsLeft = $state(false);
  // The menus: drawn in this bar on Windows, Linux and in a browser; on macOS
  // they go to the native application menu (NativeMenu) instead. Nothing is
  // drawn until the platform is known, so macOS never flashes a menu bar.
  let platformKnown = $state(!ownControls);
  if (ownControls)
    void osPlatform()
      .then((p) => (controlsLeft = p === "macos"))
      .finally(() => (platformKnown = true));
  const menusInBar = $derived(platformKnown && !controlsLeft);
  /** Below 900 px the six menus fold into one Menu button. */
  const foldMenus = $derived(layout.width < 900);

  function inlineQuery(): string {
    return surface === "settings" ? settingsPage.query : home.query;
  }

  function setInlineQuery(value: string): void {
    if (surface === "settings") settingsPage.query = value;
    else home.query = value;
  }

  function onSearchKey(event: KeyboardEvent): void {
    // Escape clears the query first; on an empty field it passes through, so
    // the workbench closes the page (Settings, or Home over a project).
    if (event.key !== "Escape") return;
    if (inlineQuery() !== "") {
      setInlineQuery("");
      event.preventDefault();
      event.stopPropagation();
    } else field?.blur();
  }

  /** "/" focuses the search field, as on most sites and in many editors. */
  function onWindowKey(event: KeyboardEvent): void {
    if (event.key !== "/" || event.ctrlKey || event.metaKey || event.altKey) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest("input, textarea, select, [contenteditable], .cm-editor"))
      return;
    event.preventDefault();
    if (surface === "workbench")
      palette.show(workspace.project === null ? "commands" : "files");
    else field?.focus();
  }

  const platform = detectPlatform();
  const homeHint = formatShortcut({ key: "h", mod: true, shift: true }, platform);

  const crumbs = $derived.by(() => {
    if (surface === "home") return ["Home"];
    if (surface === "settings") return ["Settings"];
    const project = workspace.project;
    const active = tabs.active;
    if (project === null) return [];
    const parts = active === undefined ? [] : active.path.split("/");
    return [project.name, ...parts];
  });
</script>

<svelte:window onkeydown={onWindowKey} />

<header
  class="flex h-10 shrink-0 items-center gap-3 border-b border-line bg-bg pl-3 {ownControls &&
  !controlsLeft
    ? 'pr-0'
    : 'pr-3'} {ownControls && controlsLeft ? 'pl-0' : ''}"
  data-tauri-drag-region
>
  {#if ownControls && controlsLeft}
    <WindowControls />
  {/if}
  <button
    type="button"
    class="inset-focus -ml-1.5 flex h-8 shrink-0 items-center gap-2 px-1.5 text-text hover:bg-surface-hi"
    aria-label="Home"
    title="Home ({homeHint})"
    aria-keyshortcuts="Control+Shift+H"
    onclick={() => {
      settingsPage.hide();
      goHome();
    }}
  >
    <Mark class="text-text" />
    <span
      class="hidden text-[15px] font-light {menusInBar ? '' : 'min-[900px]:inline'}"
      style="letter-spacing: var(--tracking-headline)">Manang</span
    >
  </button>

  {#if menusInBar}
    <MenuBar folded={foldMenus} />
  {:else if platformKnown && controlsLeft}
    <NativeMenu />
  {/if}

  <nav
    aria-label="Breadcrumb"
    class="min-w-0 flex-1 basis-0 {surface === 'workbench' ? '' : 'max-[640px]:hidden'}"
    data-tauri-drag-region
  >
    <ol
      class="pointer-events-none m-0 flex min-w-0 list-none items-center gap-1.5 p-0 text-xs text-text-60"
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

  {#if surface !== "workbench"}
    <label
      class="relative flex h-7 w-[min(360px,40vw)] shrink-0 items-center max-[640px]:w-auto max-[640px]:flex-1"
    >
      <Icon
        name="search"
        size={14}
        class="pointer-events-none absolute left-2.5 text-text-60"
      />
      <input
        bind:this={field}
        type="search"
        class="mng-field h-7 bg-surface pr-12 pl-8"
        placeholder={PLACEHOLDER[surface]}
        aria-label={PLACEHOLDER[surface]}
        spellcheck="false"
        autocomplete="off"
        value={inlineQuery()}
        oninput={(e) => setInlineQuery(e.currentTarget.value)}
        onkeydown={onSearchKey}
      />
      <span class="mng-kbd pointer-events-none absolute right-2.5 max-[900px]:hidden"
        >/</span
      >
    </label>

    <div
      class="flex min-w-0 flex-1 basis-0 items-center justify-end gap-2"
      data-tauri-drag-region
    >
      {#if surface === "home"}
        <button
          type="button"
          class="mng-btn shrink-0 px-2 max-[1100px]:px-1.5"
          title="New project"
          onclick={() => home.openNew()}
        >
          <Icon name="plus" size={14} />
          <span class="max-[1100px]:sr-only">New project</span>
        </button>
        <button
          type="button"
          class="mng-btn shrink-0 px-2 max-[1100px]:px-1.5"
          title="Open folder"
          onclick={() => void openFolder()}
        >
          <Icon name="folder-open" size={14} />
          <span class="max-[1100px]:sr-only">Open folder</span>
        </button>
      {/if}
    </div>
  {/if}
  {#if ownControls && !controlsLeft}
    <div class="h-full w-px shrink-0 bg-line" aria-hidden="true"></div>
    <WindowControls />
  {/if}
</header>
