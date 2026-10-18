<script lang="ts">
  import { tick } from "svelte";

  import { live } from "$lib/state/live.svelte";
  import { settings } from "$lib/state/settings.svelte";
  import { sharePopover } from "$lib/state/sharePopover.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import {
    canShareLive,
    connectionLabel,
    participantSummary,
  } from "$lib/workbench/live";
  import { basename } from "$lib/workbench/paths";

  /**
   * The Share popover: one place for everything about working with other
   * people. A non-modal dialog under the Share button: Escape closes it and
   * puts focus back on the button; clicking or tabbing away closes it too.
   * "Edit together live" starts, joins, copies the invite and leaves; the
   * setup note appears until a relay address is set.
   */
  interface Props {
    /** The Share button, which gets focus back when the popover closes. */
    anchor: HTMLElement | undefined;
  }

  const { anchor }: Props = $props();

  let root: HTMLElement | undefined = $state();

  const relay = $derived(String(settings.value("live.relay-url") ?? "").trim());
  const name = $derived(String(settings.value("live.display-name") ?? "").trim());
  const file = $derived(
    canShareLive(tabs.active?.kind) && tabs.active?.view === "text"
      ? basename(tabs.active.path)
      : null,
  );
  const conn = $derived(live.connection === "off" ? "connecting" : live.connection);

  // Take focus when opened; read the settings so the setup note is current.
  $effect(() => {
    void sharePopover.request;
    if (!sharePopover.open) return;
    void settings.load(workspace.project?.root ?? null);
    void tick().then(() => first()?.focus());
  });

  // A click outside closes it, without stealing focus from where the click went.
  $effect(() => {
    if (!sharePopover.open) return;
    const outside = (event: PointerEvent): void => {
      const target = event.target as Node;
      if (root?.contains(target) === true || anchor?.contains(target) === true) return;
      sharePopover.hide();
    };
    window.addEventListener("pointerdown", outside, true);
    return () => window.removeEventListener("pointerdown", outside, true);
  });

  function controls(): HTMLElement[] {
    return root === undefined
      ? []
      : [...root.querySelectorAll<HTMLElement>("button:not(:disabled), input")];
  }
  const first = (): HTMLElement | undefined => controls()[0];

  function close(restore: boolean): void {
    sharePopover.hide();
    if (restore) anchor?.focus();
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close(true);
    }
  }

  function onfocusout(event: FocusEvent): void {
    const next = event.relatedTarget;
    if (next instanceof Node && root?.contains(next) === false && next !== anchor) {
      sharePopover.hide();
    }
  }

  /** Run an action that opens a dialog or changes the view: the popover steps aside first. */
  function then(action: () => void): void {
    close(false);
    action();
  }

  function openSettings(): void {
    then(() => live.openRelaySettings());
  }
</script>

{#if sharePopover.open}
  <!-- A non-modal popover handles Escape and focus loss on its container, as the
       WAI-ARIA disclosure pattern does; every control inside is a real button. -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    bind:this={root}
    id="share-popover"
    role="group"
    tabindex="-1"
    aria-label="Share"
    data-testid="share-popover"
    class="absolute top-full right-0 z-30 w-[22rem] max-w-[calc(100vw-1rem)] border border-line bg-surface text-xs"
    {onkeydown}
    {onfocusout}
  >
    <section class="space-y-2 px-3 py-3" aria-labelledby="share-live-title">
      <h2 id="share-live-title" class="m-0 text-[13px] font-normal text-text">
        Edit together live
      </h2>

      {#if live.active}
        <p class="m-0 text-text" role="status" data-testid="share-live-state">
          {connectionLabel(conn, live.attempt)}: {participantSummary(live.peers)}
        </p>
        <p class="m-0 text-text-60">
          Everyone in the session is typing in {live.path === null
            ? "the same file"
            : basename(live.path)}.
        </p>
      {:else}
        <p class="m-0 text-text-60">
          Type in the same file at the same time and see each other's cursors. Your file
          stays an ordinary file on your disk.
        </p>
        {#if relay === ""}
          <div
            class="space-y-1 border border-line bg-bg px-3 py-2"
            data-testid="share-setup"
          >
            <p class="m-0 text-text">To start a session you need two things.</p>
            <dl class="m-0 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-text-60">
              <dt>Relay address</dt>
              <dd class="m-0 text-text">Not set yet</dd>
              <dt>Your name</dt>
              <dd class="m-0 text-text">
                {name === "" ? "Your computer's user name" : name}
              </dd>
            </dl>
            <p class="m-0 text-text-60">
              Ask whoever runs your team's relay for its address, then enter it in
              Settings. To join someone else's session you only need their invite.
            </p>
          </div>
          <div class="flex flex-wrap gap-2 pt-1">
            <button
              type="button"
              class="mng-btn"
              data-testid="share-open-settings"
              onclick={openSettings}>Open Settings</button
            >
            <button
              type="button"
              class="mng-btn"
              data-testid="share-join"
              disabled={live.busy}
              onclick={() => then(() => void live.join())}>Join with an invite</button
            >
          </div>
        {:else}
          <p class="m-0 text-text-60" data-testid="share-ready">
            {file === null
              ? "Open a text file to share it."
              : `You will share ${file}. Sharing as ${name === "" ? "your computer's user name" : name}.`}
          </p>
          <div class="flex flex-wrap gap-2 pt-1">
            <button
              type="button"
              class="mng-btn"
              data-testid="share-start"
              disabled={file === null || live.busy}
              onclick={() => then(() => void live.share())}>Share this file</button
            >
            <button
              type="button"
              class="mng-btn"
              data-testid="share-join"
              disabled={live.busy}
              onclick={() => then(() => void live.join())}>Join with an invite</button
            >
          </div>
        {/if}
      {/if}
    </section>

    {#if live.active}
      <section
        class="space-y-2 border-t border-line px-3 py-3"
        aria-labelledby="share-invite-title"
      >
        <h2 id="share-invite-title" class="m-0 text-[13px] font-normal text-text">
          Invite link
        </h2>
        <p class="m-0 text-text-60">
          Anyone who has the link can edit this file with you. Send it only to the
          people you mean.
        </p>
        <button
          type="button"
          class="mng-btn"
          data-testid="share-copy"
          onclick={() => void live.copyInvite()}
        >
          {live.copied ? "Copied" : "Copy invite link"}
        </button>
      </section>
      <section class="border-t border-line px-3 py-3" aria-label="Leave">
        <button
          type="button"
          class="mng-btn"
          data-testid="share-leave"
          onclick={() => then(() => void live.leave())}>Leave live session</button
        >
      </section>
    {/if}
  </div>
{/if}
