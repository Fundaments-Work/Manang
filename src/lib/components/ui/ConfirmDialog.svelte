<script lang="ts">
  import { dialog } from "$lib/state/dialog.svelte";
  import { stepChoice } from "$lib/workbench/dialogRequest";

  /**
   * Modal confirmation (`alertdialog`). Focus lands on the first action (or the
   * text field of a prompt), Tab stays inside, Esc cancels, and focus returns to
   * where it was. Used when closing something with unsaved changes, and as the
   * one-field prompt that names a new diagram (Enter picks the first choice).
   */

  let box: HTMLDivElement | undefined = $state();
  let returnTo: HTMLElement | null = null;

  $effect(() => {
    if (dialog.request === null) return;
    returnTo =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const field = box?.querySelector<HTMLInputElement>("input");
    if (field != null) {
      field.focus();
      field.select();
    } else box?.querySelector<HTMLElement>("button")?.focus();
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
    const buttons = [...box.querySelectorAll<HTMLElement>("input, button")];
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
      {#if request.input !== undefined}
        <label class="mb-5 flex flex-col gap-1.5 text-xs text-text-60">
          {request.input.label}
          <input
            bind:value={dialog.value}
            class="mng-field h-9 text-[13px]"
            autocomplete="off"
            spellcheck="false"
            onkeydown={(event) => {
              const first = request.actions[0];
              if (event.key !== "Enter" || first === undefined) return;
              event.preventDefault();
              dialog.answer(first.id);
            }}
          />
        </label>
      {/if}
      {#if request.extraInput !== undefined}
        <label class="mb-5 flex flex-col gap-1.5 text-xs text-text-60">
          {request.extraInput.label}
          <input
            bind:value={dialog.extra}
            class="mng-field h-9 text-[13px]"
            autocomplete="off"
            spellcheck="false"
            onkeydown={(event) => {
              const first = request.actions[0];
              if (event.key !== "Enter" || first === undefined) return;
              event.preventDefault();
              dialog.answer(first.id);
            }}
          />
        </label>
      {/if}
      {#if request.choice !== undefined}
        {@const choice = request.choice}
        <div class="mb-5 flex flex-col gap-1.5 text-xs text-text-60">
          <span id="dialog-choice">{choice.label}</span>
          <div
            class="flex border border-line"
            role="radiogroup"
            aria-labelledby="dialog-choice"
          >
            {#each choice.options as option, index (option.id)}
              <button
                type="button"
                role="radio"
                aria-checked={dialog.choice === option.id}
                tabindex={dialog.choice === option.id ? 0 : -1}
                class="inset-focus h-7 min-w-0 flex-1 truncate px-2 text-xs {index > 0
                  ? 'border-l border-line'
                  : ''} {dialog.choice === option.id
                  ? 'bg-surface-hi text-text'
                  : 'text-text-60 hover:text-text'}"
                onclick={() => (dialog.choice = option.id)}
                onkeydown={(event) => {
                  const next = stepChoice(choice.options, dialog.choice, event.key);
                  if (next === null) return;
                  event.preventDefault();
                  dialog.choice = next;
                  const group = event.currentTarget.parentElement;
                  group?.querySelector<HTMLElement>(`[data-choice="${next}"]`)?.focus();
                }}
                data-choice={option.id}
              >
                {option.label}
              </button>
            {/each}
          </div>
        </div>
      {/if}
      <div class="flex flex-wrap justify-end gap-2">
        <button type="button" class="mng-btn" onclick={() => dialog.answer("cancel")}
          >Cancel</button
        >
        {#each request.actions as action (action.id)}
          <button
            type="button"
            class="mng-btn {action.primary === true
              ? 'border-accent bg-accent text-accent-ink'
              : ''}"
            onclick={() => dialog.answer(action.id)}
          >
            {action.label}
          </button>
        {/each}
      </div>
    </div>
  </div>
{/if}
