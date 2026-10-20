<script lang="ts">
  import { tick } from "svelte";

  import { live } from "$lib/state/live.svelte";
  import {
    PAUSED_DISCARD_NOTE,
    PAUSED_QUESTION,
    pausedSentence,
  } from "$lib/workbench/live";

  /**
   * Many files changed on this disk at once (a Git checkout, say), so the
   * session holds them back instead of sending them to everyone. The person
   * decides: share the changes, or put the folder back as the session has it
   * (the copies go to the trash). "Decide later" only hides this window; the
   * change stays held and the status bar offers it again.
   * A modal dialog: focus stays inside, Esc means later.
   */

  let box: HTMLDivElement | undefined = $state();
  let returnTo: HTMLElement | null = null;
  let busy = $state(false);

  const open = $derived(live.active && live.paused !== null && live.pausedOpen);

  $effect(() => {
    if (!open) return;
    busy = false;
    returnTo =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void tick().then(() => box?.querySelector<HTMLElement>("button")?.focus());
    return () => returnTo?.focus();
  });

  async function answer(choice: "share" | "discard"): Promise<void> {
    busy = true;
    await live.answerPaused(choice);
    busy = false;
  }

  function onkeydown(event: KeyboardEvent): void {
    if (!open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      live.deferPaused();
      return;
    }
    if (event.key !== "Tab" || box === undefined) return;
    const items = [...box.querySelectorAll<HTMLElement>("button:not(:disabled)")];
    const first = items[0];
    const last = items[items.length - 1];
    if (!box.contains(document.activeElement)) {
      event.preventDefault();
      first?.focus();
    } else if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }
</script>

<svelte:window {onkeydown} />

{#if open && live.paused !== null}
  <div
    class="fixed inset-0 z-50 flex items-start justify-center pt-[10vh]"
    role="presentation"
  >
    <div class="absolute inset-0 bg-bg/60" aria-hidden="true"></div>
    <div
      bind:this={box}
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="paused-title"
      aria-describedby="paused-question"
      data-testid="live-paused"
      class="relative flex w-[460px] max-w-[94vw] flex-col border border-line bg-surface"
    >
      <div class="space-y-2 px-5 py-4">
        <h2
          id="paused-title"
          class="m-0 text-[15px] font-light text-text"
          style="letter-spacing: var(--tracking-headline)"
        >
          Many files changed at once
        </h2>
        <p
          id="paused-question"
          class="m-0 text-xs leading-relaxed text-text"
          data-testid="live-paused-question"
        >
          {PAUSED_QUESTION}
        </p>
        <p
          class="m-0 font-mono text-[11px] text-text-60"
          data-testid="live-paused-size"
        >
          {pausedSentence(live.paused)}
        </p>
        <p
          class="m-0 text-xs leading-relaxed text-text-60"
          data-testid="live-paused-note"
        >
          {PAUSED_DISCARD_NOTE}
        </p>
      </div>
      <div class="flex flex-wrap justify-end gap-2 border-t border-line px-5 py-3">
        <button
          type="button"
          class="mng-btn"
          data-testid="live-paused-later"
          disabled={busy}
          onclick={() => live.deferPaused()}>Decide later</button
        >
        <button
          type="button"
          class="mng-btn"
          data-testid="live-paused-discard"
          disabled={busy}
          onclick={() => void answer("discard")}>Put my folder back</button
        >
        <button
          type="button"
          class="mng-btn border-accent bg-accent text-accent-ink"
          data-testid="live-paused-share"
          disabled={busy}
          onclick={() => void answer("share")}>Share these changes</button
        >
      </div>
    </div>
  </div>
{/if}
