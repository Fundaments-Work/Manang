<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import type { KeybindingConflict, KeybindingItem } from "$lib/kernel/api";
  import { keybindingRows, type KeybindingRow } from "$lib/workbench/keybindingRows";
  import {
    findCommandConflicts,
    formatShortcutDisplay,
    recordShortcutFromEvent,
  } from "$lib/workbench/settings";

  /**
   * Keyboard shortcuts: every binding the keybindings registry knows, with
   * its source and conflicts. "Change" records the next key combination;
   * Enter applies it and Escape cancels.
   */

  interface Props {
    bindings: readonly KeybindingItem[];
    conflicts: readonly KeybindingConflict[];
    /** Command titles by id, so rows read as actions rather than ids. */
    titles: ReadonlyMap<string, string>;
    query: string;
    onchange: (command: string, key: string) => Promise<void>;
    onreset: (command: string) => Promise<void>;
  }

  const { bindings, conflicts, titles, query, onchange, onreset }: Props = $props();

  let recording = $state<string | null>(null);
  let recorded = $state("");

  const rows = $derived(keybindingRows(bindings));
  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (q === "") return rows;
    return rows.filter(
      (b) =>
        b.command.toLowerCase().includes(q) ||
        (titles.get(b.command) ?? "").toLowerCase().includes(q) ||
        b.display.toLowerCase().includes(q),
    );
  });

  /** The row being recorded (its id), not the command: a command may have several rows. */
  function start(item: KeybindingRow): void {
    recording = item.id;
    recorded = "";
  }

  function onkeydown(event: KeyboardEvent): void {
    if (recording === null) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      recording = null;
      return;
    }
    if (event.key === "Enter" && recorded !== "") {
      event.preventDefault();
      const row = rows.find((r) => r.id === recording);
      recording = null;
      if (row !== undefined) void onchange(row.command, recorded);
      return;
    }
    const shortcut = recordShortcutFromEvent(event);
    if (shortcut !== null) {
      event.preventDefault();
      recorded = shortcut;
    }
  }
</script>

<svelte:window onkeydowncapture={recording !== null ? onkeydown : undefined} />

{#if filtered.length === 0}
  <p class="m-0 py-6 text-xs text-text-60">
    {query.trim() === ""
      ? "No shortcuts are registered."
      : `No shortcut matches “${query.trim()}”.`}
  </p>
{:else}
  <ul class="m-0 list-none p-0" aria-label="Keyboard shortcuts">
    {#each filtered as item (item.id)}
      {@const clashes = findCommandConflicts(item.command, conflicts)}
      {@const title = titles.get(item.command)}
      <li
        class="grid items-center gap-x-6 gap-y-2 border-b border-line-subtle py-3 @xl:grid-cols-[minmax(0,1fr)_12rem_auto]"
      >
        <div class="min-w-0">
          <p class="m-0 truncate text-[13px] text-text">{title ?? item.command}</p>
          <p
            class="selectable m-0 mt-0.5 truncate font-mono text-[11px] text-text-muted"
          >
            {item.command}{item.when !== "" ? `, when ${item.when}` : ""}
          </p>
          {#if clashes.length > 0}
            {@const clash = clashes[0]}
            <p class="m-0 mt-1 flex items-center gap-1.5 text-xs text-text">
              <Icon name="warning" size={12} class="shrink-0" />
              Also bound to {clash?.commands
                .filter((c) => c !== item.command)
                .join(", ")}
            </p>
          {/if}
        </div>

        <div class="flex items-center gap-2">
          {#if recording === item.id}
            <span
              class="inline-flex h-7 items-center border border-accent px-2 font-mono text-xs text-text"
              role="status"
            >
              {recorded !== "" ? formatShortcutDisplay(recorded) : "Press keys"}
            </span>
          {:else}
            <span class="font-mono text-xs text-text"
              >{formatShortcutDisplay(item.key)}</span
            >
            {#if item.source === "user"}
              <span class="text-[11px] text-text-60">Changed</span>
            {/if}
          {/if}
        </div>

        <div class="flex items-center gap-1 @xl:justify-end">
          {#if recording === item.id}
            <span class="mr-2 text-[11px] text-text-60"
              >Enter to apply, Esc to cancel</span
            >
            <button type="button" class="mng-btn" onclick={() => (recording = null)}>
              Cancel
            </button>
          {:else}
            <button
              type="button"
              class="mng-btn"
              aria-label="Change shortcut for {title ?? item.command}"
              onclick={() => start(item)}
            >
              Change
            </button>
            <button
              type="button"
              class="mng-icon-btn {item.source === 'user' ? '' : 'invisible'}"
              title="Reset to default"
              aria-label="Reset shortcut for {title ?? item.command}"
              tabindex={item.source === "user" ? 0 : -1}
              onclick={() => void onreset(item.command)}
            >
              <Icon name="undo" size={14} />
            </button>
          {/if}
        </div>
      </li>
    {/each}
  </ul>
{/if}
