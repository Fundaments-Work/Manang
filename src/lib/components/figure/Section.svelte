<script lang="ts">
  import type { Snippet } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";
  import { figureUi } from "$lib/state/figureUi.svelte";

  /**
   * One collapsible inspector section: a hairline header with the title and a
   * chevron, and a body. Collapsed state is remembered per user.
   */
  interface Props {
    id: string;
    title: string;
    children: Snippet;
    /** Small controls kept in the header (always visible). */
    actions?: Snippet;
  }

  const { id, title, children, actions }: Props = $props();

  const open = $derived(!figureUi.isCollapsed(id));
</script>

<section class="border-b border-line" data-section={id} aria-label={title}>
  <div class="flex h-7 items-center pr-1">
    <button
      type="button"
      class="mng-lime-hover flex h-full min-w-0 flex-1 items-center gap-1 bg-transparent pr-1 pl-2 text-left text-[11px] text-text"
      aria-expanded={open}
      aria-controls="section-body-{id}"
      onclick={() => figureUi.toggleSection(id)}
    >
      <Icon name={open ? "chevron-down" : "chevron-right"} size={11} />
      <span class="truncate">{title}</span>
    </button>
    {#if actions !== undefined}{@render actions()}{/if}
  </div>
  {#if open}
    <div id="section-body-{id}" class="flex flex-col gap-1.5 px-2 pt-0.5 pb-2.5">
      {@render children()}
    </div>
  {/if}
</section>
