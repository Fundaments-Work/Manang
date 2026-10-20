<script lang="ts">
  import { SLOT_COLOURS, SLOT_COUNT } from "$lib/editor/live/remote";
  import type { LivePeer } from "$lib/kernel/liveApi";

  /**
   * Who has this file in front of them: one square per person in their
   * collaborator colour, at most `max`, the rest folded into "+N". Colour is
   * never the only signal: the group and each square carry the names.
   */
  interface Props {
    peers: readonly LivePeer[];
    max?: number;
    /** Where it is drawn, for the test hooks ("tab" or "row"). */
    where: string;
  }

  const { peers, max = 3, where }: Props = $props();

  const shown = $derived(peers.slice(0, max));
  const hidden = $derived(peers.slice(max));
  const slotOf = (n: number): number => ((n % SLOT_COUNT) + SLOT_COUNT) % SLOT_COUNT;
  const names = $derived(peers.map((p) => p.name).join(", "));
</script>

{#if peers.length > 0}
  <span
    class="inline-flex shrink-0 items-center gap-[3px]"
    role="img"
    aria-label="Here now: {names}"
    title={names}
    data-testid="presence-{where}"
  >
    {#each shown as peer (peer.peer)}
      <span
        class="inline-block h-1.5 w-1.5 shrink-0 outline outline-1 outline-line"
        style="background: {SLOT_COLOURS[slotOf(peer.slot)]}"
        data-peer={peer.peer}
        title={peer.name}
      ></span>
    {/each}
    {#if hidden.length > 0}
      <span class="font-mono text-[10px] leading-none text-text-60"
        >+{hidden.length}</span
      >
    {/if}
  </span>
{/if}
