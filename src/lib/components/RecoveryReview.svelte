<script lang="ts">
  import VersionDiff from "$lib/components/VersionDiff.svelte";
  import { openFile } from "$lib/state/actions";
  import { discardShown, recoverShown } from "$lib/state/recoveryActions";
  import { recovery } from "$lib/state/recovery.svelte";
  import { whenLabel } from "$lib/workbench/recovery";

  /**
   * Review of recovered edits, one file at a time: the text Manang kept against
   * what is on disk now. Recover opens the kept text as an unsaved edit (the
   * file is not touched); Skip keeps the snapshot for later; Discard removes it.
   */

  let box: HTMLDivElement | undefined = $state();
  let returnTo: HTMLElement | null = null;

  const review = $derived(recovery.review);
  const item = $derived(review === null ? undefined : review.items[review.index]);

  $effect(() => {
    if (review === null) return;
    returnTo =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    box?.querySelector<HTMLElement>("button[data-primary]")?.focus();
    return () => returnTo?.focus();
  });

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      recovery.closeReview();
      return;
    }
    if (event.key !== "Tab" || box === undefined) return;
    const buttons = [...box.querySelectorAll<HTMLElement>("button")];
    const first = buttons[0];
    const last = buttons[buttons.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }
</script>

{#if review !== null && item !== undefined}
  <div
    class="fixed inset-0 z-50 flex items-start justify-center pt-[12vh]"
    {onkeydown}
    role="presentation"
  >
    <button
      type="button"
      tabindex="-1"
      class="absolute inset-0 cursor-default"
      aria-label="Close review"
      onclick={() => recovery.closeReview()}
    ></button>
    <div
      bind:this={box}
      role="dialog"
      aria-modal="true"
      aria-labelledby="recovery-title"
      data-testid="recovery-review"
      class="relative w-[720px] max-w-[94vw] border border-line bg-surface p-5"
    >
      <h2
        id="recovery-title"
        class="m-0 text-[15px] font-light text-text"
        style="letter-spacing: var(--tracking-headline)"
      >
        {item.entry.path}
      </h2>
      <p class="mt-2 mb-3 text-xs leading-relaxed text-text-60">
        File {review.index + 1} of {review.items.length}. Kept at
        {whenLabel(item.entry.saved_at, Date.now())}.
        {#if item.entry.disk === "changed"}
          The file on disk has changed since.
        {:else if item.entry.disk === "missing"}
          The file is not on disk.
        {/if}
        Added lines are in your recovered text; removed lines are what is on disk now.
      </p>
      {#if item.diff !== null}
        <VersionDiff diff={item.diff} />
      {:else}
        <p class="m-0 border-y border-line px-3 py-2 text-xs text-text-60">
          A figure: its recovered drawing replaces the open scene as an unsaved change.
        </p>
      {/if}
      <div class="mt-4 flex flex-wrap justify-end gap-2">
        <button type="button" class="mng-btn" onclick={() => recovery.closeReview()}
          >Close</button
        >
        <button type="button" class="mng-btn" onclick={() => recovery.nextReview()}
          >Skip</button
        >
        <button type="button" class="mng-btn" onclick={() => discardShown()}
          >Discard</button
        >
        <button
          type="button"
          data-primary
          class="mng-btn border-accent bg-accent text-accent-ink"
          onclick={() => void recoverShown(openFile)}>Recover this</button
        >
      </div>
    </div>
  </div>
{/if}
