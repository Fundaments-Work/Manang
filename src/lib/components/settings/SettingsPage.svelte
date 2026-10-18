<script lang="ts">
  import { onDestroy, onMount, tick, untrack } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";
  import { kernel, reportError, type KernelCommand } from "$lib/kernel";
  import {
    keybindingsApi,
    type KeybindingConflict,
    type KeybindingItem,
    type SettingDef,
    type SettingScope,
  } from "$lib/kernel/api";
  import { relaunchApp, revealInFileManager } from "$lib/kernel/native";
  import { layout } from "$lib/state/layout.svelte";
  import { notice } from "$lib/state/notice.svelte";
  import { settings } from "$lib/state/settings.svelte";
  import { settingsPage } from "$lib/state/settingsPage.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import {
    listPlugins,
    pluginEntries,
    type PluginEntry,
    type PluginInfo,
  } from "$lib/workbench/plugins";
  import {
    filterSettings,
    formatCategoryLabel,
    groupSettings,
  } from "$lib/workbench/settings";
  import AboutView from "./AboutView.svelte";
  import KeybindingsTable from "./KeybindingsTable.svelte";
  import PluginSection from "./PluginSection.svelte";
  import SettingRow from "./SettingRow.svelte";

  /**
   * Settings as a page: a section list on the left, one scrolling column on
   * the right. The app's own settings come first; then every plugin, each
   * with what it is, what it may touch and its own settings, declared in its
   * manifest. Keyboard shortcuts and About are separate views.
   *
   * The search is the top bar's field (`settingsPage.query`), one search for
   * the whole shell. The page is a container-query context, so it adapts to
   * the space it gets: under 768px the section list becomes a scrolling strip
   * and each row stacks its control under its description.
   */

  type View = "settings" | "keyboard" | "about";

  const keys = keybindingsApi(kernel);

  let scope = $state<SettingScope>("user");
  let view = $state<View>("settings");
  let active = $state<string>("");
  let scroller: HTMLElement | undefined = $state();

  let bindings = $state.raw<readonly KeybindingItem[]>([]);
  let conflicts = $state.raw<readonly KeybindingConflict[]>([]);
  let commands = $state.raw<readonly KernelCommand[]>([]);
  let info = $state.raw<PluginInfo[] | null>(null);

  const query = $derived(settingsPage.query.trim());
  const hasProject = $derived(workspace.project !== null);
  const titles = $derived(new Map(commands.map((c) => [c.id, c.title])));
  const plugins = $derived(pluginEntries(info, commands));

  const coreGroups = $derived(
    groupSettings(filterSettings(settings.defs, query)).filter(
      (g) => g.owner === "core",
    ),
  );

  /** A plugin shows when it or one of its settings matches the search. */
  function pluginMatches(entry: PluginEntry, q: string): boolean {
    const needle = q.toLowerCase();
    return (
      entry.name.includes(needle) ||
      formatCategoryLabel(entry.name).toLowerCase().includes(needle) ||
      entry.description.toLowerCase().includes(needle) ||
      entry.commands.some((c) => c.title.toLowerCase().includes(needle))
    );
  }

  const pluginRows = $derived(
    plugins
      .map((entry) => {
        const own = settings.defs.filter((d) => d.owner === entry.name);
        if (query === "" || pluginMatches(entry, query)) return { entry, defs: own };
        return { entry, defs: filterSettings(own, query) };
      })
      .filter(
        ({ entry, defs }) =>
          query === "" || defs.length > 0 || pluginMatches(entry, query),
      ),
  );
  const nothing = $derived(coreGroups.length === 0 && pluginRows.length === 0);

  $effect(() => {
    if (!hasProject && scope === "project") scope = "user";
  });

  $effect(() => {
    if (active === "" && coreGroups[0] !== undefined) active = coreGroups[0].id;
  });

  // Typing a search always shows the settings (shortcuts filter their own view).
  $effect(() => {
    if (query !== "" && untrack(() => view) === "about") view = "settings";
  });

  async function loadKeys(): Promise<void> {
    try {
      const res = await keys.list();
      bindings = res.bindings;
      conflicts = res.conflicts;
    } catch (error) {
      reportError("keyboard shortcuts could not be listed", error);
      bindings = [];
    }
  }

  onMount(() => {
    void settings.load(workspace.project?.root ?? null);
    void loadKeys();
    void Promise.all([listPlugins(kernel), kernel.commands()])
      .then(([list, all]) => {
        info = list;
        commands = all;
      })
      .catch((error: unknown) => reportError("plugins could not be listed", error));
    let unlisten: (() => void) | undefined;
    void kernel
      .onEvent((event) => {
        if (event.topic === "keybindings.changed") void loadKeys();
      })
      .then((stop) => (unlisten = stop));
    return () => unlisten?.();
  });

  // Opening the page at a section (the palette, a plugin link, Home).
  $effect(() => {
    void settingsPage.request;
    const section = untrack(() => settingsPage.section);
    untrack(() => {
      if (section === "keyboard" || section === "about") view = section;
      else {
        view = "settings";
        if (section === "plugins") void reveal("plugins");
        else if (section !== null)
          void reveal(
            settings.defs.some((d) => d.category === section && d.owner === "core")
              ? section
              : `plugin:${section}`,
          );
      }
    });
  });

  async function reveal(id: string): Promise<void> {
    view = "settings";
    active = id;
    await tick();
    if (scroller === undefined) return;
    const target = scroller.querySelector<HTMLElement>(
      `[data-group="${CSS.escape(id)}"]`,
    );
    if (target !== null) scroller.scrollTop = target.offsetTop - 8;
  }

  /**
   * Keep the section list in step with what is on screen. Scroll events come
   * faster than frames, and measuring every section forces a layout, so this
   * measures at most once per frame (WebKitGTK stutters otherwise).
   */
  let spyFrame = 0;
  function onscroll(): void {
    if (spyFrame !== 0) return;
    spyFrame = requestAnimationFrame(() => {
      spyFrame = 0;
      if (scroller === undefined) return;
      const top = scroller.getBoundingClientRect().top + 32;
      let current = "";
      for (const el of scroller.querySelectorAll<HTMLElement>("[data-group]")) {
        if (el.getBoundingClientRect().top > top) break;
        current = el.dataset["group"] ?? "";
      }
      if (current !== "" && current !== active) active = current;
    });
  }
  onDestroy(() => cancelAnimationFrame(spyFrame));

  function disabledReason(def: SettingDef): string | undefined {
    if (scope === "project" && def.scope !== "project")
      return "Applies to you, not to a project. Switch to User to change it.";
    return undefined;
  }

  async function openFile(which: "user" | "project" | "keybindings"): Promise<void> {
    try {
      const path = await settings.openFile(which);
      if (!(await revealInFileManager(path))) notice.show(`Settings file: ${path}`);
    } catch (error) {
      notice.error(
        error instanceof Error
          ? error.message
          : "The settings file could not be opened.",
      );
    }
  }

  async function setKey(command: string, key: string): Promise<void> {
    try {
      const res = await keys.set({ command, key });
      conflicts = res.conflicts;
      await loadKeys();
      notice.show(`Shortcut changed to ${key}.`);
    } catch (error) {
      notice.error(
        error instanceof Error ? error.message : "The shortcut could not be saved.",
      );
    }
  }

  async function resetKey(command: string): Promise<void> {
    try {
      await keys.reset(command);
      await loadKeys();
    } catch (error) {
      notice.error(
        error instanceof Error ? error.message : "The shortcut could not be reset.",
      );
    }
  }
</script>

{#snippet navItem(id: string, label: string, target: View = "settings")}
  {@const current =
    target === "settings" ? view === "settings" && active === id : view === target}
  <li class="shrink-0">
    <button
      type="button"
      class="inset-focus flex h-8 w-full items-center px-4 text-left text-[13px] whitespace-nowrap @3xl:px-6 {current
        ? 'bg-surface-hi text-text'
        : 'text-text-60 hover:text-text'}"
      aria-current={current ? "true" : undefined}
      onclick={() => {
        if (target === "settings") void reveal(id);
        else view = target;
      }}
    >
      {label}
    </button>
  </li>
{/snippet}

{#snippet rows(defs: readonly SettingDef[])}
  {#each defs as def (def.id)}
    <SettingRow
      {def}
      value={settings.value(def.id)}
      source={settings.source(def.id)}
      disabledReason={disabledReason(def)}
      onchange={(value) => settings.set(def.id, value, scope)}
      onreset={() => settings.reset(def.id, scope)}
    />
  {/each}
{/snippet}

<section class="@container flex h-full min-h-0 flex-col bg-bg" aria-label="Settings">
  <header
    class="flex h-12 shrink-0 items-center gap-4 border-b border-line px-4 @3xl:h-14 @3xl:px-6"
  >
    <h1 class="m-0 min-w-0 flex-1 truncate text-base font-light text-text">
      {view === "keyboard"
        ? "Keyboard shortcuts"
        : view === "about"
          ? "About"
          : "Settings"}
    </h1>
    {#if view === "settings"}
      <div
        class="flex shrink-0 border border-line"
        role="radiogroup"
        aria-label="Apply settings to"
      >
        <button
          type="button"
          role="radio"
          aria-checked={scope === "user"}
          class="inset-focus h-7 border-r border-line px-3 text-xs {scope === 'user'
            ? 'bg-surface-hi text-text'
            : 'text-text-60 hover:text-text'}"
          onclick={() => (scope = "user")}
        >
          User
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={scope === "project"}
          disabled={!hasProject}
          title={hasProject ? undefined : "Open a project to change its settings"}
          class="inset-focus h-7 px-3 text-xs disabled:opacity-40 {scope === 'project'
            ? 'bg-surface-hi text-text'
            : 'text-text-60 hover:text-text'}"
          onclick={() => (scope = "project")}
        >
          This project
        </button>
      </div>
    {:else if view === "keyboard"}
      <button
        type="button"
        class="mng-btn shrink-0"
        onclick={() => void openFile("keybindings")}
      >
        <Icon name="file" size={14} />
        <span class="max-[480px]:sr-only">Keybindings file</span>
      </button>
    {/if}
    <button
      type="button"
      class="mng-icon-btn"
      aria-label="Close settings"
      title="Close settings (Esc)"
      onclick={() => settingsPage.hide()}
    >
      <Icon name="close" size={16} />
    </button>
  </header>

  <div class="flex min-h-0 flex-1 flex-col @3xl:flex-row">
    <nav
      class="no-scrollbar shrink-0 overflow-x-auto border-b border-line @3xl:w-56 @3xl:overflow-x-visible @3xl:overflow-y-auto @3xl:border-r @3xl:border-b-0 @3xl:py-4"
      aria-label="Settings sections"
    >
      <ul class="m-0 flex list-none p-0 @3xl:flex-col">
        {#each coreGroups as group (group.id)}
          {@render navItem(group.id, group.label)}
        {/each}
        {@render navItem("plugins", "Plugins")}
        {#each pluginRows as { entry } (entry.name)}
          {@const id = `plugin:${entry.name}`}
          <li class="hidden @3xl:block">
            <button
              type="button"
              class="inset-focus flex h-7 w-full items-center pr-6 pl-9 text-left text-xs whitespace-nowrap {view ===
                'settings' && active === id
                ? 'bg-surface-hi text-text'
                : 'text-text-60 hover:text-text'}"
              aria-current={view === "settings" && active === id ? "true" : undefined}
              onclick={() => void reveal(id)}
            >
              {formatCategoryLabel(entry.name)}
            </button>
          </li>
        {/each}
        <li class="hidden h-4 @3xl:block" aria-hidden="true"></li>
        {@render navItem("keyboard", "Keyboard shortcuts", "keyboard")}
        {@render navItem("about", "About", "about")}
      </ul>
    </nav>

    <div
      bind:this={scroller}
      class="relative min-h-0 min-w-0 flex-1 overflow-y-auto"
      onscroll={view === "settings" ? onscroll : undefined}
    >
      <div class="mx-auto w-full max-w-3xl px-4 pt-2 pb-24 @3xl:px-8 @3xl:pt-4">
        {#if view === "settings"}
          {#if settings.error !== "" && settings.defs.length === 0}
            <p class="m-0 py-6 text-xs text-text-60">{settings.error}</p>
          {:else if !settings.loaded}
            <p class="m-0 py-6 text-xs text-text-60" role="status">Loading settings</p>
          {:else if nothing}
            <p class="m-0 py-6 text-xs text-text-60">
              No setting or plugin matches “{query}”.
            </p>
          {:else}
            {#if settings.restartPending}
              <div
                class="mt-4 flex flex-wrap items-center justify-between gap-3 border border-line bg-surface px-4 py-3"
                role="status"
              >
                <p class="m-0 text-xs text-text">
                  Some changes apply after Manang restarts.
                </p>
                <button
                  type="button"
                  class="mng-btn"
                  onclick={() => void relaunchApp()}
                >
                  Restart now
                </button>
              </div>
            {/if}
            {#if scope === "project"}
              <p class="m-0 mt-4 text-xs leading-relaxed text-text-60">
                Saved in this project's <span class="font-mono"
                  >.manang/settings.toml</span
                >, so they apply to everyone who opens it.
              </p>
            {/if}

            {#each coreGroups as group (group.id)}
              <section
                data-group={group.id}
                aria-labelledby="settings-group-{group.id}"
              >
                <h2
                  id="settings-group-{group.id}"
                  class="m-0 pt-8 pb-1 text-[15px] font-light text-text"
                >
                  {group.label}
                </h2>
                {@render rows(group.settings)}
                {#if group.id === "appearance" && query === ""}
                  <div
                    class="grid gap-x-8 gap-y-3 border-t border-line-subtle py-4 @xl:grid-cols-[minmax(0,1fr)_auto]"
                  >
                    <div>
                      <p class="m-0 text-[13px] text-text">Panel layout</p>
                      <p class="m-0 mt-1 text-xs leading-relaxed text-text-60">
                        Put the side panel, bottom panel and preview split back to their
                        default sizes.
                      </p>
                    </div>
                    <div class="flex items-center @xl:justify-end">
                      <button
                        type="button"
                        class="mng-btn"
                        onclick={() => {
                          layout.reset();
                          notice.show("Panel layout reset.");
                        }}
                      >
                        Reset layout
                      </button>
                    </div>
                  </div>
                {/if}
              </section>
            {/each}

            {#if pluginRows.length > 0}
              <section data-group="plugins" aria-labelledby="settings-plugins">
                <h2
                  id="settings-plugins"
                  class="m-0 pt-10 pb-1 text-[15px] font-light text-text"
                >
                  Plugins
                </h2>
                <p class="m-0 max-w-[60ch] text-xs leading-relaxed text-text-60">
                  Everything in Manang is a plugin. Each declares in its manifest what
                  it may access and which settings it has. Plugins from other authors
                  run as WebAssembly in a sandbox.
                </p>
                {#each pluginRows as { entry, defs } (entry.name)}
                  <PluginSection {entry}>
                    {#if defs.length > 0}
                      {@render rows(defs)}
                    {/if}
                  </PluginSection>
                {/each}
              </section>
            {/if}

            <div class="flex flex-wrap items-center gap-2 pt-10">
              <button
                type="button"
                class="mng-btn"
                onclick={() => void openFile("user")}
              >
                <Icon name="file" size={14} />
                User settings file
              </button>
              {#if hasProject}
                <button
                  type="button"
                  class="mng-btn"
                  onclick={() => void openFile("project")}
                >
                  <Icon name="file" size={14} />
                  Project settings file
                </button>
              {/if}
            </div>
          {/if}
        {:else if view === "keyboard"}
          <KeybindingsTable
            {bindings}
            {conflicts}
            {titles}
            query={settingsPage.query}
            onchange={setKey}
            onreset={resetKey}
          />
        {:else}
          <AboutView />
        {/if}
      </div>
    </div>
  </div>
</section>
