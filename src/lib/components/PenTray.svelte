<script lang="ts">
  import { ink, TOOLS, type PenTool } from "$lib/state/figure.svelte";
  import { desk } from "$lib/state/desk.svelte";

  /**
   * The Pen Tray — DESIGN.md §4.1.
   *
   * A centred floating bar, h-9, carbon on hairline, zero radius. State is
   * carried by the label and the accent border rather than by colour alone.
   */

  function select(tool: PenTool): void {
    ink.setTool(tool);
  }
</script>

<div class="pointer-events-none absolute inset-x-0 bottom-6 flex justify-center">
  <div
    class="pointer-events-auto flex h-9 items-center gap-px border border-line bg-bg"
    role="toolbar"
    aria-label="Pen tray"
  >
    <button
      type="button"
      class="flex h-full items-center gap-1.5 px-3 font-sans text-xs text-text-60 transition-colors duration-fast ease-manang hover:bg-surface-hi hover:text-text"
      class:text-accent={ink.tool === "select"}
      class:border-y={false}
      aria-pressed={ink.tool === "select"}
      onclick={() => select("select")}
    >
      <svg
        viewBox="0 0 16 16"
        width="11"
        height="11"
        fill="none"
        stroke="currentColor"
        stroke-width="1.25"
        aria-hidden="true"
      >
        <path d="M3 3 L3 13 L6 10.5 L8 13.5 L9.5 12.5 L7.5 9.5 L11 9.5 Z" />
      </svg>
      Select
    </button>

    <span class="h-4 w-px self-center bg-line" aria-hidden="true"></span>

    {#each TOOLS as { tool, label, glyph } (tool)}
      <button
        type="button"
        class="flex h-full items-center gap-1.5 px-3 font-sans text-xs text-text-60 transition-colors duration-fast ease-manang hover:bg-surface-hi hover:text-text"
        class:bg-surface-hi={ink.tool === tool}
        class:text-accent={ink.tool === tool}
        aria-pressed={ink.tool === tool}
        onclick={() => select(tool)}
      >
        <span class="font-mono text-[11px]" aria-hidden="true">{glyph}</span>
        {label}
      </button>
    {/each}

    <span class="h-4 w-px self-center bg-line" aria-hidden="true"></span>

    <button
      type="button"
      class="flex h-full items-center gap-1.5 px-3 font-sans text-xs text-text-60 transition-colors duration-fast ease-manang hover:bg-surface-hi hover:text-text"
      onclick={() => ink.clear()}
    >
      <svg
        viewBox="0 0 16 16"
        width="11"
        height="11"
        fill="none"
        stroke="currentColor"
        stroke-width="1.25"
        aria-hidden="true"
      >
        <path d="M3.5 4.5 H12.5" />
        <path d="M6 4.5 V3 H10 V4.5" />
        <path d="M5 4.5 L5.5 13 H10.5 L11 4.5" />
      </svg>
      Clear
    </button>

    <button
      type="button"
      class="flex h-full items-center gap-1.5 border-l border-line bg-surface px-3 font-sans text-xs text-text transition-colors duration-fast ease-manang hover:bg-surface-hi"
      onclick={() => desk.toggleCanvas()}
    >
      Done
      <span class="font-mono text-[10px] text-text-muted">↵</span>
    </button>
  </div>
</div>
