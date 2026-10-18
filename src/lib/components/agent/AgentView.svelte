<script lang="ts">
  import { onMount, tick } from "svelte";

  import { toolLine } from "$lib/agent/readiness";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { agent } from "$lib/state/agent.svelte";

  import AgentHeader from "./AgentHeader.svelte";
  import Composer from "./Composer.svelte";
  import MarkdownText from "./MarkdownText.svelte";
  import PreviewDialog from "./PreviewDialog.svelte";
  import ProposalCard from "./ProposalCard.svelte";
  import SetupCard from "./SetupCard.svelte";

  /**
   * The Agent column: a chat. A header (name, model, History, New chat), the conversation, and
   * the composer pinned below. Nothing is sent until Send is pressed (or a selection action or
   * "Fix with AI" is chosen). One lime action at a time: Accept while a proposal waits,
   * otherwise Send; while the agent is not set up, the setup card holds it.
   */

  let log: HTMLElement | undefined = $state();
  let stick = true;
  /** Rows (by index) the person expanded: tool details and reasoning. */
  let expanded = $state<Record<number, boolean>>({});

  onMount(() => {
    agent.listen();
    agent.attachSelection();
    void agent.refreshStatus();
    void tick().then(() => {
      if (agent.ready)
        document.querySelector<HTMLElement>("[data-agent-input]")?.focus();
    });
  });

  // The conversation follows the newest line, unless the person scrolled up to read.
  $effect(() => {
    const count = agent.view.items.length;
    if (log === undefined) return;
    if (count === 0) {
      log.scrollTop = 0;
      return;
    }
    void tick().then(() => {
      if (log !== undefined && stick) log.scrollTop = log.scrollHeight;
    });
  });

  function onscroll(): void {
    if (log === undefined) return;
    stick = log.scrollHeight - log.scrollTop - log.clientHeight < 48;
  }

  function toggle(index: number): void {
    expanded = { ...expanded, [index]: expanded[index] !== true };
  }

  const status = $derived(agent.status);
  const view = $derived(agent.view);
  const needsSetup = $derived(status !== null && !status.ready);
  const pendingCount = $derived(
    view.items.filter((i) => i.kind === "proposal" && i.state === "pending").length,
  );

  const SUGGESTIONS = [
    {
      id: "rewrite",
      title: "Rewrite selection",
      hint: "Select text in the editor first",
    },
    { id: "error", title: "Explain this error", hint: "Uses the current problems" },
    {
      id: "summarise",
      title: "Summarise this section",
      hint: "The selection or the file",
    },
    { id: "caption", title: "Draft a figure caption", hint: "From the open figure" },
  ] as const;
</script>

<div class="flex h-full min-h-0 flex-col" data-agent-view>
  <AgentHeader />

  {#if status?.mock === true}
    <p
      class="m-0 border-b border-line px-3 py-1 text-[11px] text-text-60"
      data-agent-mock
    >
      Mock provider (browser): nothing is sent anywhere.
    </p>
  {/if}

  <div
    bind:this={log}
    role="log"
    aria-label="Agent conversation"
    aria-live="polite"
    class="selectable min-h-0 flex-1 overflow-y-auto"
    data-agent-log
    {onscroll}
  >
    {#if needsSetup}
      <SetupCard />
    {/if}

    {#if view.items.length === 0 && !needsSetup && status !== null}
      <div class="px-4 pt-6" data-agent-empty>
        <h3 class="m-0 text-[15px] font-light text-text">How can I help?</h3>
        <p class="m-0 mt-1 text-xs leading-relaxed text-text-60">
          Ask about your document, or pick one below. Nothing is sent until you press
          Send.
        </p>
        <ul class="m-0 mt-4 list-none border-t border-line p-0">
          {#each SUGGESTIONS as s (s.id)}
            <li class="border-b border-line">
              <button
                type="button"
                class="mng-lime-hover inset-focus flex w-full items-baseline justify-between gap-3 py-2.5 text-left text-xs text-text"
                data-suggest={s.id}
                onclick={() => void agent.suggest(s.id)}
              >
                <span>{s.title}</span>
                <span class="text-[11px] text-text-muted">{s.hint}</span>
              </button>
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    <div class="px-4 pb-3">
      {#each view.items as item, i (i)}
        {@const before = view.items[i - 1]}
        {#if item.kind === "user"}
          <div class="mt-4">
            <p class="m-0 mb-1 text-[11px] text-text-60">You</p>
            <div class="border border-line bg-surface-hi px-3 py-2" data-agent-user>
              <p class="m-0 text-[13px] leading-relaxed whitespace-pre-wrap text-text">
                {item.text}
              </p>
              {#if item.chips.length > 0}
                <p class="m-0 mt-1 text-[11px] text-text-muted">
                  With: {item.chips.join(", ")}
                </p>
              {/if}
            </div>
          </div>
        {:else if item.kind === "assistant"}
          <div class="mt-4" data-agent-answer>
            {#if before === undefined || before.kind === "user" || before.kind === "notice"}
              <p class="m-0 mb-1 text-[11px] text-text-60">Assistant</p>
            {/if}
            {#if item.thinking !== ""}
              <button
                type="button"
                class="mng-lime-hover inset-focus mb-1 flex items-center gap-1 text-[11px] text-text-60"
                aria-expanded={expanded[i] === true}
                data-agent-thinking-toggle
                onclick={() => toggle(i)}
              >
                <Icon
                  name={expanded[i] === true ? "chevron-down" : "chevron-right"}
                  size={10}
                />
                Thinking
              </button>
              {#if expanded[i] === true}
                <p
                  class="m-0 mb-2 border-l border-line pl-3 text-[11px] leading-relaxed whitespace-pre-wrap text-text-60"
                  data-agent-thinking
                >
                  {item.thinking}
                </p>
              {/if}
            {/if}
            {#if item.text !== "" || (item.open && view.running)}
              <MarkdownText text={item.text} streaming={item.open && view.running} />
            {/if}
          </div>
        {:else if item.kind === "tool"}
          <div class="mt-1.5" data-agent-tool>
            <button
              type="button"
              class="mng-lime-hover inset-focus flex w-full items-center gap-1.5 text-left text-[11px] text-text-60"
              aria-expanded={expanded[i] === true}
              onclick={() => toggle(i)}
            >
              <Icon
                name={expanded[i] === true ? "chevron-down" : "chevron-right"}
                size={10}
              />
              <span class="min-w-0 truncate"
                >{item.state === "running" ? "Working: " : ""}{toolLine(
                  item.name,
                  item.summary,
                  item.state,
                )}</span
              >
            </button>
            {#if expanded[i] === true}
              <dl
                class="m-0 mt-1 ml-4 grid grid-cols-[auto_1fr] gap-x-3 text-[11px] text-text-60"
              >
                <dt>Tool</dt>
                <dd class="m-0 font-mono">{item.name}</dd>
                <dt>Access</dt>
                <dd class="m-0">{item.risk || "read"}</dd>
                <dt>Result</dt>
                <dd class="m-0">
                  {item.state === "running"
                    ? "Running"
                    : item.state === "error"
                      ? "Failed"
                      : "Done"}{item.summary !== "" ? `: ${item.summary}` : ""}
                </dd>
              </dl>
            {/if}
          </div>
        {:else if item.kind === "approval"}
          <div class="my-3 border border-line px-3 py-2" data-agent-approval>
            <p class="m-0 text-xs text-text">
              Allow {item.tool} ({item.risk})?
              {#if item.afterUntrusted}
                <span class="text-text-60"
                  >This chat read content from outside the project, so each network call
                  asks.</span
                >
              {/if}
            </p>
            {#if item.answer === "pending"}
              <div class="mt-2 flex flex-wrap gap-2">
                <button
                  type="button"
                  class="mng-btn"
                  onclick={() => void agent.approve(item.callId, "allow")}
                  >Allow once</button
                >
                <button
                  type="button"
                  class="mng-btn"
                  onclick={() => void agent.approve(item.callId, "allow_session")}
                  >Allow for this chat</button
                >
                <button
                  type="button"
                  class="mng-btn"
                  onclick={() => void agent.approve(item.callId, "deny")}>Deny</button
                >
              </div>
            {:else}
              <p class="m-0 mt-1 text-[11px] text-text-60">
                {item.answer === "allowed" ? "Allowed." : "Denied."}
              </p>
            {/if}
          </div>
        {:else if item.kind === "proposal"}
          {#if item.edit !== null}
            <ProposalCard
              edit={item.edit}
              state={item.state}
              onaccept={() => void agent.accept(item.proposalId)}
              onreject={() => void agent.reject(item.proposalId)}
              onundo={() => agent.undoAccepted(item.proposalId)}
            />
          {/if}
        {:else if item.kind === "error"}
          <div class="mt-3 border border-line px-3 py-2" role="alert" data-agent-error>
            <p class="m-0 text-xs text-text">
              {item.message}
              {#if item.requestId !== null}
                <span class="font-mono text-[11px] text-text-60"
                  >(request id {item.requestId})</span
                >
              {/if}
            </p>
            {#if i === view.items.length - 1 && !view.running}
              <button
                type="button"
                class="mng-btn mt-2"
                data-agent-retry
                onclick={() => void agent.retry()}>Retry</button
              >
            {/if}
          </div>
        {:else}
          <p class="m-0 mt-3 text-[11px] text-text-60" data-agent-notice>{item.text}</p>
        {/if}
      {/each}

      {#if pendingCount > 1}
        <button
          type="button"
          class="mng-btn mt-1"
          data-agent-apply-all
          onclick={() => void agent.acceptAll()}>Apply all ({pendingCount})</button
        >
      {/if}
    </div>
  </div>

  <Composer blocked={needsSetup} />
</div>

<PreviewDialog />
