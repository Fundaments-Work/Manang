<script lang="ts">
  import { untrack } from "svelte";

  import { READINESS_LABEL, readiness } from "$lib/agent/readiness";
  import { agent } from "$lib/state/agent.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { settings } from "$lib/state/settings.svelte";
  import { workspace } from "$lib/state/workspace.svelte";

  /**
   * The status-bar chip `AI: ready | not set up | off`. Quiet text like its neighbours; one click
   * opens the Agent column, whose setup card handles the rest. It reads local status only and
   * never contacts a provider.
   */

  const state = $derived(readiness(agent.status));

  // Keep the chip in step with the settings that decide it.
  $effect(() => {
    void settings.value("agent.enabled");
    void settings.value("agent.provider");
    void settings.value("agent.model");
    void settings.value("agent.offline");
    void workspace.project?.root;
    untrack(() => void agent.refreshStatus());
  });
</script>

{#if workspace.project !== null}
  <button
    type="button"
    class="mng-status"
    data-ai-chip={state}
    title={state === "ready"
      ? "Open the AI agent (Ctrl+Shift+A)"
      : state === "off"
        ? "The AI agent is off. Open it to turn it on"
        : "Set up the AI agent"}
    aria-label="{READINESS_LABEL[state]}. {layout.agentOpen
      ? 'Show'
      : 'Open'} the agent"
    onclick={() => agent.reveal()}>{READINESS_LABEL[state]}</button
  >
{/if}

<style>
  .mng-status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 12px;
    color: inherit;
    white-space: nowrap;
  }
  .mng-status:hover {
    color: var(--mng-accent);
  }
</style>
