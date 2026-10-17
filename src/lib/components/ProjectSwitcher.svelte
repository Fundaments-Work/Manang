<script lang="ts">
  import { tick } from "svelte";
  import { fade } from "svelte/transition";

  import { switcher } from "$lib/state/switcher.svelte";
  import { ENGINE_LABEL } from "$lib/workbench/home";

  /**
   * Project switcher (Ctrl+O): a combobox over the registry, in a modal
   * dialog, like the command palette. Enter opens the highlighted project; the
   * workbench asks about unsaved changes first.
   */

  let input: HTMLInputElement | undefined = $state();
  let returnTo: HTMLElement | null = null;

  $effect(() => {
    if (switcher.open) {
      returnTo =
        document.activeElement instanceof HTMLElement ? document.activeElement : null;
      void tick().then(() => input?.focus());
    }
  });

  function close(): void {
    switcher.hide();
    returnTo?.focus();
  }

  function onkeydown(event: KeyboardEvent): void {
    switch (event.key) {
      case "ArrowDown":
        switcher.move(1);
        break;
      case "ArrowUp":
        switcher.move(-1);
        break;
      case "Home":
        switcher.cursor = 0;
        break;
      case "End":
        switcher.cursor = Math.max(0, switcher.items.length - 1);
        break;
      case "Enter":
        void switcher.choose();
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

  const selectedPath = $derived(switcher.selected?.entry.path);
  const activeId = $derived(
    selectedPath === undefined ? undefined : `switch-opt-${selectedPath}`,
  );

  $effect(() => {
    if (activeId === undefined) return;
    document.getElementById(activeId)?.scrollIntoView({ block: "nearest" });
  });
</script>

{#if switcher.open}
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
      aria-label="Close project switcher"
      onclick={close}
    ></button>

    <div
      role="dialog"
      aria-modal="true"
      aria-label="Switch project"
      class="relative w-[560px] max-w-[94vw] border border-line bg-surface"
    >
      <input
        bind:this={input}
        type="text"
        role="combobox"
        aria-expanded="true"
        aria-controls="switch-list"
        aria-activedescendant={activeId}
        aria-autocomplete="list"
        aria-label="Project"
        placeholder="Switch to a project"
        value={switcher.query}
        oninput={(event) => switcher.setQuery(event.currentTarget.value)}
        spellcheck="false"
        autocomplete="off"
        class="h-11 w-full border-0 border-b border-line bg-surface px-4 text-[13px] text-text outline-none placeholder:text-text-muted"
      />

      <div
        id="switch-list"
        role="listbox"
        aria-label="Projects"
        class="max-h-[52vh] overflow-y-auto py-1"
      >
        {#each switcher.items as item (item.entry.path)}
          {@const active = item.entry.path === selectedPath}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            id="switch-opt-{item.entry.path}"
            role="option"
            aria-selected={active}
            tabindex="-1"
            class="flex h-9 cursor-default items-center gap-3 px-4 text-xs {active
              ? 'bg-surface-hi text-text'
              : 'text-text-60'}"
            onmouseenter={() => (switcher.cursor = switcher.items.indexOf(item))}
            onclick={() => void switcher.choose(item)}
          >
            <span class="min-w-0 shrink-0 truncate text-[13px] text-text"
              >{item.entry.name}</span
            >
            <span class="min-w-0 flex-1 truncate font-mono text-[11px] text-text-muted"
              >{item.entry.path}</span
            >
            {#if item.entry.pinned}
              <span class="text-[11px] text-text-60">Pinned</span>
            {/if}
            <span class="text-[11px] text-text-60"
              >{item.current ? "Open now" : ENGINE_LABEL[item.entry.engine]}</span
            >
          </div>
        {:else}
          <p class="m-0 px-4 py-6 text-center text-xs text-text-60" role="status">
            {switcher.query.trim() === ""
              ? "No other projects yet. Open Home to create one."
              : `Nothing matches "${switcher.query.trim()}".`}
          </p>
        {/each}
      </div>

      <div
        class="flex items-center gap-4 border-t border-line px-4 py-2 text-[11px] text-text-muted"
      >
        <span>Up and Down to move</span>
        <span>Enter to open</span>
        <span>Esc to close</span>
      </div>
    </div>
  </div>
{/if}
