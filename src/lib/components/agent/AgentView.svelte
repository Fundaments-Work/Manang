<script lang="ts">
  import { onMount, tick } from "svelte";

  import { groupLine, rowLine } from "$lib/agent/toolLines";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { agent } from "$lib/state/agent.svelte";

  import AgentHeader from "./AgentHeader.svelte";
  import Composer from "./Composer.svelte";
  import MarkdownText from "./MarkdownText.svelte";
  import PreviewDialog from "./PreviewDialog.svelte";
  import ProposalCard from "./ProposalCard.svelte";
  import SetupCard from "./SetupCard.svelte";

  /**
   * The Agent column: a quiet conversation. A thin header, the conversation, one composer pinned
   * below. Chrome appears only when something needs attention: an approval, a question, a
   * proposal, an error. Tool calls fold into one row; reasoning is a single "Thinking" line.
   * One lime action at a time: Accept while a proposal waits, otherwise Send.
   */

  let log: HTMLElement | undefined = $state();
  let stick = true;
  /** Tool groups (by item index) the person expanded. */
  let expanded = $state<Record<number, boolean>>({});

  onMount(() => {
    agent.listen();
    void agent.refreshStatus();
    void agent.loadCommands();
    void tick().then(() => {
      if (agent.ready)
        document.querySelector<HTMLElement>("[data-agent-input]")?.focus();
    });
  });

  // The conversation follows the newest line, unless the person scrolled up to read.
  $effect(() => {
    const count = agent.view.items.length;
    void agent.view.thinking;
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
          Ask about your document. Select text and type a command, or @ to mention a
          file. Nothing is sent until you press Send.
        </p>
        <ul class="m-0 mt-4 list-none border-t border-line p-0">
          {#each agent.commands as c (c.name)}
            <li class="border-b border-line">
              <button
                type="button"
                class="mng-lime-hover inset-focus flex w-full items-baseline justify-between gap-3 py-2 text-left text-xs text-text"
                data-suggest={c.name}
                onclick={() => agent.fillCommand(c.name)}
              >
                <span class="font-mono"
                  >/{c.name}{c.hint === "" ? "" : ` ${c.hint}`}</span
                >
                <span class="text-[11px] text-text-muted">{c.description}</span>
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
              {#if item.context !== ""}
                <p class="m-0 mt-1 text-[11px] text-text-muted">{item.context}</p>
              {/if}
            </div>
          </div>
        {:else if item.kind === "assistant"}
          <div class="mt-4" data-agent-answer>
            {#if before === undefined || before.kind === "user" || before.kind === "notice"}
              <p class="m-0 mb-1 text-[11px] text-text-60">Assistant</p>
            {/if}
            {#if item.text !== "" || (item.open && view.running)}
              <MarkdownText text={item.text} streaming={item.open && view.running} />
            {/if}
          </div>
        {:else if item.kind === "tools"}
          {@const running = item.rows.some((r) => r.status === "running")}
          {@const failed = item.rows.some((r) => r.status === "error")}
          <div class="mt-1.5" data-agent-tool data-tools-running={running}>
            <button
              type="button"
              class="mng-lime-hover inset-focus flex w-full items-center gap-1.5 text-left text-[11px] {failed
                ? 'text-text'
                : 'text-text-60'}"
              aria-expanded={expanded[i] === true}
              onclick={() => toggle(i)}
            >
              <Icon
                name={expanded[i] === true ? "chevron-down" : "chevron-right"}
                size={10}
              />
              <span class="min-w-0 truncate">{groupLine(item.rows)}</span>
            </button>
            {#if expanded[i] === true}
              <ul class="m-0 mt-1 ml-4 list-none p-0 text-[11px] text-text-60">
                {#each item.rows as row (row.id)}
                  <li class="py-0.5">
                    <span>{rowLine(row)}</span>
                    {#if row.status === "error" && row.summary !== ""}
                      <span class="text-text"> — {row.summary}</span>
                    {/if}
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        {:else if item.kind === "ask"}
          <div class="mt-3" data-agent-ask>
            <p class="m-0 text-[13px] leading-relaxed text-text">{item.question}</p>
            {#if item.answer === null}
              {#if item.choices.length > 0}
                <div class="mt-2 flex flex-wrap gap-2">
                  {#each item.choices as choice (choice)}
                    <button
                      type="button"
                      class="mng-btn"
                      data-agent-ask-choice
                      disabled={view.running}
                      onclick={() => void agent.answer(i, choice)}>{choice}</button
                    >
                  {/each}
                </div>
              {/if}
            {:else}
              <p class="m-0 mt-1 text-[11px] text-text-60">
                You answered: {item.answer}
              </p>
            {/if}
          </div>
        {:else if item.kind === "approval"}
          <div class="my-2 border border-line px-3 py-2" data-agent-approval>
            <p class="m-0 text-xs text-text">
              Allow {item.tool.replace(/_/g, " ")}{item.host === null
                ? ""
                : ` (${item.host})`}?
              {#if item.afterUntrusted}
                <span class="text-text-60">
                  This chat read content from outside the project, so each call asks.
                </span>
              {/if}
            </p>
            {#if item.answer === "pending"}
              <div class="mt-2 flex flex-wrap gap-2">
                <button
                  type="button"
                  class="mng-btn"
                  data-agent-allow
                  onclick={() => void agent.approve(item.callId, "allow")}>Allow</button
                >
                {#if !item.afterUntrusted}
                  <button
                    type="button"
                    class="mng-btn"
                    onclick={() => void agent.approve(item.callId, "allow_session")}
                    >Allow for this chat</button
                  >
                {/if}
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
              onreview={() => void agent.review(item.proposalId)}
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
            {#if i === view.items.length - 1 && !view.running && item.retryable}
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

      {#if view.running && view.thinking}
        <p class="m-0 mt-3 text-[11px] text-text-muted" data-agent-thinking>
          Thinking…
        </p>
      {/if}

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
