<script lang="ts">
  import { tick } from "svelte";
  import { fade } from "svelte/transition";

  import { formatShortcut } from "$lib/commands/shortcut";
  import { palette } from "$lib/state/palette.svelte";
  import { detectPlatform } from "$lib/util/keyboard";

  /**
   * Command palette (WAI-ARIA combobox with a listbox popup, in a modal dialog).
   * Focus stays on the input and the highlighted row is named by
   * `aria-activedescendant`. It is a shortcut: every command here is also
   * reachable from a panel, the top bar or a keyboard shortcut.
   */

  const platform = detectPlatform();
  let input: HTMLInputElement | undefined = $state();
  let returnTo: HTMLElement | null = null;

  $effect(() => {
    if (palette.open) {
      returnTo =
        document.activeElement instanceof HTMLElement ? document.activeElement : null;
      void tick().then(() => input?.focus());
    }
  });

  function close(): void {
    palette.hide();
    returnTo?.focus();
  }

  function onkeydown(event: KeyboardEvent): void {
    switch (event.key) {
      case "ArrowDown":
        palette.move(1);
        break;
      case "ArrowUp":
        palette.move(-1);
        break;
      case "Home":
        palette.cursor = 0;
        break;
      case "End":
        palette.cursor = Math.max(0, palette.flat.length - 1);
        break;
      case "Enter":
        palette.execute();
        break;
      case "Escape":
        close();
        break;
      case "Tab":
        break;
      default:
        return;
    }
    event.preventDefault();
    event.stopPropagation();
  }

  $effect(() => {
    // Keep the highlighted row visible while arrowing through a long list.
    const id = palette.selected?.id;
    if (id === undefined) return;
    document
      .getElementById(`palette-opt-${CSS.escape(id)}`)
      ?.scrollIntoView({ block: "nearest" });
  });

  const activeId = $derived(
    palette.selected === undefined ? undefined : `palette-opt-${palette.selected.id}`,
  );
</script>

{#if palette.open}
  <div
    class="fixed inset-0 z-40 flex items-start justify-center pt-[16vh]"
    transition:fade={{ duration: 100 }}
    role="presentation"
    {onkeydown}
  >
    <button
      type="button"
      tabindex="-1"
      class="absolute inset-0 cursor-default"
      aria-label="Close command palette"
      onclick={close}
    ></button>

    <div
      role="dialog"
      aria-modal="true"
      aria-label={palette.mode === "files" ? "Go to file" : "Command palette"}
      class="relative w-[600px] max-w-[94vw] border border-line bg-surface"
    >
      <input
        bind:this={input}
        type="text"
        role="combobox"
        aria-expanded="true"
        aria-controls="palette-list"
        aria-activedescendant={activeId}
        aria-autocomplete="list"
        aria-label={palette.mode === "files" ? "File name" : "Command"}
        placeholder={palette.mode === "files"
          ? "Go to file by name"
          : "Type a command, or a file name to open it"}
        value={palette.query}
        oninput={(event) => palette.setQuery(event.currentTarget.value)}
        spellcheck="false"
        autocomplete="off"
        class="h-11 w-full border-0 border-b border-line bg-surface px-4 text-[13px] text-text outline-none placeholder:text-text-muted"
      />

      <div
        id="palette-list"
        role="listbox"
        aria-label="Results"
        class="max-h-[52vh] overflow-y-auto py-1"
      >
        {#each palette.groups as group (group.group)}
          <div role="group" aria-label={group.group}>
            <p class="m-0 px-4 pt-2.5 pb-1 text-[11px] text-text-60">{group.group}</p>
            {#each group.entries as entry (entry.item.id)}
              {@const command = entry.item}
              {@const active = command.id === palette.selected?.id}
              {@const disabled = command.enabled === false}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                id="palette-opt-{command.id}"
                role="option"
                aria-selected={active}
                aria-disabled={disabled}
                tabindex="-1"
                class="flex h-8 cursor-default items-center gap-3 px-4 text-xs {active
                  ? 'bg-surface-hi'
                  : ''} {disabled
                  ? 'text-text-muted'
                  : active
                    ? 'text-text'
                    : 'text-text-60'}"
                onmouseenter={() => (palette.cursor = palette.flat.indexOf(command))}
                onclick={() => palette.execute(command)}
              >
                <span class="min-w-0 truncate">{command.title}</span>
                {#if command.hint !== undefined && command.hint !== ""}
                  <span
                    class="min-w-0 flex-1 truncate font-mono text-[10px] text-text-muted"
                    >{command.hint}</span
                  >
                {:else}
                  <span class="flex-1"></span>
                {/if}
                {#if command.shortcuts !== undefined && command.shortcuts[0] !== undefined}
                  <span class="mng-kbd"
                    >{formatShortcut(command.shortcuts[0], platform)}</span
                  >
                {/if}
              </div>
            {/each}
          </div>
        {:else}
          <p class="m-0 px-4 py-6 text-center text-xs text-text-60" role="status">
            Nothing matches "{palette.query.trim()}". Try fewer letters, or search for a
            file name.
          </p>
        {/each}
      </div>

      <div
        class="flex items-center gap-4 border-t border-line px-4 py-2 text-[11px] text-text-muted"
      >
        <span>Up and Down to move</span>
        <span>Enter to run</span>
        <span>Esc to close</span>
      </div>
    </div>
  </div>
{/if}
