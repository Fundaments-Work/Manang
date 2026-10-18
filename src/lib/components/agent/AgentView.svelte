<script lang="ts">
  import { onMount, tick } from "svelte";

  import { formatCost } from "$lib/agent/reducer";
  import { agent } from "$lib/state/agent.svelte";
  import { layout } from "$lib/state/layout.svelte";

  import ContextTray from "./ContextTray.svelte";
  import PreviewDialog from "./PreviewDialog.svelte";
  import ProposalCard from "./ProposalCard.svelte";

  /**
   * The Agent view, a tab of the bottom panel: the conversation, the context tray, one input.
   * Nothing is sent until Send is pressed (or a selection action or "Fix with AI" is chosen).
   * One lime action at a time: Accept while a proposal waits, otherwise Send.
   */

  let log: HTMLElement | undefined = $state();

  /** Room for the conversation, the tray and the input. */
  const MIN_PANEL_HEIGHT = 340;

  onMount(() => {
    if (layout.panelHeight < MIN_PANEL_HEIGHT) layout.setPanelHeight(MIN_PANEL_HEIGHT);
    agent.listen();
    agent.attachSelection();
    void agent.refreshStatus();
  });

  // The conversation follows the newest line.
  $effect(() => {
    void agent.view.items;
    if (log === undefined) return;
    void tick().then(() => {
      if (log !== undefined) log.scrollTop = log.scrollHeight;
    });
  });

  const status = $derived(agent.status);
  const view = $derived(agent.view);
  const notReady = $derived(status !== null && !status.ready);
  /** Only the first-use consent is missing: it is asked for at the first Send. */
  const consentOnly = $derived(
    status !== null &&
      status.problems.length > 0 &&
      status.problems.every((p) => p === "consent_required"),
  );

  const SETUP_TEXT: Record<string, string> = {
    disabled: "The AI agent is turned off.",
    policy: "This project's policy does not allow this provider.",
    key_missing: "No API key is saved for this provider.",
    model_missing: "No model is chosen.",
    offline_blocked: "Offline mode only allows a provider on this machine.",
    consent_required: "Agree to send data to the provider before the first request.",
  };

  const setupText = $derived(
    agent.setup !== null
      ? (SETUP_TEXT[agent.setup] ?? "Set up the provider first.")
      : status?.problems[0] !== undefined
        ? (SETUP_TEXT[status.problems[0]] ?? "")
        : "",
  );

  function onInputKey(event: KeyboardEvent): void {
    if (event.key !== "Enter" || event.shiftKey || event.isComposing) return;
    event.preventDefault();
    void agent.send();
  }

  const modelLine = $derived(view.model !== "" ? view.model : (status?.model ?? ""));
</script>

<div class="flex h-full min-h-0 flex-col" data-agent-view>
  <div
    class="flex h-8 shrink-0 items-center gap-3 border-b border-line px-3 text-[11px]"
  >
    <span class="text-text" data-agent-status>
      AI: {#if status === null}checking{:else if status.ready}ready{:else if consentOnly}ready,
        asks for consent at the first Send{:else if !status.enabled}off{:else}not set up{/if}
    </span>
    {#if status !== null}
      <span class="min-w-0 truncate text-text-60"
        >{status.provider_name}{status.model !== null ? `, ${status.model}` : ""}</span
      >
      {#if status.mock === true}
        <span class="text-text-60" data-agent-mock
          >Mock provider (browser): nothing is sent anywhere</span
        >
      {/if}
    {/if}
    {#if notReady && !consentOnly}
      <button
        type="button"
        class="mng-btn h-6 px-2 text-[11px]"
        data-agent-setup
        onclick={() => agent.openSetup()}>Set up</button
      >
    {/if}
    <span class="ml-auto"></span>
    <button
      type="button"
      class="mng-lime-hover inset-focus text-[11px] text-text-60"
      disabled={view.items.length === 0 || view.running}
      onclick={() => agent.newConversation()}>New conversation</button
    >
  </div>

  <div
    bind:this={log}
    role="log"
    aria-label="Agent conversation"
    aria-live="polite"
    class="selectable min-h-0 flex-1 overflow-y-auto px-4 py-2 text-xs leading-relaxed"
    data-agent-log
  >
    {#if view.items.length === 0}
      <p class="m-0 text-text-60">
        {#if notReady && setupText !== ""}
          {setupText} Press Set up to choose a provider and add your key.
        {:else}
          Ask about your document, or select text and use Edit, AI. Nothing is sent
          until you press Send.
        {/if}
      </p>
    {/if}
    {#each view.items as item, i (i)}
      {#if item.kind === "user"}
        <div class="mt-2">
          <p class="m-0 text-[11px] text-text-60">You</p>
          <p class="m-0 whitespace-pre-wrap text-text">{item.text}</p>
          {#if item.chips.length > 0}
            <p class="m-0 text-[11px] text-text-muted">With: {item.chips.join(", ")}</p>
          {/if}
        </div>
      {:else if item.kind === "assistant"}
        <div class="mt-2" data-agent-answer>
          {#if item.thinking !== ""}
            <p
              class="m-0 text-[11px] whitespace-pre-wrap text-text-60"
              data-agent-thinking
            >
              Reasoning: {item.thinking}
            </p>
          {/if}
          {#if item.text !== ""}
            <p class="m-0 whitespace-pre-wrap text-text">{item.text}</p>
          {/if}
        </div>
      {:else if item.kind === "tool"}
        <p class="m-0 mt-1 font-mono text-[11px] text-text-60" data-agent-tool>
          {item.state === "running"
            ? "Running"
            : item.state === "error"
              ? "Failed"
              : "Used"}
          {item.name}{item.summary !== "" ? `: ${item.summary}` : ""}
        </p>
      {:else if item.kind === "approval"}
        <div class="my-2 border border-line px-2.5 py-2" data-agent-approval>
          <p class="m-0 text-text">
            Allow {item.tool} ({item.risk})?
            {#if item.afterUntrusted}
              <span class="text-text-60"
                >This session read content from outside the project, so each network
                call asks.</span
              >
            {/if}
          </p>
          {#if item.answer === "pending"}
            <div class="mt-1.5 flex gap-2">
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
                >Allow for this session</button
              >
              <button
                type="button"
                class="mng-btn"
                onclick={() => void agent.approve(item.callId, "deny")}>Deny</button
              >
            </div>
          {:else}
            <p class="m-0 text-[11px] text-text-60">
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
          />
        {/if}
      {:else if item.kind === "error"}
        <p class="m-0 mt-2 text-text" role="alert" data-agent-error>
          {item.message}
          {#if item.requestId !== null}
            <span class="font-mono text-[11px] text-text-60"
              >(request id {item.requestId})</span
            >
          {/if}
          {#if item.retryable}
            <span class="text-text-60">You can try again.</span>
          {/if}
        </p>
      {:else}
        <p class="m-0 mt-1 text-text-60" data-agent-notice>{item.text}</p>
      {/if}
    {/each}
  </div>

  {#if notReady && agent.setup !== null}
    <p
      class="m-0 border-t border-line px-4 py-1.5 text-xs text-text"
      role="status"
      data-agent-blocked
    >
      {setupText}
    </p>
  {/if}

  <ContextTray />

  <div class="flex items-end gap-2 border-t border-line px-3 py-2">
    <textarea
      class="mng-field min-h-[34px] flex-1 resize-none py-1.5 leading-snug"
      style="height: 34px"
      rows="1"
      aria-label="Message to the agent"
      placeholder="Ask about your document"
      bind:value={agent.input}
      onkeydown={onInputKey}
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
        class="mng-btn h-[34px] {agent.pending === undefined
          ? 'border-accent bg-accent text-accent-ink'
          : ''}"
        data-agent-send
        onclick={() => void agent.send()}>Send</button
      >
    {/if}
  </div>
  <p
    class="m-0 border-t border-line px-3 py-1 font-mono text-[10px] text-text-60"
    data-agent-usage
  >
    {modelLine !== "" ? modelLine : "no model"} · {view.usage.input + view.usage.output} tokens
    ({view.usage.input} in, {view.usage.output} out) · {formatCost(
      view.usage.costMicroUsd,
    )} (estimate, the provider's bill is the truth)
  </p>
</div>

<PreviewDialog />
