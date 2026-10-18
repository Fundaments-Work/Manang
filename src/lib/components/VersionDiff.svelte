<script lang="ts">
  import type { GitFileDiff } from "$lib/kernel/api";

  /**
   * One file's diff, read-only: hunks with old and new line numbers, additions
   * and removals tinted tonally. Used by the Versions panel; line-level only.
   */
  let { diff }: { diff: GitFileDiff } = $props();
</script>

<div
  class="max-h-72 overflow-auto border-y border-line font-mono text-[11px] leading-relaxed"
  role="region"
  aria-label="Changes in {diff.path}"
>
  {#if diff.binary}
    <p class="m-0 px-3 py-2 text-text-60">A binary file: no text to compare.</p>
  {:else if diff.too_large}
    <p class="m-0 px-3 py-2 text-text-60">Too large to compare here.</p>
  {:else if diff.hunks.length === 0}
    <p class="m-0 px-3 py-2 text-text-60">No changes in the text.</p>
  {:else}
    {#each diff.hunks as hunk, hIdx (hIdx)}
      <div
        class="border-b border-line/60 bg-surface px-2 py-0.5 text-[10px] text-text-muted"
      >
        {hunk.header}
      </div>
      {#each hunk.lines as line, lIdx (lIdx)}
        <div
          class="flex items-baseline px-2 py-0.5 {line.kind === 'add'
            ? 'bg-math/20 text-text'
            : line.kind === 'remove'
              ? 'bg-text/20 text-text'
              : 'text-text-60'}"
        >
          <span class="w-7 shrink-0 select-none text-right text-[10px] text-text-muted">
            {line.old_line ?? ""}
          </span>
          <span class="w-7 shrink-0 select-none text-right text-[10px] text-text-muted">
            {line.new_line ?? ""}
          </span>
          <span class="w-4 select-none text-center">
            {line.kind === "add" ? "+" : line.kind === "remove" ? "-" : " "}
          </span>
          <span class="whitespace-pre">{line.text}</span>
        </div>
      {/each}
    {/each}
  {/if}
</div>
