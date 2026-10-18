<script lang="ts">
  import { tick } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";
  import {
    ALL_DOCS,
    getDoc,
    searchDocs,
    type HelpDoc,
    type SearchMatch,
  } from "$lib/help/docs";
  import { render as mockRender } from "$lib/kernel/mockMarkdown";
  import { kernel } from "$lib/kernel";
  import { markdownApi } from "$lib/kernel/api";
  import { helpPage } from "$lib/state/helpPage.svelte";

  const query = $derived(helpPage.query.trim());
  const activeDoc: HelpDoc = $derived(
    getDoc(helpPage.activeSlug) ?? ALL_DOCS[0] ?? {
      slug: "index",
      title: "User Guide",
      order: 0,
      raw: "# User Guide\n\nNo documentation found.",
    },
  );

  const searchResults: SearchMatch[] = $derived(
    query !== "" ? searchDocs(query, ALL_DOCS) : [],
  );

  let renderedHtml = $state("");
  let container: HTMLElement | undefined = $state();

  $effect(() => {
    const raw = activeDoc.raw;
    let cancelled = false;

    markdownApi(kernel)
      .renderHtml(raw)
      .then((res) => {
        if (!cancelled) renderedHtml = res.html;
      })
      .catch(() => {
        if (!cancelled) renderedHtml = mockRender(raw).html;
      });

    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    void activeDoc.slug;
    if (container !== undefined) {
      container.scrollTop = 0;
    }
  });

  function onContentClick(event: MouseEvent): void {
    const target = (event.target as HTMLElement | null)?.closest("a");
    if (!target) return;
    const href = target.getAttribute("href");
    if (href === null) return;

    if (href.endsWith(".md") || href.includes(".md#")) {
      event.preventDefault();
      const parts = href.split("#");
      const file = parts[0] ?? "";
      const hash = parts[1];
      const slug = file.replace(/\.md$/, "");
      if (getDoc(slug) !== undefined) {
        helpPage.select(slug);
        if (hash !== undefined && hash !== "") {
          void tick().then(() => {
            const el = document.getElementById(hash);
            el?.scrollIntoView({ behavior: "smooth" });
          });
        }
      }
    } else if (href.startsWith("#")) {
      event.preventDefault();
      const id = href.slice(1);
      const el = document.getElementById(id);
      el?.scrollIntoView({ behavior: "smooth" });
    }
  }
</script>

<div
  class="relative flex h-full w-full overflow-hidden bg-bg text-text"
  data-testid="help-page"
>
  <!-- Sidebar navigation of documentation articles -->
  <aside
    class="flex w-60 shrink-0 flex-col border-r border-line bg-surface max-[768px]:w-48"
    aria-label="Documentation navigation"
  >
    <div
      class="flex h-10 items-center justify-between border-b border-line px-3 text-xs text-text-60"
    >
      <span class="font-normal text-text">User guide</span>
      <span class="text-[11px] text-text-muted">{ALL_DOCS.length} articles</span>
    </div>

    <nav class="flex-1 overflow-y-auto py-1">
      {#each ALL_DOCS as doc (doc.slug)}
        {@const selected = doc.slug === activeDoc.slug && query === ""}
        <button
          type="button"
          class="flex w-full items-center border-l-2 px-3 py-1.5 text-left text-xs transition-colors {selected
            ? 'border-accent bg-surface-hi text-text'
            : 'border-transparent text-text-60 hover:bg-surface-hi hover:text-text'}"
          onclick={() => helpPage.select(doc.slug)}
        >
          <span class="truncate">{doc.title}</span>
        </button>
      {/each}
    </nav>
  </aside>

  <!-- Reader area / Search results -->
  <main
    bind:this={container}
    class="flex flex-1 flex-col overflow-y-auto bg-bg"
    aria-label="Documentation viewer"
  >
    <!-- Top header with active document title and close button -->
    <header
      class="sticky top-0 z-10 flex h-10 shrink-0 items-center justify-between border-b border-line bg-bg/95 px-6 backdrop-blur"
    >
      <div class="flex items-center gap-2 text-xs text-text-60">
        <span>Help</span>
        <span>/</span>
        <span class="text-text">{query !== "" ? `Search results` : activeDoc.title}</span>
      </div>

      <button
        type="button"
        class="mng-btn flex items-center gap-1.5 px-2 py-1 text-xs"
        onclick={() => helpPage.hide()}
        title="Close (Esc)"
        aria-label="Close user guide"
      >
        <Icon name="close" size={13} />
        <span>Close</span>
        <span class="mng-kbd">Esc</span>
      </button>
    </header>

    <div class="mx-auto w-full max-w-3xl px-6 py-6">
      {#if query !== ""}
        <!-- Search Results View -->
        <div class="flex flex-col gap-4">
          <p class="m-0 text-xs text-text-60">
            {searchResults.length} {searchResults.length === 1 ? "result" : "results"} for "{query}"
          </p>

          {#if searchResults.length === 0}
            <div class="border border-line bg-surface p-6 text-center text-xs text-text-60">
              <p class="m-0 text-text">No articles found matching "{query}".</p>
              <p class="mt-2 mb-0 text-text-muted">
                Try searching for topics like Typst, shortcuts, preview, or figures.
              </p>
            </div>
          {:else}
            <div class="flex flex-col divide-y divide-line border border-line bg-surface">
              {#each searchResults as match (match.doc.slug + match.snippet)}
                <button
                  type="button"
                  class="flex flex-col gap-1.5 p-4 text-left transition-colors hover:bg-surface-hi"
                  onclick={() => helpPage.select(match.doc.slug)}
                >
                  <span class="text-xs font-normal text-text">{match.doc.title}</span>
                  <span class="text-xs text-text-60 leading-relaxed font-sans line-clamp-2">
                    {match.snippet}
                  </span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {:else}
        <!-- Document Content View -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
        <article
          class="mng-help-prose text-xs leading-relaxed"
          onclick={onContentClick}
        >
          <!-- eslint-disable-next-line svelte/no-at-html-tags -->
          {@html renderedHtml}
        </article>
      {/if}
    </div>
  </main>
</div>

<style>
  :global(.mng-help-prose) {
    color: var(--color-text-60);
  }

  :global(.mng-help-prose h1) {
    font-size: 1.25rem;
    font-weight: 400;
    color: var(--color-text);
    margin-top: 0;
    margin-bottom: 1.25rem;
    padding-bottom: 0.5rem;
    border-bottom: 1px solid var(--color-line);
  }

  :global(.mng-help-prose h2) {
    font-size: 1rem;
    font-weight: 400;
    color: var(--color-text);
    margin-top: 1.75rem;
    margin-bottom: 0.75rem;
  }

  :global(.mng-help-prose h3) {
    font-size: 0.875rem;
    font-weight: 400;
    color: var(--color-text);
    margin-top: 1.25rem;
    margin-bottom: 0.5rem;
  }

  :global(.mng-help-prose p) {
    margin-top: 0;
    margin-bottom: 0.875rem;
    line-height: 1.6;
  }

  :global(.mng-help-prose a) {
    color: var(--color-text);
    text-decoration: underline;
    text-decoration-color: var(--color-line);
    text-underline-offset: 2px;
  }

  :global(.mng-help-prose a:hover) {
    color: var(--color-accent);
    text-decoration-color: var(--color-accent);
  }

  :global(.mng-help-prose ul) {
    list-style-type: disc;
    padding-left: 1.25rem;
    margin-top: 0;
    margin-bottom: 0.875rem;
  }

  :global(.mng-help-prose ol) {
    list-style-type: decimal;
    padding-left: 1.25rem;
    margin-top: 0;
    margin-bottom: 0.875rem;
  }

  :global(.mng-help-prose li) {
    margin-bottom: 0.35rem;
    line-height: 1.5;
  }

  :global(.mng-help-prose pre) {
    background-color: var(--color-surface);
    border: 1px solid var(--color-line);
    padding: 0.75rem;
    margin-top: 0.75rem;
    margin-bottom: 0.875rem;
    overflow-x: auto;
    font-family: var(--font-mono, monospace);
  }

  :global(.mng-help-prose code) {
    font-family: var(--font-mono, monospace);
    font-size: 0.75rem;
    background-color: var(--color-surface);
    border: 1px solid var(--color-line-subtle);
    padding: 0.1rem 0.3rem;
    color: var(--color-text);
  }

  :global(.mng-help-prose pre code) {
    background-color: transparent;
    border: none;
    padding: 0;
  }

  :global(.mng-help-prose blockquote) {
    border-left: 2px solid var(--color-line);
    padding-left: 0.875rem;
    margin-left: 0;
    margin-right: 0;
    margin-top: 0.75rem;
    margin-bottom: 0.875rem;
    color: var(--color-text-muted);
  }

  :global(.mng-help-prose hr) {
    border: none;
    border-top: 1px solid var(--color-line);
    margin-top: 1.5rem;
    margin-bottom: 1.5rem;
  }
</style>
