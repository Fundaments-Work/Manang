<script lang="ts">
  /**
   * Writing surface — DESIGN.md §3.2.
   *
   * Placeholder for the CodeMirror 6 mount that lands in the next slice. The
   * gutter metrics, mono typography and math colour are already the real ones so
   * the layout can be reviewed before the editor is wired in; CodeMirror will
   * own this DOM subtree entirely once mounted, which is why the manuscript
   * never re-renders through the view framework.
   */
  interface Props {
    lines: readonly string[];
  }

  const { lines }: Props = $props();

  const first = 140;

  /** Math delimiters are the one place the maths token is allowed. */
  function isMath(line: string): boolean {
    return line.includes("$$");
  }
</script>

<div class="flex h-full min-w-0 flex-1 overflow-hidden bg-bg">
  <div
    class="h-full w-full overflow-y-auto px-4 py-3 font-mono text-[13px] leading-[1.6]"
    aria-label="Manuscript"
  >
    {#each lines as line, index (index)}
      <div class="flex gap-4">
        <span
          class="w-8 shrink-0 text-right text-text-muted tabular-nums"
          aria-hidden="true"
        >
          {first + index}
        </span>
        {#if isMath(line)}
          <span class="min-w-0 whitespace-pre-wrap text-math">{line}</span>
        {:else}
          <span class="min-w-0 whitespace-pre-wrap text-text">{line}</span>
        {/if}
      </div>
    {/each}
  </div>
</div>
