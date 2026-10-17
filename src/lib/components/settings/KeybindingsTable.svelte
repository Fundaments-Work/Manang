<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import type { KeybindingConflict, KeybindingItem } from "$lib/kernel/api";
  import {
    findCommandConflicts,
    formatShortcutDisplay,
    recordShortcutFromEvent,
  } from "$lib/workbench/settings";

  interface Props {
    bindings: readonly KeybindingItem[];
    conflicts: readonly KeybindingConflict[];
    onchange: (command: string, key: string) => Promise<void>;
    onreset: (command: string) => Promise<void>;
  }

  const { bindings, conflicts, onchange, onreset }: Props = $props();

  let query = $state("");
  let recordingCommand = $state<string | null>(null);
  let recordedKey = $state("");

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (q === "") return bindings;
    return bindings.filter(
      (b) =>
        b.command.toLowerCase().includes(q) ||
        b.key.toLowerCase().includes(q) ||
        b.display.toLowerCase().includes(q),
    );
  });

  function startRecording(b: KeybindingItem): void {
    recordingCommand = b.command;
    recordedKey = "";
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (!recordingCommand) return;

    if (event.key === "Escape") {
      event.preventDefault();
      recordingCommand = null;
      return;
    }

    if (event.key === "Enter" && recordedKey !== "") {
      event.preventDefault();
      const cmd = recordingCommand;
      const key = recordedKey;
      recordingCommand = null;
      void onchange(cmd, key);
      return;
    }

    const shortcut = recordShortcutFromEvent(event);
    if (shortcut) {
      event.preventDefault();
      recordedKey = shortcut;
    }
  }
</script>

<svelte:window onkeydown={recordingCommand ? handleKeydown : undefined} />

<div class="space-y-3">
  <!-- Search input -->
  <div class="relative">
    <input
      type="search"
      bind:value={query}
      placeholder="Filter keybindings..."
      class="mng-input w-full pl-8 text-xs"
      aria-label="Filter keybindings"
    />
    <div class="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-text-muted">
      <Icon name="search" size={13} />
    </div>
  </div>

  <!-- Keybindings Table -->
  <div class="border border-line overflow-hidden">
    <div class="grid grid-cols-12 border-b border-line bg-surface-hi/40 px-3 py-1.5 font-mono text-[10px] text-text-muted">
      <div class="col-span-5">COMMAND</div>
      <div class="col-span-3">KEYBINDING</div>
      <div class="col-span-2">SOURCE</div>
      <div class="col-span-2 text-right">ACTIONS</div>
    </div>

    {#if filtered.length === 0}
      <div class="p-4 text-center text-xs text-text-muted">
        No keybindings match "{query}".
      </div>
    {:else}
      <div class="max-h-[460px] overflow-y-auto divide-y divide-line/40">
        {#each filtered as item (item.command)}
          {@const cmdConflicts = findCommandConflicts(item.command, conflicts)}
          <div class="grid grid-cols-12 items-center px-3 py-2 text-xs hover:bg-surface-hi/30">
            <div class="col-span-5 min-w-0 pr-2">
              <div class="truncate font-medium text-text">{item.command}</div>
              {#if item.when}
                <div class="truncate font-mono text-[9px] text-text-muted">{item.when}</div>
              {/if}
              {#if cmdConflicts.length > 0}
                <div class="mt-0.5 text-[10px] text-red-400">
                  Conflict: shares {cmdConflicts[0]?.key} with {cmdConflicts[0]?.commands.filter((c) => c !== item.command).join(", ")}
                </div>
              {/if}
            </div>

            <div class="col-span-3 min-w-0">
              {#if recordingCommand === item.command}
                <div class="inline-flex items-center gap-1.5 border border-accent bg-accent/10 px-2 py-0.5 font-mono text-[11px] text-accent animate-pulse">
                  <span>{recordedKey !== "" ? formatShortcutDisplay(recordedKey) : "Press keys..."}</span>
                </div>
              {:else}
                <span class="mng-kbd">
                  {formatShortcutDisplay(item.key)}
                </span>
              {/if}
            </div>

            <div class="col-span-2 font-mono text-[10px]">
              {#if item.source === "user"}
                <span class="text-accent">User</span>
              {:else}
                <span class="text-text-muted">Default</span>
              {/if}
            </div>

            <div class="col-span-2 flex items-center justify-end gap-1">
              {#if recordingCommand === item.command}
                <button
                  type="button"
                  class="mng-btn px-2 py-0.5 text-[10px]"
                  onclick={() => (recordingCommand = null)}
                >
                  Cancel
                </button>
              {:else}
                <button
                  type="button"
                  class="mng-icon-btn text-text-60 hover:text-text"
                  title="Change shortcut"
                  aria-label={`Change shortcut for ${item.command}`}
                  onclick={() => startRecording(item)}
                >
                  <Icon name="command" size={13} />
                </button>
                {#if item.source === "user"}
                  <button
                    type="button"
                    class="mng-icon-btn text-text-60 hover:text-text"
                    title="Reset to default"
                    aria-label={`Reset shortcut for ${item.command}`}
                    onclick={() => void onreset(item.command)}
                  >
                    <Icon name="refresh" size={13} />
                  </button>
                {/if}
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  {#if recordingCommand}
    <div class="p-2 border border-accent/40 bg-accent/5 text-[11px] text-text-60">
      Press key combination, then press <span class="font-mono font-bold text-text">Enter</span> to apply, or <span class="font-mono font-bold text-text">Esc</span> to cancel.
    </div>
  {/if}
</div>
