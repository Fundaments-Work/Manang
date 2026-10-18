<script lang="ts">
  import { tokenLabel } from "$lib/agent/chips";
  import type { ChipKind } from "$lib/agent/types";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { agent } from "$lib/state/agent.svelte";

  /**
   * The context tray: every chip that will be sent with the next request, with its size, and
   * nothing else. Remove a chip to stop sending it; pin one to keep it between conversations.
   * "Preview what will be sent" shows the exact payload before anything leaves the machine.
   */

  const ADDABLE: readonly { kind: ChipKind; label: string }[] = [
    { kind: "selection", label: "Selection" },
    { kind: "file", label: "Current file" },
    { kind: "diagnostics", label: "Problems" },
    { kind: "outline", label: "Outline" },
    { kind: "bibliography-keys", label: "Bibliography keys" },
    { kind: "figure", label: "Figure" },
    { kind: "pinned", label: "Pinned note" },
  ];

  let adding = $state(false);
  const limit = $derived(agent.status?.max_context_tokens ?? 32000);
  const over = $derived(agent.trayTokens > limit);

  function add(kind: ChipKind): void {
    adding = false;
    void agent.addChip(kind);
  }
</script>

<div class="border-t border-line px-3 py-1.5" aria-label="Context to send" role="group">
  <div class="flex flex-wrap items-center gap-1.5">
    <span class="text-[11px] text-text-60">Context</span>
    {#each agent.tray as chip (chip.id)}
      <span
        class="inline-flex h-6 max-w-[16rem] items-center border border-line bg-surface text-[11px]"
        data-chip={chip.kind}
      >
        <span class="min-w-0 truncate px-2 text-text" title={chip.label}
          >{chip.label}</span
        >
        <span
          class="px-1 font-mono text-[10px] text-text-60"
          aria-label="{chip.tokens} tokens">{tokenLabel(chip.tokens)}</span
        >
        <button
          type="button"
          class="mng-icon-btn h-6 w-6"
          aria-label={chip.pinned === true
            ? `Unpin ${chip.label}`
            : `Pin ${chip.label}`}
          aria-pressed={chip.pinned === true}
          title={chip.pinned === true ? "Pinned: kept between conversations" : "Pin"}
          onclick={() => agent.togglePin(chip.id)}
        >
          <Icon name={chip.pinned === true ? "pin-filled" : "pin"} size={11} />
        </button>
        <button
          type="button"
          class="mng-icon-btn h-6 w-6"
          aria-label="Remove {chip.label}"
          onclick={() => agent.removeChip(chip.id)}
        >
          <Icon name="close" size={10} />
        </button>
      </span>
    {/each}
    {#if agent.tray.length === 0}
      <span class="text-[11px] text-text-muted" data-empty-tray
        >Nothing attached: only your message is sent.</span
      >
    {/if}
    <span class="relative">
      <button
        type="button"
        class="mng-btn h-6 px-2 text-[11px]"
        aria-expanded={adding}
        aria-haspopup="menu"
        onclick={() => (adding = !adding)}>Add</button
      >
      {#if adding}
        <div
          role="menu"
          aria-label="Add context"
          class="absolute bottom-full left-0 z-30 mb-1 w-44 border border-line bg-surface py-1"
        >
          {#each ADDABLE as item (item.kind)}
            <button
              type="button"
              role="menuitem"
              class="inset-focus block h-7 w-full px-3 text-left text-xs text-text-60 hover:bg-surface-hi hover:text-text"
              onclick={() => add(item.kind)}>{item.label}</button
            >
          {/each}
        </div>
      {/if}
    </span>
    <span class="ml-auto flex items-center gap-3">
      <span
        class="font-mono text-[10px] {over ? 'text-text' : 'text-text-60'}"
        data-tray-tokens
        title="About 3.5 characters per token; the provider's count is the truth"
        >~{tokenLabel(agent.trayTokens)} / {tokenLabel(limit)} tokens</span
      >
      <button
        type="button"
        class="mng-lime-hover inset-focus text-[11px] text-text-60"
        data-agent-preview
        onclick={() => void agent.previewRequest()}>Preview what will be sent</button
      >
    </span>
  </div>
</div>
