<script lang="ts">
  import { layout } from "$lib/state/layout.svelte";
  import type { Theme } from "$lib/workbench/layout";

  /**
   * Settings. Kept to what exists: the theme and the layout reset. Preferences
   * are stored in this window's browser storage until the settings plugin takes
   * over, which is why there is nothing else to configure yet.
   */

  const themes: readonly { id: Theme; label: string }[] = [
    { id: "dark", label: "Dark" },
    { id: "light", label: "Light" },
  ];
</script>

<div class="mng-panel-head"><h2>Settings</h2></div>

<div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
  <fieldset class="m-0 border-0 p-0">
    <legend class="mb-2 p-0 text-xs text-text">Theme</legend>
    <div role="radiogroup" aria-label="Theme" class="flex gap-px">
      {#each themes as theme (theme.id)}
        <button
          type="button"
          role="radio"
          aria-checked={layout.theme === theme.id}
          class="mng-btn flex-1 justify-center"
          onclick={() => layout.setTheme(theme.id)}
        >
          {theme.label}
        </button>
      {/each}
    </div>
  </fieldset>

  <div class="mt-6">
    <p class="m-0 mb-2 text-xs text-text">Layout</p>
    <p class="m-0 mb-3 text-[11px] leading-relaxed text-text-60">
      Panel sizes, open panels and open tabs are remembered between sessions.
    </p>
    <button type="button" class="mng-btn" onclick={() => layout.reset()}
      >Reset layout</button
    >
  </div>

  <p class="m-0 mt-8 text-[11px] leading-relaxed text-text-muted">
    These preferences live in this window for now. The settings plugin will store them
    with your user profile.
  </p>
  <p class="m-0 mt-4 font-mono text-[10px] text-text-muted">Made by Fundaments.work</p>
</div>
