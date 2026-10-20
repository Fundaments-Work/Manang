<script lang="ts">
  import { tick } from "svelte";

  import SharePanel from "$lib/components/SharePanel.svelte";
  import { SLOT_COLOURS, SLOT_COUNT } from "$lib/editor/live/remote";
  import type { LiveScope } from "$lib/kernel/liveApi";
  import { layout } from "$lib/state/layout.svelte";
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
  /** What to share when starting: the open file, or everything. */
  let chosen = $state<LiveScope | null>(null);
  const scope = $derived<LiveScope>(chosen ?? (file === null ? "project" : "file"));
  const SCOPES: readonly { id: LiveScope; label: string }[] = [
    { id: "file", label: "This file" },
    { id: "project", label: "Whole project" },
  ];
  const canStart = $derived(
    scope === "file" ? file !== null : workspace.project !== null,
  );
  const slotOf = (n: number): number => ((n % SLOT_COUNT) + SLOT_COUNT) % SLOT_COUNT;
  /** The file a person has in front of them, for "in <file>". */
  const whereIs = (path: string | null | undefined): string | null => {
    const at = path ?? (live.scope === "file" ? live.path : null);
    return at === null || at === undefined ? null : basename(at);
  };
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

  function onscopekey(event: KeyboardEvent): void {
    const forward = event.key === "ArrowRight" || event.key === "ArrowDown";
    if (!forward && event.key !== "ArrowLeft" && event.key !== "ArrowUp") return;
    event.preventDefault();
    chosen = scope === "file" ? "project" : "file";
    void tick().then(() =>
      root?.querySelector<HTMLElement>(`[data-scope="${chosen}"]`)?.focus(),
    );
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
          {live.scope === "project"
            ? "Everyone in the session is editing this project."
            : `Everyone in the session is typing in ${live.path === null ? "the same file" : basename(live.path)}.`}
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
          <div
            class="flex border border-line"
            role="radiogroup"
            aria-label="What to share"
            data-testid="share-scope"
          >
            {#each SCOPES as option, index (option.id)}
              <button
                type="button"
                role="radio"
                aria-checked={scope === option.id}
                tabindex={scope === option.id ? 0 : -1}
                data-scope={option.id}
                class="inset-focus h-7 min-w-0 flex-1 truncate px-2 text-xs {index > 0
                  ? 'border-l border-line'
                  : ''} {scope === option.id
                  ? 'bg-surface-hi text-text'
                  : 'text-text-60 hover:text-text'}"
                onclick={() => (chosen = option.id)}
                onkeydown={onscopekey}>{option.label}</button
              >
            {/each}
          </div>
          <p class="m-0 text-text-60" data-testid="share-ready">
            {#if scope === "file"}
              {file === null
                ? "Open a text file to share it."
                : `You will share ${file}. Sharing as ${name === "" ? "your computer's user name" : name}.`}
            {:else}
              {workspace.project === null
                ? "Open a project to share it."
                : `You will share every file in this project. Anyone you invite sees what differs from their own copy before anything changes. Sharing as ${name === "" ? "your computer's user name" : name}.`}
            {/if}
          </p>
          <div class="flex flex-wrap gap-2 pt-1">
            <button
              type="button"
              class="mng-btn"
              data-testid="share-start"
              disabled={!canStart || live.busy}
              onclick={() => then(() => void live.share(scope))}
              >{scope === "file"
                ? "Share this file"
                : "Share the whole project"}</button
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
        aria-labelledby="share-people-title"
        data-testid="share-participants"
      >
        <h2 id="share-people-title" class="m-0 text-[13px] font-normal text-text">
          People here
        </h2>
        <ul class="m-0 list-none space-y-1 p-0">
          <li class="flex items-center gap-2 text-text">
            <span
              class="inline-block h-1.5 w-1.5 shrink-0 bg-text outline outline-1 outline-line"
              aria-hidden="true"
            ></span>
            <span class="min-w-0 flex-1 truncate">You</span>
          </li>
          {#each live.peers as peer (peer.peer)}
            {@const at = whereIs(peer.path)}
            {@const followed = live.following === peer.peer}
            <li class="flex items-center gap-2" data-peer={peer.peer}>
              <span
                class="inline-block h-1.5 w-1.5 shrink-0 outline outline-1 outline-line"
                style="background: {SLOT_COLOURS[slotOf(peer.slot)]}"
                aria-hidden="true"
              ></span>
              <span class="min-w-0 flex-1 truncate">
                <span class="text-text">{peer.name}</span>
                {#if at !== null}
                  <span class="font-mono text-[11px] text-text-60"> in {at}</span>
                {/if}
              </span>
              <button
                type="button"
                class="inset-focus shrink-0 px-1 text-[11px] text-math hover:underline"
                data-testid="share-follow"
                aria-pressed={followed}
                aria-label="{followed ? 'Stop following' : 'Follow'} {peer.name}"
                onclick={() => live.follow(followed ? null : peer.peer)}
                >{followed ? "Stop following" : "Follow"}</button
              >
            </li>
          {/each}
          {#if live.peers.length === 0}
            <li class="text-text-60">Only you so far. Send the invite link below.</li>
          {/if}
        </ul>
      </section>
      <section
        class="space-y-2 border-t border-line px-3 py-3"
        aria-labelledby="share-invite-title"
      >
        <h2 id="share-invite-title" class="m-0 text-[13px] font-normal text-text">
          Invite link
        </h2>
        <p class="m-0 text-text-60">
          Anyone who has the link can edit {live.scope === "project"
            ? "this project"
            : "this file"} with you. Send it only to the people you mean.
        </p>
        <button
          type="button"
          class="mng-btn border-accent bg-accent text-accent-ink"
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

    <section
      class="space-y-2 border-t border-line px-3 py-3"
      aria-labelledby="share-versions-title"
    >
      <h2 id="share-versions-title" class="m-0 text-[13px] font-normal text-text">
        Share versions
      </h2>
      <p class="m-0 text-text-60">
        Send the versions you saved to your co-authors, and get theirs. One turn at a
        time, not live.
      </p>
      <SharePanel
        embedded
        quiet={live.active || layout.sideView === "source-control"}
      />
    </section>
  </div>
{/if}
