<script lang="ts">
  import { parseMarkdown, type Inline } from "$lib/agent/markdown";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { notice } from "$lib/state/notice.svelte";

  /**
   * An assistant message as flowing text: paragraphs, lists, inline code and code blocks with a
   * Copy button. Rendered from parsed data as text nodes (never HTML). While streaming, the
   * last block carries a quiet caret.
   */

  interface Props {
    text: string;
    streaming?: boolean;
  }

  const { text, streaming = false }: Props = $props();

  const blocks = $derived(parseMarkdown(text));

  async function copy(code: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(code);
      notice.show("Copied.");
    } catch {
      notice.show("Copy is not available here. Select the text instead.");
    }
  }
</script>

{#snippet inline(parts: readonly Inline[])}
  {#each parts as part, i (i)}
    {#if part.kind === "code"}<code class="bg-surface-hi px-1 font-mono text-[11px]"
        >{part.text}</code
      >{:else if part.kind === "strong"}<strong class="font-normal text-text"
        >{part.text}</strong
      >{:else if part.kind === "em"}<em>{part.text}</em>{:else}{part.text}{/if}
  {/each}
{/snippet}

<div class="selectable text-[13px] leading-relaxed text-text" data-agent-markdown>
  {#each blocks as block, i (i)}
    {@const lastBlock = i === blocks.length - 1}
    {#if block.kind === "paragraph"}
      <p class="m-0 mb-2 last:mb-0">
        {@render inline(block.inline)}{#if streaming && lastBlock}<span
            class="mng-caret"
            aria-hidden="true"
          ></span>{/if}
      </p>
    {:else if block.kind === "heading"}
      <p class="m-0 mb-1 text-text">{@render inline(block.inline)}</p>
    {:else if block.kind === "quote"}
      <p class="m-0 mb-2 border-l border-line pl-3 text-text-60">
        {@render inline(block.inline)}
      </p>
    {:else if block.kind === "list"}
      {#if block.ordered}
        <ol class="m-0 mb-2 list-decimal pl-5">
          {#each block.items as item, j (j)}<li>{@render inline(item)}</li>{/each}
        </ol>
      {:else}
        <ul class="m-0 mb-2 list-disc pl-5">
          {#each block.items as item, j (j)}<li>{@render inline(item)}</li>{/each}
        </ul>
      {/if}
    {:else}
      <div class="mb-2 border border-line bg-bg" data-agent-code>
        <div
          class="flex h-6 items-center justify-between border-b border-line pr-1 pl-2"
        >
          <span class="font-mono text-[10px] text-text-muted"
            >{block.lang || "text"}</span
          >
          <button
            type="button"
            class="mng-icon-btn h-6 w-6"
            aria-label="Copy code"
            title="Copy"
            onclick={() => void copy(block.text)}
          >
            <Icon name="copy" size={12} />
          </button>
        </div>
        <pre
          class="m-0 overflow-x-auto px-2.5 py-2 font-mono text-[11px] leading-relaxed">{block.text}{#if streaming && lastBlock}<span
              class="mng-caret"
              aria-hidden="true"></span>{/if}</pre>
      </div>
    {/if}
  {/each}
  {#if streaming && blocks.length === 0}<span class="mng-caret" aria-hidden="true"
    ></span>{/if}
</div>

<style>
  .mng-caret {
    display: inline-block;
    width: 0.5ch;
    height: 1em;
    margin-left: 1px;
    vertical-align: text-bottom;
    background: var(--mng-text-60);
    animation: mng-caret 1s steps(2, start) infinite;
  }
  @keyframes mng-caret {
    to {
      visibility: hidden;
    }
  }
</style>
