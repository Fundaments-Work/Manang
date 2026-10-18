<script lang="ts">
  import { describeEdit, sizeOf } from "$lib/agent/proposal";
  import type { TextEdit } from "$lib/agent/types";

  /**
   * A proposal in the chat is one quiet row: "Edit to main.typ, 2 changes", the model's one-line
   * reason, Review (brings the diff into view in the editor, where it lives) and, while it waits,
   * Accept (the one lime action) and Reject. After Accept the row offers Undo. Accepting applies
   * every hunk as ONE editor transaction.
   */

  interface Props {
    edit: TextEdit;
    state: "pending" | "accepted" | "rejected" | "stale" | "undone";
    onreview: () => void;
    onaccept: () => void;
    onreject: () => void;
    onundo: () => void;
  }

  const { edit, state, onreview, onaccept, onreject, onundo }: Props = $props();

  const size = $derived(sizeOf(edit));
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

<div class="my-2 border border-line px-2.5 py-2" data-proposal data-proposal-state={state}>
  <p class="m-0 flex items-baseline gap-2 text-xs text-text">
    <span
      class="min-w-0 flex-1 truncate"
      title="{size.removed} characters out, {size.added} in">{describeEdit(edit)}</span
    >
    {#if state === "pending"}
      <button
        type="button"
        class="mng-lime-hover inset-focus shrink-0 text-[11px] text-text-60 underline"
        data-agent-review
        onclick={onreview}>Review</button
      >
    {/if}
  </p>
  {#if edit.note !== ""}
    <p class="m-0 mt-1 text-xs text-text-60">{edit.note}</p>
  {/if}
  <div class="mt-2 flex items-center gap-2">
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
