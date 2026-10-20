<script lang="ts">
  import { tick } from "svelte";

  import type { LiveChoiceAction } from "$lib/kernel/liveApi";
  import { live } from "$lib/state/live.svelte";
  import { stepChoice } from "$lib/workbench/dialogRequest";
  import {
    actionFor,
    canJoin,
    buildChoices,
    confirmLabel,
    groupPlan,
    planSummary,
    replacedCount,
    replaceSentence,
  } from "$lib/workbench/livePlan";

  /**
   * Joining a project: before anything on this disk changes, the files are
   * sorted into the same, new for you, different, and only yours. A different
   * file takes the host's version unless you keep yours, and your old copy is
   * saved to a trash folder first. Replacing more than one file needs one
   * explicit confirmation. Closing the dialog leaves the session untouched.
   * A modal dialog: focus stays inside, Esc cancels, focus returns afterwards.
   */

  let box: HTMLDivElement | undefined = $state();
  let returnTo: HTMLElement | null = null;
  let picks = $state<Record<string, LiveChoiceAction>>({});
  let confirmed = $state(false);
  let busy = $state(false);

  const plan = $derived(live.plan ?? []);
  const groups = $derived(groupPlan(plan));
  const replaced = $derived(replacedCount(plan, picks));
  const mustConfirm = $derived(replaced > 1);
  const ready = $derived(canJoin(plan, picks, confirmed) && !busy);
  const open = $derived(live.plan !== null);

  const REPLACE = [
    { id: "take-host", label: "Use the host's" },
    { id: "keep-mine", label: "Keep mine" },
  ] as const;
  const PRIVATE = [
    { id: "keep-private", label: "Keep private" },
    { id: "share", label: "Share it" },
  ] as const;

  // A new plan starts from the defaults.
  $effect(() => {
    if (live.plan === null) return;
    picks = {};
    confirmed = false;
    busy = false;
  });

  $effect(() => {
    if (!open) return;
    returnTo =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void tick().then(() =>
      box
        ?.querySelector<HTMLElement>('[role="radio"][aria-checked="true"], button')
        ?.focus(),
    );
    return () => returnTo?.focus();
  });

  function cancel(): void {
    if (busy) return;
    void live.leave();
  }

  async function join(): Promise<void> {
    const choices = buildChoices(plan, picks, confirmed);
    if (choices === null) return;
    busy = true;
    const ok = await live.resolvePlan(choices);
    if (!ok) busy = false;
  }

  // On the window, so Escape and Tab still work when focus fell on plain text.
  function onkeydown(event: KeyboardEvent): void {
    if (!open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancel();
      return;
    }
    if (event.key !== "Tab" || box === undefined) return;
    const items = [
      ...box.querySelectorAll<HTMLElement>(
        'button:not(:disabled):not([tabindex="-1"]), input:not(:disabled), summary',
      ),
    ];
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

  function pick(path: string, action: LiveChoiceAction): void {
    picks = { ...picks, [path]: action };
  }

  function onradio(
    event: KeyboardEvent,
    path: string,
    options: readonly { id: string; label: string }[],
  ): void {
    const current = actionFor(plan, picks, path) ?? "";
    const next = stepChoice(options, current, event.key);
    if (next === null) return;
    event.preventDefault();
    pick(path, next as LiveChoiceAction);
    const group =
      event.currentTarget instanceof HTMLElement
        ? event.currentTarget.parentElement
        : null;
    void tick().then(() =>
      group?.querySelector<HTMLElement>(`[data-choice="${next}"]`)?.focus(),
    );
  }
</script>

{#snippet segment(path: string, options: readonly { id: string; label: string }[])}
  {@const current = actionFor(plan, picks, path)}
  <div
    class="flex shrink-0 border border-line"
    role="radiogroup"
    aria-label="What to do with {path}"
  >
    {#each options as option, index (option.id)}
      <button
        type="button"
        role="radio"
        aria-checked={current === option.id}
        tabindex={current === option.id ? 0 : -1}
        data-choice={option.id}
        class="inset-focus h-6 px-2 text-[11px] {index > 0
          ? 'border-l border-line'
          : ''} {current === option.id
          ? 'bg-surface-hi text-text'
          : 'text-text-60 hover:text-text'}"
        onclick={() => pick(path, option.id as LiveChoiceAction)}
        onkeydown={(event) => onradio(event, path, options)}
      >
        {option.label}
      </button>
    {/each}
  </div>
{/snippet}

{#snippet names(paths: readonly string[])}
  <ul
    class="m-0 max-h-28 list-none overflow-y-auto p-0 font-mono text-[11px] text-text-60"
  >
    {#each paths as path (path)}
      <li class="truncate py-0.5">{path}</li>
    {/each}
  </ul>
{/snippet}

<svelte:window {onkeydown} />

{#if open}
  <div
    class="fixed inset-0 z-50 flex items-start justify-center pt-[10vh]"
    role="presentation"
  >
    <div class="absolute inset-0 bg-bg/60" aria-hidden="true"></div>
    <div
      bind:this={box}
      role="dialog"
      aria-modal="true"
      aria-labelledby="join-plan-title"
      aria-describedby="join-plan-summary"
      data-testid="join-plan"
      class="relative flex max-h-[80vh] w-[540px] max-w-[94vw] flex-col border border-line bg-surface"
    >
      <div class="space-y-1 border-b border-line px-5 py-4">
        <h2
          id="join-plan-title"
          class="m-0 text-[15px] font-light text-text"
          style="letter-spacing: var(--tracking-headline)"
        >
          Join the shared project
        </h2>
        <p class="m-0 text-xs leading-relaxed text-text-60">
          Nothing on your disk changes until you press Join. Here is how your files
          compare with the host's.
        </p>
        <p
          id="join-plan-summary"
          class="m-0 font-mono text-[11px] text-text"
          data-testid="join-plan-summary"
        >
          {planSummary(groups)}
        </p>
      </div>

      <div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4 text-xs">
        {#if groups.differing.length > 0}
          <section aria-labelledby="plan-differing" data-testid="plan-differing">
            <h3 id="plan-differing" class="m-0 mb-1 text-[13px] font-normal text-text">
              Different on your disk ({groups.differing.length})
            </h3>
            <p
              class="m-0 mb-2 leading-relaxed text-text-60"
              data-testid="plan-replace-note"
            >
              {replaceSentence(replaced)}
            </p>
            <ul class="m-0 list-none space-y-1 p-0">
              {#each groups.differing as path (path)}
                <li
                  class="flex items-center justify-between gap-3 border border-line px-2 py-1"
                  data-path={path}
                >
                  <span
                    class="min-w-0 truncate font-mono text-[11px] text-text"
                    title={path}>{path}</span
                  >
                  {@render segment(path, REPLACE)}
                </li>
              {/each}
            </ul>
            {#if mustConfirm}
              <label
                class="mt-2 flex cursor-default items-start gap-2 border border-line bg-bg px-2 py-2 text-text"
              >
                <input
                  type="checkbox"
                  bind:checked={confirmed}
                  class="mt-0.5 shrink-0"
                  data-testid="plan-confirm-all"
                />
                <span>
                  {confirmLabel(replaced)}.
                  <span class="text-text-60"
                    >Your old copies are saved to a trash folder first.</span
                  >
                </span>
              </label>
            {/if}
          </section>
        {/if}

        {#if groups.guestOnly.length > 0}
          <section aria-labelledby="plan-guest" data-testid="plan-guest-only">
            <h3 id="plan-guest" class="m-0 mb-1 text-[13px] font-normal text-text">
              Only on your disk ({groups.guestOnly.length})
            </h3>
            <p class="m-0 mb-2 leading-relaxed text-text-60">
              These stay private unless you share them with everyone in the project.
            </p>
            <ul class="m-0 list-none space-y-1 p-0">
              {#each groups.guestOnly as path (path)}
                <li
                  class="flex items-center justify-between gap-3 border border-line px-2 py-1"
                  data-path={path}
                >
                  <span
                    class="min-w-0 truncate font-mono text-[11px] text-text"
                    title={path}>{path}</span
                  >
                  {@render segment(path, PRIVATE)}
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if groups.missing.length > 0}
          <details data-testid="plan-missing">
            <summary class="inset-focus cursor-default text-text">
              New for you ({groups.missing.length})
              <span class="text-text-60">: added to your project</span>
            </summary>
            <div class="pt-1">{@render names(groups.missing)}</div>
          </details>
        {/if}
        {#if groups.identical.length > 0}
          <details data-testid="plan-identical">
            <summary class="inset-focus cursor-default text-text">
              The same on both sides ({groups.identical.length})
              <span class="text-text-60">: nothing to do</span>
            </summary>
            <div class="pt-1">{@render names(groups.identical)}</div>
          </details>
        {/if}
        {#if plan.length === 0}
          <p class="m-0 text-text-60">The host has no files yet.</p>
        {/if}
      </div>

      <div
        class="flex flex-wrap items-center justify-end gap-2 border-t border-line px-5 py-3"
      >
        {#if mustConfirm && !confirmed}
          <span class="mr-auto text-[11px] text-text-60" role="status"
            >Confirm the replacement above to join.</span
          >
        {/if}
        <button
          type="button"
          class="mng-btn"
          data-testid="plan-cancel"
          disabled={busy}
          onclick={cancel}>Cancel</button
        >
        <button
          type="button"
          class="mng-btn {ready ? 'border-accent bg-accent text-accent-ink' : ''}"
          data-testid="plan-join"
          disabled={!ready}
          onclick={() => void join()}>Join</button
        >
      </div>
    </div>
  </div>
{/if}
