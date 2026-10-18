<script lang="ts">
  import Resizer from "$lib/components/ui/Resizer.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { AGENT_MAX, AGENT_MIN, DEFAULT_LAYOUT } from "$lib/workbench/layout";

  import AgentView from "./AgentView.svelte";

  /**
   * The Agent column on the right of the workbench (editor | preview | agent), like the
   * secondary side bar of other editors. Resizable and remembered; under 900px it floats
   * over the editor instead of squeezing it, and the preview keeps its own space.
   */

  const overlay = $derived(layout.agentOverlay);
  const width = $derived(
    overlay ? Math.min(layout.agentWidth, layout.width - 48 - 24) : layout.agentWidth,
  );
</script>

{#if !overlay}
  <Resizer
    orientation="vertical"
    label="Resize agent panel"
    value={layout.agentWidth}
    min={AGENT_MIN}
    max={AGENT_MAX}
    reverse
    onchange={(w) => layout.setAgentWidth(w)}
    onreset={() => layout.setAgentWidth(DEFAULT_LAYOUT.agentWidth)}
  />
{/if}
<aside
  id="agent-panel"
  aria-label="Agent"
  class="flex min-h-0 shrink-0 flex-col bg-surface {overlay
    ? 'absolute top-0 right-0 bottom-0 z-20 border-l border-line'
    : ''}"
  style="width: {width}px"
>
  <AgentView />
</aside>
