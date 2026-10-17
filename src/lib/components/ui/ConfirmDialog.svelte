<script lang="ts">
  import { dialog } from "$lib/state/dialog.svelte";

  /**
   * Modal confirmation (`alertdialog`). Focus lands on the first action, Tab
   * stays inside, Esc cancels, and focus returns to where it was. Used only when
   * closing something with unsaved changes.
   */

  let box: HTMLDivElement | undefined = $state();
  let returnTo: HTMLElement | null = null;

  $effect(() => {
    if (dialog.request === null) return;
    returnTo =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    box?.querySelector<HTMLElement>("button")?.focus();
    return () => returnTo?.focus();
  });

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      dialog.answer("cancel");
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

{#if dialog.request !== null}
  {@const request = dialog.request}
  <div
    class="fixed inset-0 z-50 flex items-start justify-center pt-[22vh]"
    {onkeydown}
    role="presentation"
  >
    <button
      type="button"
      tabindex="-1"
      class="absolute inset-0 cursor-default"
      aria-label="Cancel"
      onclick={() => dialog.answer("cancel")}
    ></button>
    <div
      bind:this={box}
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="dialog-title"
      aria-describedby="dialog-message"
      class="relative w-[420px] max-w-[92vw] border border-line bg-surface p-5"
    >
      <h2
        id="dialog-title"
        class="m-0 text-[15px] font-light text-text"
        style="letter-spacing: var(--tracking-headline)"
      >
        {request.title}
      </h2>
      <p id="dialog-message" class="mt-2 mb-5 text-xs leading-relaxed text-text-60">
        {request.message}
      </p>
      <div class="flex justify-end gap-2">
        <button type="button" class="mng-btn" onclick={() => dialog.answer("cancel")}
          >Cancel</button
        >
        {#each request.actions as action (action.id)}
          <button
            type="button"
            class="mng-btn"
            onclick={() => dialog.answer(action.id)}
          >
            {action.label}
          </button>
        {/each}
      </div>
    </div>
  </div>
{/if}
