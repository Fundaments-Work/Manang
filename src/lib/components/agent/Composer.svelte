<script lang="ts">
  import { tick } from "svelte";

  import { tokenLabel } from "$lib/agent/chips";
  import { formatCost } from "$lib/agent/reducer";
  import type { ChipKind } from "$lib/agent/types";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { agent } from "$lib/state/agent.svelte";
  import { tabs } from "$lib/state/tabs.svelte";

  import DropMenu from "./DropMenu.svelte";

  /**
   * The composer, pinned to the bottom of the Agent column: the context chips that will be sent,
   * a text box that grows with what you type (Enter sends, Shift+Enter adds a line), Send that
   * becomes Stop while the agent works, and one quiet line with the size and cost estimate and
   * "Preview what will be sent". "@" or the + button attaches more context.
   */

  interface Props {
    /** Sending is not possible yet (set-up is missing): the box stays but says why. */
    blocked?: boolean;
  }

  const { blocked = false }: Props = $props();

  interface Choice {
    readonly id: string;
    readonly label: string;
    readonly run: () => void;
  }

  const KINDS: readonly { kind: ChipKind; label: string }[] = [
    { kind: "selection", label: "Selection" },
    { kind: "file", label: "Current file" },
    { kind: "diagnostics", label: "Problems" },
    { kind: "outline", label: "Outline" },
    { kind: "bibliography-keys", label: "Bibliography keys" },
    { kind: "figure", label: "Figure" },
    { kind: "pinned", label: "Pinned note" },
  ];

  /** Everything that can be attached: the kinds, then the other open documents. */
  const choices = $derived<readonly Choice[]>([
    ...KINDS.map((k) => ({
      id: k.kind,
      label: k.label,
      run: () => void agent.addChip(k.kind),
    })),
    ...tabs.tabs
      .filter((t) => t.kind === "document" && t.path !== tabs.active?.path)
      .map((t) => ({
        id: `file:${t.path}`,
        label: t.path,
        run: () => agent.addFile(t.path),
      })),
  ]);

  let box: HTMLTextAreaElement | undefined = $state();
  let mention = $state<{ start: number; query: string } | null>(null);
  let mentionAt = $state(0);

  const matches = $derived(
    mention === null
      ? []
      : choices.filter((c) =>
          c.label.toLowerCase().includes(mention!.query.toLowerCase()),
        ),
  );
  const limit = $derived(agent.status?.max_context_tokens ?? 32000);
  const over = $derived(agent.trayTokens > limit);
  const view = $derived(agent.view);
  const modelLine = $derived(
    view.model !== "" ? view.model : (agent.status?.model ?? ""),
  );

  // The box grows with its text, up to about eight lines.
  $effect(() => {
    void agent.input;
    if (box === undefined) return;
    box.style.height = "auto";
    box.style.height = `${Math.min(box.scrollHeight, 168)}px`;
  });

  function trackMention(): void {
    if (box === undefined) return;
    const before = box.value.slice(0, box.selectionStart);
    const m = /(?:^|\s)@([^\s@]*)$/.exec(before);
    mention =
      m === null
        ? null
        : { start: before.length - m[1]!.length - 1, query: m[1] ?? "" };
    mentionAt = 0;
  }

  async function pick(choice: Choice): Promise<void> {
    if (box !== undefined && mention !== null) {
      const end = box.selectionStart;
      agent.input = agent.input.slice(0, mention.start) + agent.input.slice(end);
      const caret = mention.start;
      mention = null;
      await tick();
      box.setSelectionRange(caret, caret);
      box.focus();
    }
    choice.run();
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.isComposing) return;
    if (mention !== null && matches.length > 0) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const step = event.key === "ArrowDown" ? 1 : -1;
        mentionAt = (mentionAt + step + matches.length) % matches.length;
        return;
      }
      if (event.key === "Enter" || event.key === "Tab") {
        event.preventDefault();
        const choice = matches[mentionAt];
        if (choice !== undefined) void pick(choice);
        return;
      }
    }
    if (event.key === "Escape" && mention !== null) {
      event.preventDefault();
      event.stopPropagation();
      mention = null;
      return;
    }
    if (event.key !== "Enter" || event.shiftKey) return;
    event.preventDefault();
    if (!view.running) void agent.send();
  }
</script>

<div class="shrink-0 border-t border-line bg-surface">
  <div
    class="flex flex-wrap items-center gap-1.5 px-3 pt-2"
    role="group"
    aria-label="Context to send"
  >
    {#each agent.tray as chip (chip.id)}
      <span
        class="inline-flex h-6 max-w-full items-center border border-line text-[11px]"
        data-chip={chip.kind}
      >
        <span class="min-w-0 truncate pl-2 text-text" title={chip.label}
          >{chip.label}</span
        >
        <span
          class="px-1.5 font-mono text-[10px] text-text-60"
          aria-label="{chip.tokens} tokens">{tokenLabel(chip.tokens)}</span
        >
        <button
          type="button"
          class="mng-icon-btn h-6 w-6"
          aria-label={chip.pinned === true
            ? `Unpin ${chip.label}`
            : `Pin ${chip.label}`}
          aria-pressed={chip.pinned === true}
          title={chip.pinned === true ? "Pinned: kept between chats" : "Pin"}
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
    <DropMenu
      label="Add context"
      up
      triggerClass="h-6 w-6 justify-center border border-line"
    >
      {#snippet trigger()}<Icon name="plus" size={11} />{/snippet}
      {#snippet children(close)}
        {#each choices as choice (choice.id)}
          <button
            type="button"
            role="menuitem"
            class="inset-focus block w-full truncate px-3 py-1.5 text-left text-xs text-text-60 hover:bg-surface-muted hover:text-text"
            onclick={() => {
              close();
              choice.run();
            }}>{choice.label}</button
          >
        {/each}
      {/snippet}
    </DropMenu>
    {#if agent.tray.length === 0}
      <span class="text-[11px] text-text-muted" data-empty-tray
        >Nothing attached: only your message is sent. Type @ to attach.</span
      >
    {/if}
  </div>

  <div class="relative flex items-end gap-2 px-3 pt-2 pb-1">
    {#if mention !== null && matches.length > 0}
      <div
        role="listbox"
        aria-label="Attach context"
        class="absolute bottom-full left-3 z-30 mb-1 max-h-56 min-w-48 overflow-y-auto border border-line bg-surface-hi py-1"
      >
        {#each matches as choice, i (choice.id)}
          <button
            type="button"
            role="option"
            aria-selected={i === mentionAt}
            tabindex="-1"
            class="block w-full truncate px-3 py-1.5 text-left text-xs {i === mentionAt
              ? 'bg-surface-muted text-text'
              : 'text-text-60'}"
            onmousedown={(e) => {
              e.preventDefault();
              void pick(choice);
            }}>@{choice.label}</button
          >
        {/each}
      </div>
    {/if}
    <textarea
      bind:this={box}
      class="mng-field min-h-[34px] flex-1 resize-none py-1.5 leading-snug"
      rows="1"
      aria-label="Message to the agent"
      placeholder={blocked
        ? "Set up the agent to start chatting"
        : "Ask about your document"}
      bind:value={agent.input}
      {onkeydown}
      oninput={trackMention}
      onclick={trackMention}
      disabled={view.running}
      data-agent-input
    ></textarea>
    {#if view.running}
      <button
        type="button"
        class="mng-btn h-[34px]"
        data-agent-stop
        onclick={() => void agent.stop()}>Stop</button
      >
    {:else}
      <button
        type="button"
        class="mng-btn h-[34px] {agent.pending === undefined && !blocked
          ? 'border-accent bg-accent text-accent-ink'
          : ''}"
        data-agent-send
        disabled={blocked}
        onclick={() => void agent.send()}>Send</button
      >
    {/if}
  </div>
  <p
    class="m-0 flex flex-wrap items-baseline gap-x-3 px-3 pb-2 font-mono text-[10px] text-text-60"
    data-agent-usage
  >
    <span class="min-w-0 truncate"
      >{modelLine !== "" ? modelLine : "no model"} · {view.usage.input +
        view.usage.output} tokens · {formatCost(view.usage.costMicroUsd)} (estimate)</span
    >
    <span
      class={over ? "text-text" : ""}
      data-tray-tokens
      title="About 3.5 characters per token; the provider's count and bill are the truth"
      >~{tokenLabel(agent.trayTokens)} / {tokenLabel(limit)} attached</span
    >
    <button
      type="button"
      class="mng-lime-hover inset-focus font-sans text-[11px] text-text-60"
      data-agent-preview
      onclick={() => void agent.previewRequest()}>Preview what will be sent</button
    >
  </p>
</div>
