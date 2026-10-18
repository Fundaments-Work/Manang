<script lang="ts">
  import { diffSegments } from "$lib/agent/proposal";
  import type { TextEdit } from "$lib/agent/types";

  /**
   * One proposed edit as a diff card: the file, how many lines come out and go in, the change
   * (removed text dim and struck through, inserted text in Sage), the model's one-line reason,
   * then Accept (the one lime action while a proposal waits) and Reject. After Accept the card
   * offers Undo. Accepting applies ONE editor transaction.
   */

  interface Props {
    edit: TextEdit;
    state: "pending" | "accepted" | "rejected" | "stale" | "undone";
    onaccept: () => void;
    onreject: () => void;
    onundo: () => void;
  }

  const { edit, state, onaccept, onreject, onundo }: Props = $props();

  const segments = $derived(diffSegments(edit.before, edit.after));
  const lines = (text: string): number => (text === "" ? 0 : text.split("\n").length);
  const removed = $derived(lines(edit.before));
  const added = $derived(lines(edit.after));
  const verdict = $derived(
    state === "accepted"
      ? "Applied."
      : state === "undone"
        ? "Applied, then undone."
        : state === "rejected"
          ? "Rejected."
          : state === "stale"
            ? "Dropped: the text changed since this was proposed."
            : "",
  );
</script>

<div class="my-2 border border-line bg-bg" data-proposal data-proposal-state={state}>
  <p
    class="m-0 flex items-baseline gap-2 border-b border-line px-2.5 py-1.5 text-[11px]"
  >
    <span class="min-w-0 flex-1 truncate font-mono text-text" title={edit.path}
      >{edit.path}</span
    >
    <span
      class="shrink-0 font-mono text-text-60"
      aria-label="{added} lines added, {removed} lines removed"
      >+{added} −{removed}</span
    >
  </p>
  <pre
    class="selectable m-0 max-h-40 overflow-auto px-2.5 py-2 font-mono text-[11px] leading-relaxed break-words whitespace-pre-wrap"
    aria-label="Proposed change">{#each segments as segment, i (i)}{#if segment.kind === "removed"}<span
          class="mng-ai-removed"
          data-diff="removed">{segment.text}</span
        >{:else if segment.kind === "inserted"}<span
          class="mng-ai-inserted"
          data-diff="inserted">{segment.text}</span
        >{:else}<span class="text-text-60">{segment.text}</span>{/if}{/each}</pre>
  {#if edit.explanation !== ""}
    <p class="m-0 px-2.5 pb-2 text-xs text-text-60">{edit.explanation}</p>
  {/if}
  <div class="flex items-center gap-2 border-t border-line px-2.5 py-1.5">
    {#if state === "pending"}
      <button
        type="button"
        class="mng-btn border-accent bg-accent text-accent-ink"
        data-agent-accept
        onclick={onaccept}>Accept</button
      >
      <button type="button" class="mng-btn" data-agent-reject onclick={onreject}
        >Reject</button
      >
    {:else}
      <span class="text-[11px] text-text-60" role="status">{verdict}</span>
      {#if state === "accepted"}
        <button type="button" class="mng-btn ml-auto" data-agent-undo onclick={onundo}
          >Undo</button
        >
      {/if}
    {/if}
  </div>
</div>
