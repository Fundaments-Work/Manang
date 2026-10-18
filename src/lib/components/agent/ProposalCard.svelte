<script lang="ts">
  import { describeEdit, diffSegments } from "$lib/agent/proposal";
  import type { TextEdit } from "$lib/agent/types";

  /**
   * One proposal as a diff card: what is removed (dim, struck through) and what is inserted
   * (Sage), the model's one-line reason, and Accept and Reject. Accept is the Agent view's one
   * lime action while a proposal waits. Accepting applies ONE editor transaction.
   */

  interface Props {
    edit: TextEdit;
    state: "pending" | "accepted" | "rejected" | "stale";
    onaccept: () => void;
    onreject: () => void;
  }

  const { edit, state, onaccept, onreject }: Props = $props();

  const segments = $derived(diffSegments(edit.before, edit.after));
  const verdict = $derived(
    state === "accepted"
      ? "Accepted. Ctrl+Z undoes it."
      : state === "rejected"
        ? "Rejected."
        : state === "stale"
          ? "Dropped: the text changed since this was proposed."
          : "",
  );
</script>

<div class="my-2 border border-line bg-bg" data-proposal data-proposal-state={state}>
  <p class="m-0 border-b border-line px-2.5 py-1.5 text-[11px] text-text-60">
    {describeEdit(edit)}
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
    {/if}
  </div>
</div>
