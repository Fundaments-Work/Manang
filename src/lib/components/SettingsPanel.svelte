<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { kernel } from "$lib/kernel";
  import {
    keybindingsApi,
    settingsApi,
    themeApi,
    type KeybindingConflict,
    type KeybindingItem,
    type SettingDef,
    type SettingScope,
    type SettingsSchema,
    type ThemeSummary,
  } from "$lib/kernel/api";
  import { layout } from "$lib/state/layout.svelte";
  import { notice } from "$lib/state/notice.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import type { Theme } from "$lib/workbench/layout";
  import {
    filterSettings,
    groupSettings,
  } from "$lib/workbench/settings";
  import AboutView from "./settings/AboutView.svelte";
  import KeybindingsTable from "./settings/KeybindingsTable.svelte";
  import SettingRow from "./settings/SettingRow.svelte";

  const settingsKernel = settingsApi(kernel);
  const keybindingsKernel = keybindingsApi(kernel);
  const themeKernel = themeApi(kernel);

  type Tab = "settings" | "keybindings" | "about";
  let activeTab = $state<Tab>("settings");

  // Settings State
  let schema = $state<SettingsSchema | null>(null);
  let values = $state<Record<string, { value: unknown; source: "default" | "user" | "project" }>>({});
  let activeScope = $state<SettingScope>("user");
  let searchQuery = $state("");
  let loading = $state(false);

  // Keybindings State
  let bindings = $state<readonly KeybindingItem[]>([]);
  let conflicts = $state<readonly KeybindingConflict[]>([]);

  // Themes State
  let themes = $state<readonly ThemeSummary[]>([]);

  const hasProject = $derived(workspace.project !== null);

  const allSettings = $derived(schema?.settings ?? []);
  const filtered = $derived(filterSettings(allSettings, searchQuery));
  const groups = $derived(groupSettings(filtered));

  async function loadAll(): Promise<void> {
    loading = true;
    try {
      if (hasProject && workspace.project?.root) {
        await settingsKernel.useProject(workspace.project.root);
      }

      const [sSchema, sValues, kList, tList] = await Promise.all([
        settingsKernel.schema(),
        settingsKernel.get(),
        keybindingsKernel.list(),
        themeKernel.list(),
      ]);

      schema = sSchema;
      values = sValues.values;
      bindings = kList.bindings;
      conflicts = kList.conflicts;
      themes = tList.themes;
    } catch (e) {
      notice.error(e instanceof Error ? e.message : "Failed to load settings.");
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void loadAll();

    let unlisten: (() => void) | undefined;
    void kernel
      .onEvent((ev) => {
        if (ev.topic === "settings.changed") {
          const data = ev.payload as { id: string; value: unknown; scope: "default" | "user" | "project" };
          if (data && data.id) {
            values[data.id] = { value: data.value, source: data.scope };
          }
        } else if (ev.topic === "keybindings.changed") {
          void keybindingsKernel.list().then((res) => {
            bindings = res.bindings;
            conflicts = res.conflicts;
          });
        }
      })
      .then((unsub) => {
        unlisten = unsub;
      });

    return () => {
      unlisten?.();
    };
  });

  async function handleSettingChange(def: SettingDef, nextValue: unknown): Promise<void> {
    try {
      const res = await settingsKernel.set({
        id: def.id,
        value: nextValue,
        scope: activeScope,
      });
      values[def.id] = { value: res.value, source: res.source };

      // If theme changed, apply immediately
      if (def.id === "appearance.theme" && typeof nextValue === "string") {
        if (nextValue === "dark" || nextValue === "light") {
          layout.setTheme(nextValue as Theme);
        }
      }
    } catch (e) {
      notice.error(e instanceof Error ? e.message : "Failed to update setting.");
    }
  }

  async function handleSettingReset(def: SettingDef): Promise<void> {
    try {
      const res = await settingsKernel.reset({
        id: def.id,
        scope: activeScope,
      });
      values[def.id] = { value: res.value, source: res.source };

      if (def.id === "appearance.theme" && typeof res.value === "string") {
        if (res.value === "dark" || res.value === "light") {
          layout.setTheme(res.value as Theme);
        }
      }
    } catch (e) {
      notice.error(e instanceof Error ? e.message : "Failed to reset setting.");
    }
  }

  async function handleKeyChange(command: string, key: string): Promise<void> {
    try {
      const res = await keybindingsKernel.set({ command, key });
      notice.show(`Updated keybinding for ${command} to ${key}.`);
      bindings = (await keybindingsKernel.list()).bindings;
      conflicts = res.conflicts;
    } catch (e) {
      notice.error(e instanceof Error ? e.message : "Failed to save keybinding.");
    }
  }

  async function handleKeyReset(command: string): Promise<void> {
    try {
      await keybindingsKernel.reset(command);
      notice.show(`Reset keybinding for ${command}.`);
      const res = await keybindingsKernel.list();
      bindings = res.bindings;
      conflicts = res.conflicts;
    } catch (e) {
      notice.error(e instanceof Error ? e.message : "Failed to reset keybinding.");
    }
  }

  async function handleOpenFile(scope: "user" | "project" | "keybindings"): Promise<void> {
    try {
      const res = await settingsKernel.openFile(scope);
      notice.show(`Settings file: ${res.path}`);
    } catch (e) {
      notice.error(e instanceof Error ? e.message : "Failed to open settings file.");
    }
  }

  async function handleThemeSelect(themeId: string): Promise<void> {
    try {
      await settingsKernel.set({
        id: "appearance.theme",
        value: themeId,
        scope: activeScope,
      });
      if (themeId === "dark" || themeId === "light") {
        layout.setTheme(themeId as Theme);
      }
    } catch (e) {
      notice.error(e instanceof Error ? e.message : "Failed to switch theme.");
    }
  }
</script>

<div class="mng-panel-head flex items-center justify-between border-b border-line px-3 py-2">
  <div class="flex items-center gap-1 font-medium text-xs text-text">
    <span>Settings</span>
  </div>
  <div class="flex items-center gap-1">
    <button
      type="button"
      class="mng-btn px-2 py-0.5 text-[11px] {activeTab === 'settings' ? 'border-accent text-accent' : 'text-text-60'}"
      onclick={() => (activeTab = "settings")}
    >
      Settings
    </button>
    <button
      type="button"
      class="mng-btn px-2 py-0.5 text-[11px] {activeTab === 'keybindings' ? 'border-accent text-accent' : 'text-text-60'}"
      onclick={() => (activeTab = "keybindings")}
    >
      Keyboard
    </button>
    <button
      type="button"
      class="mng-btn px-2 py-0.5 text-[11px] {activeTab === 'about' ? 'border-accent text-accent' : 'text-text-60'}"
      onclick={() => (activeTab = "about")}
    >
      About
    </button>
  </div>
</div>

<div class="min-h-0 flex-1 overflow-y-auto">
  {#if activeTab === "settings"}
    <!-- Settings toolbar: Scope and Search -->
    <div class="border-b border-line p-3 space-y-2 bg-surface">
      <div class="flex items-center justify-between gap-2">
        <!-- Scope Toggle -->
        <div class="flex items-center gap-1 border border-line p-0.5 bg-bg text-xs">
          <button
            type="button"
            class="px-2 py-0.5 font-medium transition-colors {activeScope === 'user' ? 'bg-surface text-accent' : 'text-text-60 hover:text-text'}"
            onclick={() => (activeScope = "user")}
          >
            User
          </button>
          <button
            type="button"
            disabled={!hasProject}
            class="px-2 py-0.5 font-medium transition-colors {!hasProject ? 'opacity-40 cursor-not-allowed' : activeScope === 'project' ? 'bg-surface text-accent' : 'text-text-60 hover:text-text'}"
            title={!hasProject ? "Open a project folder to edit project settings" : undefined}
            onclick={() => hasProject && (activeScope = "project")}
          >
            Project
          </button>
        </div>

        <!-- Open settings file -->
        <button
          type="button"
          class="mng-btn text-[11px]"
          onclick={() => void handleOpenFile(activeScope === "project" ? "project" : "user")}
        >
          Open file
        </button>
      </div>

      <!-- Search Field -->
      <div class="relative">
        <input
          type="search"
          bind:value={searchQuery}
          placeholder="Search settings (e.g. font, wrap, theme)..."
          class="mng-input w-full pl-8 text-xs"
          aria-label="Search settings"
        />
        <div class="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-text-muted">
          <Icon name="search" size={13} />
        </div>
      </div>
    </div>

    <!-- Theme quick bar -->
    <div class="border-b border-line px-4 py-3 bg-surface-hi/20">
      <div class="mb-2 text-xs font-medium text-text">Theme</div>
      <div class="flex gap-2">
        {#each themes as t (t.id)}
          <button
            type="button"
            class="mng-btn flex-1 justify-center text-xs {layout.theme === t.id ? 'border-accent text-accent' : 'text-text-60'}"
            onclick={() => void handleThemeSelect(t.id)}
          >
            {t.name}
          </button>
        {/each}
      </div>
    </div>

    <!-- Grouped Settings List -->
    {#if loading && schema === null}
      <div class="p-6 text-center text-xs text-text-muted">Loading settings...</div>
    {:else if groups.length === 0}
      <div class="p-6 text-center text-xs text-text-muted">
        No settings match "{searchQuery}".
      </div>
    {:else}
      <div class="divide-y divide-line">
        {#each groups as group (group.id)}
          <div>
            <div class="bg-surface-hi/60 px-4 py-1.5 font-mono text-[10px] font-bold tracking-wider text-text-60">
              {group.label.toUpperCase()}
            </div>
            <div>
              {#each group.settings as def (def.id)}
                {@const itemValue = values[def.id]?.value ?? def.default}
                {@const itemSource = values[def.id]?.source ?? "default"}
                <SettingRow
                  {def}
                  value={itemValue}
                  source={itemSource}
                  onchange={(val) => void handleSettingChange(def, val)}
                  onreset={() => void handleSettingReset(def)}
                />
              {/each}
            </div>
          </div>
        {/each}
      </div>
    {/if}

    <!-- Bottom Layout Reset -->
    <div class="border-t border-line p-4 space-y-2 bg-surface">
      <div class="text-xs font-medium text-text">Layout & State</div>
      <p class="m-0 text-[11px] text-text-60 leading-relaxed">
        Reset panel sizes and tab splits to factory defaults.
      </p>
      <button
        type="button"
        class="mng-btn text-xs"
        onclick={() => {
          layout.reset();
          notice.show("Layout reset to default.");
        }}
      >
        Reset layout
      </button>
    </div>

  {:else if activeTab === "keybindings"}
    <div class="p-3">
      <div class="flex items-center justify-between mb-3">
        <span class="text-xs text-text-60">Custom and default shortcut bindings</span>
        <button
          type="button"
          class="mng-btn text-[11px]"
          onclick={() => void handleOpenFile("keybindings")}
        >
          Open keybindings.toml
        </button>
      </div>

      <KeybindingsTable
        {bindings}
        {conflicts}
        onchange={handleKeyChange}
        onreset={handleKeyReset}
      />
    </div>

  {:else}
    <AboutView />
  {/if}
</div>
