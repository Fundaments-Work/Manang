<script lang="ts">
  import { onDestroy, onMount } from "svelte";

  import { kernel } from "$lib/kernel";
  import { goToMatch } from "$lib/state/actions";
  import { documents } from "$lib/state/documents.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { getDocumentOutline, type OutlineItem } from "$lib/workbench/outline";
  import { basename } from "$lib/workbench/paths";

  /**
   * Outline panel: extracts and displays headings and labels for the active
   * document (Typst, LaTeX, or Markdown). Updates while typing with 300ms debounce.
   */

  let items = $state<OutlineItem[]>([]);
  let loading = $state(false);
  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  const activeDoc = $derived.by(() => {
    const active = tabs.active;
    if (active === undefined || active.kind !== "document") return null;
    return active;
  });

  const activePath = $derived(activeDoc?.path ?? null);
  const activeName = $derived(activePath ? basename(activePath) : null);
  const activeEngine = $derived(activePath ? workspace.engineFor(activePath) : "plain");

  async function updateOutline(immediate = false): Promise<void> {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }

    if (!activePath) {
      items = [];
      loading = false;
      return;
    }

    const run = async (): Promise<void> => {
      const session = documents.get(activePath);
      let text: string;
      if (session) {
        text = session.text();
      } else {
        try {
          text = await workspace.readFile(activePath);
        } catch {
          items = [];
          loading = false;
          return;
        }
      }

      try {
        const parsed = await getDocumentOutline(text, activeEngine, kernel);
        items = parsed;
      } catch {
        items = [];
      } finally {
        loading = false;
      }
    };

    if (immediate) {
      loading = true;
      await run();
    } else {
      loading = true;
      debounceTimer = setTimeout(() => {
        void run();
      }, 300);
    }
  }

  // React to active document or engine changes
  $effect(() => {
    if (activePath && activeEngine) {
      void updateOutline(true);
    } else {
      items = [];
      loading = false;
    }
  });

  onMount(() => {
    // Listen for edits to any open document
    documents.onEdit(() => {
      void updateOutline(false);
    });
  });

  onDestroy(() => {
    if (debounceTimer) {
      clearTimeout(debounceTimer);
      debounceTimer = null;
    }
  });
</script>

<div class="mng-panel-head">
  <h2>Outline</h2>
  {#if activeName}
    <span
      class="max-w-[140px] truncate text-[11px] text-text-muted"
      title={activePath ?? ""}
    >
      {activeName}
    </span>
  {/if}
</div>

<div class="min-h-0 flex-1 overflow-y-auto pb-4">
  {#if activePath === null}
    <div class="px-4 text-xs leading-relaxed text-text-60">
      <p class="m-0">No document open.</p>
      <p class="mt-2 text-text-muted">
        Open a Typst (<span class="font-mono text-[11px]">.typ</span>), LaTeX (<span
          class="font-mono text-[11px]">.tex</span
        >), or Markdown (<span class="font-mono text-[11px]">.md</span>) document
        to see its heading outline.
      </p>
    </div>
  {:else if loading && items.length === 0}
    <p class="m-0 px-4 text-xs text-text-muted" role="status">Parsing outline…</p>
  {:else if items.length === 0}
    <div class="px-4 text-xs leading-relaxed text-text-60">
      <p class="m-0">No headings found.</p>
      <p class="mt-2 text-text-muted">
        Headings using <span class="font-mono text-[11px]">= Heading</span> in Typst,
        <span class="font-mono text-[11px]">\section&#123;...&#125;</span> in LaTeX,
        or <span class="font-mono text-[11px]"># Heading</span> in Markdown will appear
        here.
      </p>
    </div>
  {:else}
    <nav aria-label="Document outline">
      <ul class="m-0 list-none p-0">
        {#each items as item, i (item.line + ":" + item.column + ":" + i)}
          <li>
            <button
              type="button"
              class="inset-focus flex min-h-[28px] w-full items-center gap-2 py-1 pr-3 text-left text-xs hover:bg-surface-hi text-text-60 hover:text-text transition-colors"
              style="padding-left: {Math.max(0.75, (item.level - 1) * 0.75 + 0.75)}rem"
              onclick={() => void goToMatch(activePath, item.line, item.column)}
              title="Go to line {item.line}, column {item.column}"
            >
              <span
                class="shrink-0 font-mono text-[10px] text-text-muted"
                aria-hidden="true"
              >
                H{item.level}
              </span>
              <span class="truncate flex-1 font-normal text-text">
                {item.title}
              </span>
              {#if item.label}
                <span
                  class="shrink-0 max-w-[80px] truncate font-mono text-[10px] text-accent/80 bg-surface-hi px-1 border border-line"
                  title="Label: {item.label}"
                >
                  {item.label}
                </span>
              {/if}
              <span class="shrink-0 font-mono text-[10px] text-text-muted">
                L{item.line}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    </nav>
  {/if}
</div>
