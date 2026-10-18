<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import { workbenchCommands } from "$lib/commands/workbench";
  import { detectPlatform } from "$lib/util/keyboard";
  import {
    extractShortcutGroups,
    filterShortcutGroups,
    type ShortcutGroup,
  } from "$lib/workbench/shortcuts";

  /**
   * Shortcut reference overlay (Ctrl+/):
   * Cheat-sheet listing all workbench shortcuts organized by category.
   * Read-only; does not modify command definitions.
   */

  let open = $state(false);
  let filterText = $state("");
  let inputEl = $state<HTMLInputElement | null>(null);

  const platform = detectPlatform();

  // Read workbench commands without performing any actions
  const allCommands = workbenchCommands(() => {});
  const groups: readonly ShortcutGroup[] = extractShortcutGroups(allCommands, platform);
  const filteredGroups = $derived(filterShortcutGroups(groups, filterText));

  export function show(): void {
    open = true;
    filterText = "";
    setTimeout(() => inputEl?.focus(), 50);
  }

  export function hide(): void {
    open = false;
  }

  function onkeydown(event: KeyboardEvent): void {
    // Ctrl+/ or Cmd+/ or Ctrl+? toggles the cheat sheet
    const isMod = event.ctrlKey || event.metaKey;
    if (isMod && (event.key === "/" || event.key === "?")) {
      event.preventDefault();
      if (open) {
        hide();
      } else {
        show();
      }
      return;
    }

    if (open && event.key === "Escape") {
      event.preventDefault();
      hide();
    }
  }
</script>

<svelte:window {onkeydown} />

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) hide();
    }}
  >
    <div
      class="flex max-h-[85vh] w-full max-w-2xl flex-col bg-surface border border-line text-text text-xs"
      role="dialog"
      aria-modal="true"
      aria-label="Keyboard Shortcuts Reference"
    >
      <!-- Header -->
      <div class="flex items-center justify-between border-b border-line px-4 py-3">
        <div class="flex items-center gap-2">
          <h2 class="m-0 text-sm text-text">Keyboard Shortcuts</h2>
          <span
            class="font-mono text-[10px] text-text-muted bg-surface-hi px-1.5 py-0.5 border border-line"
          >
            {platform.isMac ? "⌘/" : "Ctrl+/"}
          </span>
        </div>
        <button
          type="button"
          class="mng-icon-btn"
          aria-label="Close shortcuts overlay"
          onclick={hide}
        >
          <Icon name="close" size={14} />
        </button>
      </div>

      <!-- Search Filter -->
      <div class="border-b border-line px-4 py-2 bg-surface">
        <input
          bind:this={inputEl}
          bind:value={filterText}
          type="search"
          placeholder="Search shortcuts..."
          aria-label="Search shortcuts"
          class="mng-field w-full"
        />
      </div>

      <!-- Shortcuts Body -->
      <div class="min-h-0 flex-1 overflow-y-auto p-4 space-y-6">
        {#if filteredGroups.length === 0}
          <div class="py-8 text-center text-text-muted">
            No shortcuts matching "{filterText}".
          </div>
        {:else}
          {#each filteredGroups as group (group.name)}
            <div>
              <h3 class="m-0 mb-2 font-mono text-[11px] text-text-60">
                {group.name}
              </h3>
              <div class="divide-y divide-line/40 border border-line bg-surface-hi/40">
                {#each group.entries as entry (entry.id)}
                  <div
                    class="flex items-center justify-between px-3 py-2 hover:bg-surface-hi/80 transition-colors"
                  >
                    <div class="pr-4">
                      <div class="text-text font-normal">{entry.title}</div>
                      {#if entry.description}
                        <div class="text-[11px] text-text-muted mt-0.5">
                          {entry.description}
                        </div>
                      {/if}
                    </div>
                    <kbd
                      class="shrink-0 font-mono text-[11px] px-2 py-0.5 bg-surface border border-line text-text"
                    >
                      {entry.formatted}
                    </kbd>
                  </div>
                {/each}
              </div>
            </div>
          {/each}
        {/if}
      </div>

      <!-- Footer -->
      <div
        class="flex items-center justify-between border-t border-line px-4 py-2.5 bg-surface text-[11px] text-text-muted"
      >
        <span>Customise keybindings in Settings → Keybindings</span>
        <button type="button" class="mng-btn px-2 py-1 text-xs" onclick={hide}>
          Close (Esc)
        </button>
      </div>
    </div>
  </div>
{/if}
