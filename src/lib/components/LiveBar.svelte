<script lang="ts">
  import { SLOT_COLOURS, SLOT_COUNT, SLOT_STROKES } from "$lib/editor/live/remote";
  import { live } from "$lib/state/live.svelte";
  import {
    connectionDetail,
    connectionLabel,
    NO_RELAY_MESSAGE,
    participantSummary,
  } from "$lib/workbench/live";

  /**
   * The bar above an editor that is in a live session (or that was asked to
   * share and has no relay). It states the connection, lists the people with
   * the colour and stroke of their caret, and offers Copy invite and Leave.
   * Nothing here is the view's one signal-colour action: the person is typing.
   */
  interface Props {
    path: string;
  }

  const { path }: Props = $props();

  const inSession = $derived(live.isLive(path));
  const conn = $derived(live.connection === "off" ? "connecting" : live.connection);
  const label = $derived(connectionLabel(conn, live.attempt));
  const detail = $derived(connectionDetail(conn, live.error));
  const slot = (n: number): number => ((n % SLOT_COUNT) + SLOT_COUNT) % SLOT_COUNT;

  let inviteField: HTMLInputElement | undefined = $state();
  $effect(() => {
    if (live.inviteText !== null) inviteField?.select();
  });
</script>

{#if inSession}
  <div
    class="border-b border-line bg-surface text-xs"
    role="region"
    aria-label="Live session"
    data-testid="live-bar"
    data-state={conn}
    data-role={live.role}
  >
    <div class="flex flex-wrap items-center gap-x-3 gap-y-2 px-3 py-2">
      <p class="m-0 flex items-center gap-2 text-text" role="status">
        <span
          class="inline-block size-2 border border-text"
          class:bg-text={conn === "connected"}
          class:border-dashed={conn === "reconnecting"}
          aria-hidden="true"
        ></span>
        <span data-testid="live-state">{label}</span>
      </p>

      <ul
        class="m-0 flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-1 p-0 text-text-60"
        aria-label="People in this session"
      >
        <li class="list-none">You</li>
        {#each live.peers as peer (peer.peer)}
          <li class="flex list-none items-center gap-1.5" data-peer={peer.peer}>
            <span
              class="inline-block h-3 border-l-2"
              style="border-color: {SLOT_COLOURS[
                slot(peer.slot)
              ]}; border-left-style: {SLOT_STROKES[slot(peer.slot)]}"
              aria-hidden="true"
            ></span>
            <span>{peer.name}</span>
          </li>
        {/each}
        {#if live.peers.length === 0 && conn === "connected"}
          <li class="list-none text-text-muted">{participantSummary(live.peers)}</li>
        {/if}
      </ul>

      {#if live.inviteText !== null}
        <input
          bind:this={inviteField}
          class="mng-field min-w-0 max-w-[260px] flex-1 text-[11px]"
          readonly
          value={live.inviteText}
          aria-label="Invite"
          data-testid="live-invite"
          onfocus={(e) => e.currentTarget.select()}
        />
      {/if}
      <button
        type="button"
        class="mng-btn"
        data-testid="live-copy"
        onclick={() => void live.copyInvite()}
      >
        {live.copied ? "Copied" : "Copy invite"}
      </button>
      <button
        type="button"
        class="mng-btn"
        data-testid="live-leave"
        onclick={() => void live.leave()}>Leave</button
      >
    </div>
    {#if detail !== null}
      <p
        class="m-0 border-t border-line px-3 py-1.5 text-[11px] text-text-60"
        role={conn === "error" ? "alert" : "status"}
        data-testid="live-detail"
      >
        {detail}
      </p>
    {/if}
  </div>
{:else if live.needsRelay && !live.active}
  <div
    class="border-b border-line bg-surface text-xs"
    role="status"
    data-testid="live-setup"
  >
    <div class="flex flex-wrap items-center gap-x-3 gap-y-2 px-3 py-2">
      <p class="m-0 min-w-0 flex-1 text-text-60">
        {NO_RELAY_MESSAGE} A relay only passes on encrypted data; it cannot read your text.
      </p>
      <button
        type="button"
        class="mng-btn"
        data-testid="live-open-settings"
        onclick={() => live.openRelaySettings()}>Open settings</button
      >
      <button type="button" class="mng-btn" onclick={() => live.dismissRelayHint()}
        >Dismiss</button
      >
    </div>
  </div>
{/if}
