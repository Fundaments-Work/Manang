<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import type { PdfView } from "$lib/state/pdfView.svelte";
  import { countLabel } from "$lib/workbench/pdf/find";

  /**
   * Find in PDF: a small bar inside the viewer (not the editor's search).
   * Live search as you type; Enter / Shift+Enter or the arrows move between
   * matches, Esc closes and gives the focus back to the pages.
   */
  interface Props {
    view: PdfView;
    /** Where the focus goes when the bar closes. */
    onclose: () => void;
  }

  const { view, onclose }: Props = $props();

  let input: HTMLInputElement | undefined = $state();

  $effect(() => {
    void view.findFocus;
    input?.focus();
    input?.select();
  });

  const label = $derived(countLabel(view.findCurrent, view.findTotal, view.findQuery));

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      view.stepFind(event.shiftKey ? -1 : 1);
    } else if (event.key === "Escape") {
      view.closeFind();
      onclose();
    } else if (event.key === "f" && (event.ctrlKey || event.metaKey)) {
      input?.select();
    } else {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
  }
</script>

<div
  class="absolute top-2 right-4 z-20 flex h-8 items-center gap-0.5 border border-line bg-surface pr-0.5 pl-2"
  role="search"
  aria-label="Find in PDF"
  data-pdf-find
>
  <input
    bind:this={input}
    bind:value={view.findQuery}
    class="h-6 w-36 min-w-0 bg-transparent font-mono text-[11px] text-text outline-none placeholder:text-text-60"
    type="text"
    placeholder="Find in PDF"
    aria-label="Find in PDF"
    aria-describedby="mng-pdf-find-count"
    spellcheck="false"
    autocomplete="off"
    {onkeydown}
  />
  <span
    id="mng-pdf-find-count"
    class="min-w-16 px-1 text-right font-mono text-[11px] whitespace-nowrap text-text-60"
    aria-live="polite"
    data-pdf-find-count>{label}</span
  >
  <button
    type="button"
    class="mng-icon-btn h-6 w-6 font-mono text-[11px]"
    aria-label="Match case"
    title="Match case"
    aria-pressed={view.findCase}
    onclick={() => (view.findCase = !view.findCase)}>Aa</button
  >
  <button
    type="button"
    class="mng-icon-btn h-6 w-6"
    aria-label="Previous match"
    title="Previous match (Shift+Enter)"
    disabled={view.findTotal === 0}
    onclick={() => view.stepFind(-1)}
  >
    <Icon name="arrowUp" size={13} />
  </button>
  <button
    type="button"
    class="mng-icon-btn h-6 w-6"
    aria-label="Next match"
    title="Next match (Enter)"
    disabled={view.findTotal === 0}
    onclick={() => view.stepFind(1)}
  >
    <Icon name="arrowDown" size={13} />
  </button>
  <button
    type="button"
    class="mng-icon-btn h-6 w-6"
    aria-label="Close find"
    title="Close (Esc)"
    onclick={() => {
      view.closeFind();
      onclose();
    }}
  >
    <Icon name="close" size={13} />
  </button>
</div>
