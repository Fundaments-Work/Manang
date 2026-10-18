<script lang="ts">
  import { onMount, untrack } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";
  import {
    citationsApi,
    type CitationEntry,
    type CitationStyle,
    type CitationVerificationResult,
  } from "$lib/kernel/api";
  import { errorMessage, kernel, reportError } from "$lib/kernel";
  import { insertCitation } from "$lib/state/actions";
  import { notice } from "$lib/state/notice.svelte";
  import { settings } from "$lib/state/settings.svelte";
  import { settingsPage } from "$lib/state/settingsPage.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import {
    authorSummary,
    formatDifference,
    formatVenue,
    formatYear,
    verificationBadge,
    type BibEntry,
  } from "$lib/workbench/references";
  import { citationStyleOf, optionLabel } from "$lib/workbench/settingOptions";

  const api = citationsApi(kernel);

  let query = $state("");
  let entries = $state<CitationEntry[]>([]);
  let totalIndexed = $state(0);
  let loadedFilesCount = $state(0);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let expandedKeys = $state<Record<string, boolean>>({});
  let verifyingKeys = $state<Record<string, boolean>>({});
  let verificationResults = $state<Record<string, CitationVerificationResult>>({});
  let formattedCitations = $state<Record<string, string>>({});
  /**
   * The bibliography style, chosen in Settings (`citations.style`). Passed
   * explicitly so a change shows here at once; undefined lets the plugin use
   * its configured default.
   */
  const style: CitationStyle | undefined = $derived(
    citationStyleOf(settings.value("citations.style")),
  );
  const styleLabel = $derived(
    style === undefined ? "Default style" : optionLabel("citations.style", style),
  );
  let loadingFormatKeys = $state<Record<string, boolean>>({});

  let searchDebounceTimer: ReturnType<typeof setTimeout> | undefined;

  function isBibPath(p: string): boolean {
    const lower = p.toLowerCase();
    return (
      lower.endsWith(".bib") ||
      lower.endsWith(".biblatex") ||
      lower.endsWith(".csl") ||
      lower.endsWith(".csl.json") ||
      (lower.endsWith(".json") &&
        !lower.endsWith(".figure.json") &&
        !lower.endsWith("package.json") &&
        !lower.endsWith("tsconfig.json"))
    );
  }

  async function performSearch(q: string): Promise<void> {
    try {
      const res = await api.search({ query: q.trim(), limit: 100 });
      entries = [...res.results.map((r) => r.entry)];
    } catch (err) {
      error = errorMessage(err);
    }
  }

  function handleQueryInput(newQuery: string): void {
    query = newQuery;
    clearTimeout(searchDebounceTimer);
    searchDebounceTimer = setTimeout(() => {
      void performSearch(newQuery);
    }, 200);
  }

  async function loadBibliographyFiles(): Promise<void> {
    loading = true;
    error = null;
    try {
      const bibPaths = workspace.paths.filter(isBibPath);
      loadedFilesCount = bibPaths.length;

      const root = workspace.project?.root;
      if (bibPaths.length > 0 && root) {
        const fullPaths = bibPaths.map((p) => (p.startsWith("/") ? p : `${root}/${p}`));
        const res = await api.load({ paths: fullPaths });
        totalIndexed = res.total;
      } else if (bibPaths.length > 0) {
        const res = await api.load({ paths: bibPaths });
        totalIndexed = res.total;
      } else {
        totalIndexed = 0;
      }

      await performSearch(query);
    } catch (err) {
      error = errorMessage(err);
    } finally {
      loading = false;
    }
  }

  async function handleVerify(key: string): Promise<void> {
    verifyingKeys = { ...verifyingKeys, [key]: true };
    try {
      const res = await api.verify(key);
      verificationResults = { ...verificationResults, [key]: res };
    } catch (err) {
      verificationResults = {
        ...verificationResults,
        [key]: {
          status: "error",
          differences: [],
          message: errorMessage(err),
        },
      };
    } finally {
      const updated = { ...verifyingKeys };
      delete updated[key];
      verifyingKeys = updated;
    }
  }

  async function fetchFormatted(key: string): Promise<void> {
    loadingFormatKeys = { ...loadingFormatKeys, [key]: true };
    const asked = style;
    try {
      const res = await api.format(key, asked);
      // A newer style was chosen meanwhile; its own request fills this in.
      if (asked !== style) return;
      formattedCitations = { ...formattedCitations, [key]: res.formatted };
    } catch (err) {
      reportError(`could not format the reference ${key}`, err);
      notice.error(`Could not format ${key}: ${errorMessage(err)}`);
    } finally {
      const updated = { ...loadingFormatKeys };
      delete updated[key];
      loadingFormatKeys = updated;
    }
  }

  async function toggleDetails(key: string): Promise<void> {
    const isExpanded = !expandedKeys[key];
    expandedKeys = { ...expandedKeys, [key]: isExpanded };
    if (isExpanded && !formattedCitations[key]) {
      await fetchFormatted(key);
    }
  }

  // A new style re-formats what is open; the rest formats when opened.
  $effect(() => {
    void style;
    untrack(() => {
      formattedCitations = {};
      for (const [key, open] of Object.entries(expandedKeys))
        if (open) void fetchFormatted(key);
    });
  });

  async function copyFormattedCitation(key: string): Promise<void> {
    const text = formattedCitations[key];
    if (text) {
      try {
        await navigator.clipboard.writeText(text);
        notice.show("Formatted citation copied to clipboard.");
      } catch {
        notice.error("Could not copy citation to clipboard.");
      }
    }
  }

  function handleInsert(entry: CitationEntry): void {
    insertCitation(entry as unknown as BibEntry);
  }

  onMount(() => {
    void loadBibliographyFiles();

    let unlisten: (() => void) | undefined;
    kernel
      .onEvent((event) => {
        if (event.topic === "project.changed") {
          void loadBibliographyFiles();
        }
      })
      .then((u) => {
        unlisten = u;
      });

    return () => {
      clearTimeout(searchDebounceTimer);
      unlisten?.();
    };
  });
</script>

<div class="mng-panel-head">
  <h2>References</h2>
  <div class="flex items-center gap-1">
    <button
      type="button"
      class="mng-icon-btn"
      title="Reload bibliography files"
      aria-label="Reload bibliography files"
      onclick={() => void loadBibliographyFiles()}
    >
      <Icon name="refresh" size={13} />
    </button>
  </div>
</div>

<div class="px-3 pb-1">
  <input
    type="search"
    class="mng-field"
    placeholder="Filter by author, title or key"
    aria-label="Filter references"
    value={query}
    oninput={(e) => handleQueryInput(e.currentTarget.value)}
    spellcheck="false"
  />
</div>

<div class="min-h-0 flex-1 overflow-y-auto pb-4">
  {#if loading && entries.length === 0}
    <p class="m-0 px-4 pt-2 text-xs text-text-muted" role="status">
      Reading bibliography files…
    </p>
  {:else if error}
    <div class="m-3 border border-text-60 bg-surface p-3 text-xs text-text">
      <div class="flex items-center gap-1.5 text-text">
        <Icon name="error" size={13} />
        <span>Failed to load references</span>
      </div>
      <p class="mt-1 m-0 text-text-60 leading-relaxed">{error}</p>
      <button
        type="button"
        class="mng-btn mt-2 text-xs"
        onclick={() => void loadBibliographyFiles()}
      >
        Retry
      </button>
    </div>
  {:else if totalIndexed === 0}
    <div class="px-4 pt-3 text-xs leading-relaxed text-text-60">
      <p class="m-0">
        No references found. Add a <span class="font-mono text-[11px]">.bib</span> or CSL-JSON
        file to the project and its entries appear here, ready to cite.
      </p>
    </div>
  {:else}
    <p
      class="m-0 px-4 py-2 text-[11px] text-text-muted"
      role="status"
      aria-live="polite"
    >
      {#if query.trim() === ""}
        {totalIndexed}
        {totalIndexed === 1 ? "reference" : "references"} in {loadedFilesCount}
        {loadedFilesCount === 1 ? "file" : "files"}
      {:else}
        {entries.length} of {totalIndexed} match
      {/if}
    </p>

    {#if entries.length === 0}
      <p class="m-0 px-4 text-xs text-text-60">
        Nothing matches "{query.trim()}". Try an author's surname, year, or part of the
        title.
      </p>
    {/if}

    <ul class="m-0 list-none p-0" aria-label="References">
      {#each entries as entry (entry.key)}
        {@const isExpanded = expandedKeys[entry.key] ?? false}
        {@const isVerifying = verifyingKeys[entry.key] ?? false}
        {@const vResult = verificationResults[entry.key]}
        {@const vBadge = vResult ? verificationBadge(vResult.status) : null}
        {@const venueText = formatVenue(entry)}

        <li class="border-b border-line-subtle last:border-b-0">
          <div class="group px-3 py-2 hover:bg-surface-hi">
            <!-- Header: Authors and Year -->
            <div class="flex items-baseline justify-between gap-2">
              <span class="truncate text-xs text-text">
                {authorSummary(entry.authors)}
              </span>
              {#if entry.year !== undefined}
                <span class="shrink-0 font-mono text-[11px] text-text-60">
                  {formatYear(entry.year)}
                </span>
              {/if}
            </div>

            <!-- Title -->
            {#if entry.title}
              <div class="mt-0.5 line-clamp-2 text-[11px] leading-snug text-text-60">
                {entry.title}
              </div>
            {/if}

            <!-- Venue / Container -->
            {#if venueText}
              <div class="mt-0.5 truncate text-[10px] text-text-muted italic">
                {venueText}
              </div>
            {/if}

            <!-- Key & Entry type row -->
            <div class="mt-1 flex items-center justify-between gap-2">
              <span class="font-mono text-[10px] text-text-muted">
                {entry.key}
              </span>
              <span class="text-[10px] text-text-muted">
                {entry.entry_type}
              </span>
            </div>

            <!-- Verification result badge -->
            {#if vBadge}
              <div class="mt-1.5 flex items-center gap-1.5 text-[11px]">
                <span
                  class="inline-flex items-center gap-1 border px-1.5 py-0.5 font-mono text-[10px] {vBadge.colorClass}"
                  title={vBadge.description}
                >
                  <Icon name={vBadge.icon} size={10} />
                  <span>{vBadge.label}</span>
                </span>
                {#if vResult?.status === "error" && vResult.message}
                  <span class="truncate text-[10px] text-text">
                    {vResult.message}
                  </span>
                {/if}
              </div>

              <!-- Mismatch differences -->
              {#if vResult?.status === "mismatch" && vResult.differences.length > 0}
                <div class="mt-1 border border-line bg-surface p-1.5 text-[10px]">
                  <span class="text-text">Metadata differences:</span>
                  <ul class="m-0 mt-0.5 list-disc pl-3 text-text-60">
                    {#each vResult.differences as diff (diff.field)}
                      <li>{formatDifference(diff)}</li>
                    {/each}
                  </ul>
                </div>
              {/if}
            {/if}

            <!-- Action buttons -->
            <div class="mt-2 flex items-center gap-1.5">
              <button
                type="button"
                class="mng-btn text-[11px]"
                title="Insert citation at cursor"
                onclick={() => handleInsert(entry)}
              >
                Insert
              </button>
              <button
                type="button"
                class="mng-btn text-[11px]"
                title="Verify citation against Crossref/OpenAlex/arXiv"
                disabled={isVerifying}
                onclick={() => void handleVerify(entry.key)}
              >
                {#if isVerifying}
                  Verifying…
                {:else}
                  Verify
                {/if}
              </button>
              <button
                type="button"
                class="mng-icon-btn ml-auto"
                title={isExpanded ? "Collapse details" : "Expand details"}
                aria-label={isExpanded ? "Collapse details" : "Expand details"}
                onclick={() => void toggleDetails(entry.key)}
              >
                <Icon name={isExpanded ? "chevron-down" : "chevron-right"} size={12} />
              </button>
            </div>

            <!-- Expanded Details Pane -->
            {#if isExpanded}
              <div
                class="mt-2.5 border-t border-line-subtle pt-2 text-[11px] leading-relaxed text-text-60"
              >
                <!-- Full Authors -->
                {#if entry.authors && entry.authors.length > 0}
                  <div class="mb-1">
                    <span class="text-text">Authors:</span>
                    <span>
                      {entry.authors.map((a) => a.name).join(", ")}
                    </span>
                  </div>
                {/if}

                <!-- DOI -->
                {#if entry.doi}
                  <div class="mb-1 flex items-baseline gap-1">
                    <span class="text-text">DOI:</span>
                    <a
                      href="https://doi.org/{entry.doi}"
                      target="_blank"
                      rel="noreferrer"
                      class="truncate font-mono text-[10px] text-text-60 underline hover:text-text"
                    >
                      {entry.doi}
                    </a>
                  </div>
                {/if}

                <!-- URL -->
                {#if entry.url}
                  <div class="mb-1 flex items-baseline gap-1">
                    <span class="text-text">URL:</span>
                    <a
                      href={entry.url}
                      target="_blank"
                      rel="noreferrer"
                      class="truncate font-mono text-[10px] text-text-60 underline hover:text-text"
                    >
                      {entry.url}
                    </a>
                  </div>
                {/if}

                <!-- arXiv ID -->
                {#if entry.arxiv_id}
                  <div class="mb-1">
                    <span class="text-text">arXiv:</span>
                    <span class="font-mono text-[10px]">{entry.arxiv_id}</span>
                  </div>
                {/if}

                <!-- Abstract -->
                {#if entry.abstract_text}
                  <div class="mb-2 mt-1">
                    <span class="text-text">Abstract:</span>
                    <p class="m-0 mt-0.5 line-clamp-4 leading-normal text-text-muted">
                      {entry.abstract_text}
                    </p>
                  </div>
                {/if}

                <!-- Formatted Citation Preview -->
                <div class="mt-2 border border-line bg-surface p-2">
                  <div class="flex items-center justify-between pb-1">
                    <span class="text-[10px] text-text-muted">
                      Formatted citation
                    </span>
                    <div class="flex items-center gap-1">
                      <button
                        type="button"
                        class="px-1 py-0.5 text-[10px] text-text-muted hover:text-text"
                        title="Citation style: change it in Settings"
                        aria-label="Citation style {styleLabel}, change it in Settings"
                        data-citation-style={style ?? ""}
                        onclick={() => settingsPage.show("citations")}
                      >
                        {styleLabel}
                      </button>
                      {#if formattedCitations[entry.key]}
                        <button
                          type="button"
                          class="mng-icon-btn ml-1"
                          title="Copy formatted citation"
                          aria-label="Copy formatted citation"
                          onclick={() => void copyFormattedCitation(entry.key)}
                        >
                          <Icon name="copy" size={11} />
                        </button>
                      {/if}
                    </div>
                  </div>

                  {#if loadingFormatKeys[entry.key]}
                    <div class="py-1 text-[10px] text-text-muted">Formatting…</div>
                  {:else if formattedCitations[entry.key]}
                    <div class="text-[11px] leading-snug text-text select-text">
                      {formattedCitations[entry.key]}
                    </div>
                  {:else}
                    <button
                      type="button"
                      class="text-[10px] text-text-60 underline hover:text-text"
                      onclick={() => void fetchFormatted(entry.key)}
                    >
                      Generate formatted citation
                    </button>
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>
