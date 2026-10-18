<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import { agent } from "$lib/state/agent.svelte";
  import { layout } from "$lib/state/layout.svelte";

  import DropMenu from "./DropMenu.svelte";

  /**
   * The header of the Agent column: its name, "New chat", History (earlier chats of this
   * project) and the model picker (provider and model, compact). Everything here is local: no
   * menu sends anything.
   */

  const status = $derived(agent.status);
  const preset = $derived(status?.presets.find((p) => p.id === status.provider));
  const modelName = $derived(
    agent.view.model !== "" && agent.view.running
      ? agent.view.model
      : (status?.model ?? ""),
  );
  const pickerText = $derived(
    status === null
      ? "Model"
      : modelName !== ""
        ? `${status.provider_name} / ${modelName}`
        : status.provider_name,
  );

  const item =
    "inset-focus flex w-full items-baseline justify-between gap-3 px-3 py-1.5 text-left text-xs text-text-60 hover:bg-surface-muted hover:text-text";

  function ago(ms: number): string {
    const minutes = Math.max(0, Math.round((Date.now() - ms) / 60_000));
    if (minutes < 1) return "now";
    if (minutes < 60) return `${minutes} min ago`;
    const hours = Math.round(minutes / 60);
    if (hours < 24) return `${hours} h ago`;
    return `${Math.round(hours / 24)} d ago`;
  }
</script>

<div class="flex h-9 shrink-0 items-center gap-1 border-b border-line pr-1 pl-3">
  <h2 class="m-0 text-xs font-normal text-text">Agent</h2>
  <span class="min-w-0 flex-1 truncate pl-2" data-agent-status-wrap>
    <DropMenu label="Model: {pickerText}" triggerClass="max-w-full text-[11px]">
      {#snippet trigger()}
        <span class="truncate" data-agent-model>{pickerText}</span>
        <Icon name="chevron-down" size={10} />
      {/snippet}
      {#snippet children(close)}
        {#if preset !== undefined && preset.models.length > 0}
          <p class="m-0 px-3 pt-1 pb-0.5 text-[11px] text-text-muted">Model</p>
          {#each preset.models as model (model)}
            <button
              type="button"
              role="menuitem"
              class={item}
              aria-current={model === status?.model}
              onclick={() => {
                close();
                void agent.chooseModel(model);
              }}
            >
              <span class="font-mono">{model}</span>
              {#if model === status?.model}<span class="text-text">current</span>{/if}
            </button>
          {/each}
        {/if}
        <p class="m-0 px-3 pt-2 pb-0.5 text-[11px] text-text-muted">Provider</p>
        {#each status?.presets ?? [] as p (p.id)}
          <button
            type="button"
            role="menuitem"
            class={item}
            aria-current={p.id === status?.provider}
            onclick={() => {
              close();
              void agent.chooseProvider(p.id);
            }}
          >
            <span>{p.name}</span>
            {#if p.id === status?.provider}<span class="text-text">current</span>{/if}
          </button>
        {/each}
        <button
          type="button"
          role="menuitem"
          class="{item} mt-1 border-t border-line"
          onclick={() => {
            close();
            agent.openSetup();
          }}>Provider and key in Settings</button
        >
      {/snippet}
    </DropMenu>
  </span>

  <button
    type="button"
    class="mng-icon-btn h-7 w-7"
    aria-label="New chat"
    title="New chat"
    data-agent-new
    disabled={agent.view.items.length === 0 || agent.view.running}
    onclick={() => agent.newConversation()}
  >
    <Icon name="plus" size={14} />
  </button>
  <DropMenu
    label="History: earlier chats"
    align="right"
    triggerClass="w-7 justify-center"
    onopen={() => void agent.loadHistory()}
  >
    {#snippet trigger()}
      <Icon name="recent" size={14} />
    {/snippet}
    {#snippet children(close)}
      {#if agent.history.length === 0}
        <p class="m-0 px-3 py-2 text-xs text-text-60">
          No earlier chats in this project.
        </p>
      {/if}
      {#each agent.history.slice(0, 20) as h (h.id)}
        <button
          type="button"
          role="menuitem"
          class={item}
          onclick={() => {
            close();
            void agent.openSession(h.id);
          }}
        >
          <span class="min-w-0 flex-1 truncate">{h.title || "Untitled chat"}</span>
          <span class="shrink-0 text-[11px] text-text-muted">{ago(h.updated_ms)}</span>
        </button>
      {/each}
    {/snippet}
  </DropMenu>
  <button
    type="button"
    class="mng-icon-btn h-7 w-7"
    aria-label="Close agent"
    title="Close (Ctrl+Shift+A)"
    onclick={() => layout.closeAgent()}
  >
    <Icon name="close" size={12} />
  </button>
</div>
