<script lang="ts">
  import { document } from "$lib/state/document.svelte";

  /**
   * Status bar.
   *
   * DESIGN.md §10: speed indicators are hidden — no millisecond counters,
   * because speed is an invisible reality and nobody wants telemetry in their
   * reading column. What remains is the document's own state.
   */
</script>

<footer
  class="flex h-7 shrink-0 items-center gap-4 border-t border-line px-4 font-mono text-[10px] text-text-muted"
>
  <span class="text-text-60">{document.meta.path}</span>
  <span>{document.meta.language}</span>
  <span>{document.lineCount} lines</span>
  <span>{document.wordCount} words</span>

  {#if document.diagnostics.length > 0}
    <span class="ml-auto text-text-60">
      {document.diagnostics.length}
      {document.diagnostics.length === 1 ? "whisper" : "whispers"}
    </span>
  {/if}
</footer>
