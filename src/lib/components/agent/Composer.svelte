<script lang="ts">
  import { tick } from "svelte";

  import { mentionLabel } from "$lib/agent/context";
  import { fillPercent } from "$lib/agent/reducer";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { agent } from "$lib/state/agent.svelte";
  import { build } from "$lib/state/build.svelte";
  import { tabs } from "$lib/state/tabs.svelte";

  /**
   * The one composer, pinned to the bottom of the Agent column: a text box that grows with what
   * you type (Enter sends, Shift+Enter adds a line), Send that becomes Stop while the agent
   * works, and one quiet line under it with what travels along ("main.typ, selection (38
   * words)"). "/" lists the slash commands, "@" mentions a file, the problems or the figure.
   * The fill of the working window shows only once it is past half.
   */

  interface Props {
    /** Sending is not possible yet (set-up is missing): the box stays but says why. */
    blocked?: boolean;
  }

  const { blocked = false }: Props = $props();

  interface Choice {
    readonly id: string;
    readonly label: string;
    readonly hint?: string;
    readonly run: () => void;
  }

  /** What `@` can mention: the other open documents, the problems, the figure. */
  const mentionChoices = $derived<readonly Choice[]>([
    ...tabs.tabs
      .filter((t) => t.kind === "document")
      .map((t) => ({
        id: `file:${t.path}`,
        label: t.path,
        run: () => agent.mentionFile(t.path),
      })),
    ...(build.problems.some((p) => p.file !== "")
      ? [{ id: "problems", label: "problems", run: () => agent.mentionProblems() }]
      : []),
    ...(tabs.active?.kind === "figure"
      ? [{ id: "figure", label: "figure", run: () => agent.mentionFigure() }]
      : []),
  ]);

  let box: HTMLTextAreaElement | undefined = $state();
  let mention = $state<{ start: number; query: string } | null>(null);
  let at = $state(0);

  const mentionMatches = $derived(
    mention === null
      ? []
      : mentionChoices.filter((c) =>
          c.label.toLowerCase().includes(mention!.query.toLowerCase()),
        ),
  );
  const slashChoices = $derived<readonly Choice[]>(
    agent.slashMenu.map((c) => ({
      id: c.name,
      label: `/${c.name}`,
      hint: c.hint === "" ? c.description : `${c.hint}  ${c.description}`,
      run: () => agent.fillCommand(c.name),
    })),
  );
  const menu = $derived<readonly Choice[]>(
    slashChoices.length > 0 ? slashChoices : mentionMatches,
  );
  const view = $derived(agent.view);
  const fill = $derived(fillPercent(view.usage));

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
    at = 0;
  }

  async function pick(choice: Choice): Promise<void> {
    if (box !== undefined && mention !== null && slashChoices.length === 0) {
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
    if (menu.length > 0) {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const step = event.key === "ArrowDown" ? 1 : -1;
        at = (at + step + menu.length) % menu.length;
        return;
      }
      if (event.key === "Tab" || (event.key === "Enter" && slashChoices.length > 0)) {
        event.preventDefault();
        const choice = menu[at];
        if (choice !== undefined) void pick(choice);
        return;
      }
      if (event.key === "Enter" && mention !== null) {
        event.preventDefault();
        const choice = menu[at];
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
  <div class="relative flex items-end gap-2 px-3 pt-2.5 pb-1">
    {#if menu.length > 0}
      <div
        role="listbox"
        aria-label={slashChoices.length > 0 ? "Commands" : "Mention"}
        class="absolute bottom-full left-3 z-30 mb-1 max-h-56 min-w-56 max-w-[calc(100%-1.5rem)] overflow-y-auto border border-line bg-surface-hi py-1"
        data-agent-menu
      >
        {#each menu as choice, i (choice.id)}
          <button
            type="button"
            role="option"
            aria-selected={i === at}
            tabindex="-1"
            class="flex w-full items-baseline justify-between gap-3 px-3 py-1.5 text-left text-xs {i ===
            at
              ? 'bg-surface-muted text-text'
              : 'text-text-60'}"
            onmousedown={(e) => {
              e.preventDefault();
              void pick(choice);
            }}
          >
            <span class="truncate {slashChoices.length > 0 ? 'font-mono' : ''}"
              >{slashChoices.length > 0 ? "" : "@"}{choice.label}</span
            >
            {#if choice.hint !== undefined}
              <span class="truncate text-[11px] text-text-muted">{choice.hint}</span>
            {/if}
          </button>
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
        : "Ask about your document, or type / for commands"}
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
    class="m-0 flex flex-wrap items-center gap-x-2 gap-y-0.5 px-3 pb-2 text-[11px] text-text-muted"
    data-agent-context
  >
    <span class="min-w-0 truncate" data-agent-context-text>
      {agent.contextLine === "" ? "Only your message is sent" : agent.contextLine}
    </span>
    {#each agent.mentions as m, i (i)}
      <button
        type="button"
        class="mng-lime-hover inset-focus inline-flex items-center gap-0.5 text-text-60"
        aria-label="Remove {mentionLabel(m)}"
        data-agent-mention={m.kind}
        onclick={() => agent.removeMention(i)}
      >
        {mentionLabel(m)}
        <Icon name="close" size={9} />
      </button>
    {/each}
    {#if fill >= 50}
      <span
        class="ml-auto font-mono text-[10px] {fill >= 85 ? 'text-text' : ''}"
        data-agent-fill
        title="How much of the working window this chat fills. Old tool results are cleared at 60% and the early part is summarised at 85%."
        >{fill}% of the window</span
      >
    {/if}
  </p>
</div>
