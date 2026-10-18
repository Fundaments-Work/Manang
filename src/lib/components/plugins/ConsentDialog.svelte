<script lang="ts">
  import { plugins } from "$lib/state/plugins.svelte";

  /**
   * The consent dialog: what a plugin asks to access, in plain language,
   * before it is installed, updated or approved. All or nothing: Approve
   * grants everything listed, Cancel grants nothing. Focus starts on Cancel,
   * Tab stays inside, Esc cancels, focus returns to where it was.
   */

  let box: HTMLDivElement | undefined = $state();
  let returnTo: HTMLElement | null = null;

  const request = $derived(plugins.consent);

  $effect(() => {
    if (request === null) return;
    returnTo =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    box?.querySelector<HTMLElement>("button")?.focus();
    return () => returnTo?.focus();
  });

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      plugins.answerConsent(false);
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

  const heading = (mode: "install" | "update" | "review", name: string): string =>
    mode === "install"
      ? `Install ${name}?`
      : mode === "update"
        ? `Update ${name}?`
        : `Approve ${name}?`;

  const confirmLabel = (mode: "install" | "update" | "review"): string =>
    mode === "install" ? "Approve and install" : "Approve";
</script>

{#if request !== null}
  <div
    class="fixed inset-0 z-50 flex items-start justify-center pt-[16vh]"
    {onkeydown}
    role="presentation"
  >
    <button
      type="button"
      tabindex="-1"
      class="absolute inset-0 cursor-default"
      aria-label="Cancel"
      onclick={() => plugins.answerConsent(false)}
    ></button>
    <div
      bind:this={box}
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="consent-title"
      aria-describedby="consent-message"
      data-consent={request.name}
      class="relative max-h-[70vh] w-[440px] max-w-[92vw] overflow-y-auto border border-line bg-surface p-5"
    >
      <h2
        id="consent-title"
        class="m-0 flex items-baseline gap-2 text-[15px] font-light text-text"
        style="letter-spacing: var(--tracking-headline)"
      >
        {heading(request.mode, request.name)}
        <span class="font-mono text-[11px] text-text-muted">{request.version}</span>
      </h2>
      <p id="consent-message" class="mt-2 mb-0 text-xs leading-relaxed text-text-60">
        {#if request.description !== ""}{request.description}{/if}
        {#if request.mode !== "install" && request.running !== null}
          Version {request.running} is running now.
        {/if}
      </p>
      <p class="mt-1 mb-0 text-[11px] text-text-muted">
        {#if request.publisher !== null}
          Published by {request.publisher}, not verified.
        {:else}
          No publisher stated, not signed.
        {/if}
        It runs in a sandbox with only the access below.
      </p>

      <h3 class="mt-4 mb-1 text-xs font-normal text-text">What it asks for</h3>
      {#if request.lines.length === 0}
        <p class="m-0 text-xs text-text-60" data-consent-none>
          No access to your files, documents or network.
        </p>
      {:else}
        <ul class="m-0 list-none p-0 text-xs leading-relaxed" data-consent-lines>
          {#each request.lines as line (line.text)}
            <li class="flex items-baseline gap-2 py-0.5">
              <span class={line.isNew ? "text-text" : "text-text-60"}>{line.text}</span>
              {#if line.isNew}
                <span
                  class="border border-text-60 px-1 font-mono text-[10px] text-text"
                  data-consent-new>New</span
                >
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      <div class="mt-5 flex flex-wrap justify-end gap-2">
        <button
          type="button"
          class="mng-btn"
          onclick={() => plugins.answerConsent(false)}>Cancel</button
        >
        <button
          type="button"
          class="mng-btn"
          data-consent-approve
          onclick={() => plugins.answerConsent(true)}
        >
          {confirmLabel(request.mode)}
        </button>
      </div>
    </div>
  </div>
{/if}
