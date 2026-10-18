<script lang="ts">
  import { revealLogs } from "$lib/state/actions";
  import { feedback } from "$lib/state/feedback.svelte";
  import { MESSAGE_PLACEHOLDER } from "$lib/workbench/feedback";

  /**
   * Send feedback / Report a problem. Modal: focus lands on the message, Tab
   * stays inside, Esc closes and focus returns to where it was. The diagnostics
   * the person is about to share are shown in full; nothing is sent by Manang.
   */

  let box: HTMLDivElement | undefined = $state();
  let returnTo: HTMLElement | null = null;

  $effect(() => {
    if (!feedback.open) return;
    returnTo =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    box?.querySelector<HTMLTextAreaElement>("textarea")?.focus();
    return () => returnTo?.focus();
  });

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      feedback.close();
      return;
    }
    if (event.key !== "Tab" || box === undefined) return;
    const items = [
      ...box.querySelectorAll<HTMLElement>(
        "textarea, input, button:not([tabindex='-1'])",
      ),
    ];
    const first = items[0];
    const last = items[items.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }
</script>

{#if feedback.open}
  <div
    class="fixed inset-0 z-50 flex items-start justify-center pt-[10vh]"
    {onkeydown}
    role="presentation"
  >
    <button
      type="button"
      tabindex="-1"
      class="absolute inset-0 cursor-default"
      aria-label="Close"
      onclick={() => feedback.close()}
    ></button>
    <div
      bind:this={box}
      role="dialog"
      aria-modal="true"
      aria-labelledby="feedback-title"
      data-testid="feedback-dialog"
      class="relative flex max-h-[80vh] w-[560px] max-w-[94vw] flex-col border border-line bg-surface p-5"
    >
      <h2
        id="feedback-title"
        class="m-0 text-[15px] font-light text-text"
        style="letter-spacing: var(--tracking-headline)"
      >
        {feedback.kind === "problem" ? "Report a problem" : "Send feedback"}
      </h2>
      <p class="mt-2 mb-4 text-xs leading-relaxed text-text-60">
        Write a few lines, then copy this and send it to the Manang team. Manang sends
        nothing by itself, and you can read everything that is included.
      </p>

      <label class="flex flex-col gap-1.5 text-xs text-text-60">
        {feedback.kind === "problem" ? "What happened" : "Your feedback"}
        <textarea
          bind:value={feedback.message}
          class="mng-field min-h-24 resize-y py-2 text-[13px] leading-relaxed"
          placeholder={MESSAGE_PLACEHOLDER[feedback.kind]}
          spellcheck="true"
        ></textarea>
      </label>

      <label class="mt-4 flex items-center gap-2 text-xs text-text">
        <input type="checkbox" bind:checked={feedback.includeReport} />
        Include diagnostics (versions, plugins, the end of the log)
      </label>

      {#if feedback.includeReport}
        <div class="mt-2 min-h-0 flex-1 overflow-auto border border-line bg-bg">
          {#if feedback.loading}
            <p class="m-0 p-3 text-xs text-text-60" role="status">
              Collecting diagnostics
            </p>
          {:else if feedback.failed}
            <p class="m-0 p-3 text-xs text-text" role="alert">
              The diagnostics could not be collected. You can still send your message.
            </p>
          {:else}
            <pre
              class="m-0 max-h-56 overflow-auto p-3 font-mono text-[11px] leading-relaxed whitespace-pre-wrap text-text-60"
              data-testid="feedback-report">{feedback.report}</pre>
          {/if}
        </div>
        <p class="mt-1 mb-0 text-[11px] text-text-muted">
          Home folders and anything that looks like a key are removed. Your documents
          are never included.
        </p>
      {/if}

      <div class="mt-5 flex items-center justify-end gap-2">
        <button type="button" class="mng-btn" onclick={() => void revealLogs(false)}>
          Open logs folder
        </button>
        <button type="button" class="mng-btn" onclick={() => feedback.close()}>
          Close
        </button>
        <button
          type="button"
          class="mng-btn border-accent bg-accent text-accent-ink"
          data-testid="feedback-copy"
          onclick={() => void feedback.copy()}
        >
          {feedback.copied ? "Copied" : "Copy"}
        </button>
      </div>
    </div>
  </div>
{/if}
