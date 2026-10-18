<script lang="ts">
  import { redactionSegments } from "$lib/agent/redaction";
  import { tokenLabel } from "$lib/agent/context";
  import { formatCost } from "$lib/agent/reducer";
  import { agent } from "$lib/state/agent.svelte";

  /**
   * "Preview what will be sent": the exact outbound request, read-only, with the redaction diff.
   * Nothing was sent to produce it. Esc or Close dismisses it.
   */

  let box: HTMLDivElement | undefined = $state();
  let returnTo: HTMLElement | null = null;

  $effect(() => {
    if (!agent.previewOpen) return;
    returnTo =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    box?.querySelector<HTMLElement>("button")?.focus();
    return () => returnTo?.focus();
  });

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      agent.closePreview();
    }
  }

  const preview = $derived(agent.preview);
  const redacted = $derived(
    preview?.parts.filter((c) => c.original !== undefined) ?? [],
  );
</script>

{#if agent.previewOpen}
  <div
    class="fixed inset-0 z-50 flex items-start justify-center pt-[8vh]"
    {onkeydown}
    role="presentation"
  >
    <button
      type="button"
      tabindex="-1"
      class="absolute inset-0 cursor-default"
      aria-label="Close preview"
      onclick={() => agent.closePreview()}
    ></button>
    <div
      bind:this={box}
      role="dialog"
      aria-modal="true"
      aria-labelledby="agent-preview-title"
      class="relative flex max-h-[80vh] w-[720px] max-w-[94vw] flex-col border border-line bg-surface"
      data-agent-preview-dialog
    >
      <div class="flex items-baseline gap-3 border-b border-line px-5 py-3">
        <h2
          id="agent-preview-title"
          class="m-0 text-[15px] font-light text-text"
          style="letter-spacing: var(--tracking-headline)"
        >
          What will be sent
        </h2>
        <span class="text-[11px] text-text-60">Nothing has been sent yet.</span>
        <button
          type="button"
          class="mng-btn ml-auto"
          onclick={() => agent.closePreview()}>Close</button
        >
      </div>
      <div class="selectable min-h-0 flex-1 overflow-y-auto px-5 py-3 text-xs">
        {#if preview === null}
          <p class="m-0 text-text-60" role="status">
            {agent.previewError === "" ? "Preparing the request." : agent.previewError}
          </p>
        {:else}
          {#if preview.mock === true}
            <p class="m-0 mb-3 text-text-60">
              Browser mock: this is the shape of the request. No provider is contacted.
            </p>
          {/if}
          <dl class="m-0 grid grid-cols-[7rem_1fr] gap-x-3 gap-y-1">
            <dt class="text-text-60">Provider</dt>
            <dd class="m-0 text-text">{preview.provider}, model {preview.model}</dd>
            <dt class="text-text-60">Address</dt>
            <dd class="m-0 font-mono text-[11px] break-all text-text">{preview.url}</dd>
            <dt class="text-text-60">Size</dt>
            <dd class="m-0 text-text" data-preview-tokens>
              about {tokenLabel(preview.tokens.request)} tokens in all,
              {tokenLabel(preview.tokens.context)} of them your context
            </dd>
            <dt class="text-text-60">Cost</dt>
            <dd class="m-0 text-text">
              {formatCost(preview.cost.est_input_micro_usd)} for the input. An estimate: the
              provider's bill is the truth.
            </dd>
            <dt class="text-text-60">Tools</dt>
            <dd class="m-0 text-text">
              {preview.tools.map((t) => `${t.name} (${t.risk})`).join(", ") || "none"}
            </dd>
          </dl>

          <h3 class="mt-4 mb-1 text-[13px] font-normal text-text">What goes with your message</h3>
          {#if preview.parts.length === 0}
            <p class="m-0 text-text-60">Only your message, and the project overview in the first message of a chat.</p>
          {/if}
          {#each preview.parts as chip (chip.id)}
            <p class="m-0 text-text-60">
              {chip.label}, about {tokenLabel(chip.tokens)} tokens{chip.truncated
                ? ", cut to the size limit"
                : ""}
            </p>
          {/each}

          <h3 class="mt-4 mb-1 text-[13px] font-normal text-text" data-redactions>
            Redactions ({preview.redaction_count})
          </h3>
          {#if redacted.length === 0}
            <p class="m-0 text-text-60">
              Nothing needed redacting. Secret-like strings are replaced before sending.
            </p>
          {/if}
          {#each redacted as chip (chip.id)}
            <p class="m-0 mt-2 text-text-60">
              {chip.label}: removed text is struck through
            </p>
            <pre
              class="m-0 border border-line bg-bg px-2.5 py-2 font-mono text-[11px] leading-relaxed break-words whitespace-pre-wrap">{#each redactionSegments(chip.original ?? "", chip.redactions) as seg, i (i)}{#if seg.kind === "removed"}<span
                    class="mng-ai-removed">{seg.text}</span
                  >{:else if seg.kind === "replacement"}<span class="mng-ai-inserted"
                    >{seg.text}</span
                  >{:else}{seg.text}{/if}{/each}</pre>
          {/each}

          <h3 class="mt-4 mb-1 text-[13px] font-normal text-text">Request body</h3>
          <pre
            class="m-0 max-h-72 overflow-auto border border-line bg-bg px-2.5 py-2 font-mono text-[11px] leading-relaxed break-words whitespace-pre-wrap"
            data-preview-body>{preview.body}</pre>
        {/if}
      </div>
    </div>
  </div>
{/if}
