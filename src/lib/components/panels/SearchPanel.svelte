<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";

  import Icon from "$lib/components/ui/Icon.svelte";
  import { kernel, errorMessage } from "$lib/kernel";
  import {
    searchApi,
    type SearchFileMatches,
    type SearchFileResult,
  } from "$lib/kernel/api";
  import { goToMatch, openFile } from "$lib/state/actions";
  import { notice } from "$lib/state/notice.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { basename, dirname } from "$lib/workbench/paths";
  import {
    formatMatchCount,
    parsePatternList,
    splitLineByRanges,
  } from "$lib/workbench/search";

  /**
   * Search: project-wide file names (`search.files`) and content (`search.text`),
   * with regex, case sensitivity, word boundary, include/exclude glob filters,
   * collapsible per-file match groups, and atomic replace (`search.replace-preview/apply`).
   */

  const api = searchApi(kernel);

  let query = $state("");
  let replaceText = $state("");
  let showReplace = $state(false);
  let showFilters = $state(false);
  let includePattern = $state("");
  let excludePattern = $state("");

  // Options
  let matchCase = $state(false);
  let wholeWord = $state(false);
  let useRegex = $state(false);

  // Results
  let fileHits = $state.raw<SearchFileMatches[]>([]);
  let nameHits = $state.raw<SearchFileResult[]>([]);
  let searching = $state(false);
  let replacing = $state(false);
  let truncated = $state(false);
  let searchError = $state("");
  let replaceMessage = $state("");
  let ranQuery = $state("");
  let collapsedFiles = new SvelteSet<string>();

  let inputEl: HTMLInputElement | undefined = $state();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let searchToken = 0;

  const projectRoot = $derived(workspace.project?.root ?? "");
  const totalMatches = $derived(fileHits.reduce((n, f) => n + f.matches.length, 0));

  onMount(() => {
    inputEl?.focus();
  });

  $effect(() => {
    // Re-trigger search when query or options change
    const q = query.trim();
    const root = projectRoot;
    const cs = matchCase;
    const ww = wholeWord;
    const rx = useRegex;
    const inc = includePattern.trim();
    const exc = excludePattern.trim();

    clearTimeout(timer);
    if (q === "" || root === "") {
      fileHits = [];
      nameHits = [];
      searching = false;
      searchError = "";
      truncated = false;
      ranQuery = "";
      return;
    }

    timer = setTimeout(() => {
      void executeSearch(q, root, cs, ww, rx, inc, exc);
    }, 200);

    return () => clearTimeout(timer);
  });

  async function executeSearch(
    q: string,
    root: string,
    cs: boolean,
    ww: boolean,
    rx: boolean,
    inc: string,
    exc: string,
  ): Promise<void> {
    const token = ++searchToken;
    searching = true;
    searchError = "";
    replaceMessage = "";

    try {
      // Cancel previous in-flight search
      await api.cancel();

      const include = parsePatternList(inc);
      const exclude = parsePatternList(exc);

      const [textRes, filesRes] = await Promise.all([
        api.text({
          root,
          query: q,
          case_sensitive: cs,
          whole_word: ww,
          regex: rx,
          include: include.length > 0 ? include : undefined,
          exclude: exclude.length > 0 ? exclude : undefined,
          limit: 500,
        }),
        api.files({
          root,
          query: q,
          limit: 20,
        }),
      ]);

      if (token !== searchToken) return;

      fileHits = textRes.files as SearchFileMatches[];
      nameHits = filesRes.results as SearchFileResult[];
      truncated = textRes.truncated;
      ranQuery = q;
    } catch (error) {
      if (token !== searchToken) return;
      searchError = errorMessage(error);
      fileHits = [];
      nameHits = [];
    } finally {
      if (token === searchToken) {
        searching = false;
      }
    }
  }

  function toggleFileCollapse(path: string): void {
    if (collapsedFiles.has(path)) {
      collapsedFiles.delete(path);
    } else {
      collapsedFiles.add(path);
    }
  }

  function toggleCollapseAll(): void {
    if (collapsedFiles.size === fileHits.length) {
      collapsedFiles.clear();
    } else {
      for (const f of fileHits) collapsedFiles.add(f.path);
    }
  }

  async function handleReplaceAll(): Promise<void> {
    const q = query.trim();
    const root = projectRoot;
    if (q === "" || root === "" || replacing) return;

    replacing = true;
    replaceMessage = "";

    try {
      const include = parsePatternList(includePattern);
      const exclude = parsePatternList(excludePattern);

      const preview = await api.replacePreview({
        root,
        query: q,
        replacement: replaceText,
        case_sensitive: matchCase,
        whole_word: wholeWord,
        regex: useRegex,
        include: include.length > 0 ? include : undefined,
        exclude: exclude.length > 0 ? exclude : undefined,
      });

      if (preview.total_edits === 0) {
        replaceMessage = "No matches to replace.";
        return;
      }

      const apply = await api.replaceApply({
        plan_id: preview.plan_id,
      });

      if (apply.failed > 0) {
        const failedFiles = apply.files
          .filter((f) => !f.ok)
          .map((f) => `${basename(f.path)}: ${f.error ?? "failed"}`)
          .join(", ");
        replaceMessage = `Replaced in ${apply.applied} files. ${apply.failed} failed: ${failedFiles}`;
        notice.error(replaceMessage);
      } else {
        replaceMessage = `Replaced ${preview.total_edits} ${preview.total_edits === 1 ? "occurrence" : "occurrences"} across ${apply.applied} ${apply.applied === 1 ? "file" : "files"}.`;
        notice.show(replaceMessage);
      }

      // Re-run search after replace
      void executeSearch(
        query.trim(),
        projectRoot,
        matchCase,
        wholeWord,
        useRegex,
        includePattern.trim(),
        excludePattern.trim(),
      );
    } catch (error) {
      replaceMessage = `Replace failed: ${errorMessage(error)}`;
      notice.error(replaceMessage);
    } finally {
      replacing = false;
    }
  }
</script>

<div class="mng-panel-head">
  <h2>Search</h2>
  {#if fileHits.length > 0}
    <button
      type="button"
      class="mng-icon-btn"
      title={collapsedFiles.size === fileHits.length ? "Expand all" : "Collapse all"}
      aria-label={collapsedFiles.size === fileHits.length
        ? "Expand all"
        : "Collapse all"}
      onclick={toggleCollapseAll}
    >
      <Icon name="collapse-all" size={14} />
    </button>
  {/if}
</div>

<div class="space-y-2 border-b border-line px-3 pb-3">
  <!-- Query row with replace toggle -->
  <div class="flex items-center gap-1.5">
    <button
      type="button"
      class="mng-icon-btn h-7 w-7 shrink-0 text-text-60 hover:text-text"
      title={showReplace ? "Hide replace" : "Show replace"}
      aria-label={showReplace ? "Hide replace" : "Show replace"}
      aria-expanded={showReplace}
      onclick={() => (showReplace = !showReplace)}
    >
      <Icon name={showReplace ? "chevron-down" : "chevron-right"} size={14} />
    </button>

    <div class="relative flex-1">
      <input
        bind:this={inputEl}
        bind:value={query}
        type="search"
        class="mng-field pr-20"
        placeholder="Search text or file names"
        aria-label="Search project"
        spellcheck="false"
        onkeydown={(event) => {
          if (event.key === "Escape") query = "";
        }}
      />
      <!-- Inline search options -->
      <div class="absolute right-1 top-1/2 flex -translate-y-1/2 items-center gap-0.5">
        <button
          type="button"
          class="flex h-5 w-5 items-center justify-center font-mono text-[10px] transition-colors {matchCase
            ? 'bg-surface-hi text-text'
            : 'text-text-muted hover:text-text'}"
          title="Match case (Alt+C)"
          aria-pressed={matchCase}
          onclick={() => (matchCase = !matchCase)}
        >
          Aa
        </button>
        <button
          type="button"
          class="flex h-5 w-5 items-center justify-center font-mono text-[10px] transition-colors {wholeWord
            ? 'bg-surface-hi text-text'
            : 'text-text-muted hover:text-text'}"
          title="Match whole word (Alt+W)"
          aria-pressed={wholeWord}
          onclick={() => (wholeWord = !wholeWord)}
        >
          \b
        </button>
        <button
          type="button"
          class="flex h-5 w-5 items-center justify-center font-mono text-[10px] transition-colors {useRegex
            ? 'bg-surface-hi text-text'
            : 'text-text-muted hover:text-text'}"
          title="Use regular expression (Alt+R)"
          aria-pressed={useRegex}
          onclick={() => (useRegex = !useRegex)}
        >
          .*
        </button>
      </div>
    </div>
  </div>

  <!-- Replace row -->
  {#if showReplace}
    <div class="flex items-center gap-1.5 pl-8.5">
      <input
        bind:value={replaceText}
        type="text"
        class="mng-field flex-1"
        placeholder="Replace"
        aria-label="Replacement text"
        spellcheck="false"
      />
      <button
        type="button"
        class="mng-btn shrink-0"
        disabled={query.trim() === "" || totalMatches === 0 || replacing}
        onclick={() => void handleReplaceAll()}
      >
        {replacing ? "Replacing…" : "Replace all"}
      </button>
    </div>
  {/if}

  <!-- Toggle include/exclude files filter -->
  <div>
    <button
      type="button"
      class="flex items-center gap-1 text-[11px] text-text-muted hover:text-text-60"
      aria-expanded={showFilters}
      onclick={() => (showFilters = !showFilters)}
    >
      <Icon name={showFilters ? "chevron-down" : "chevron-right"} size={12} />
      <span>Files to include / exclude</span>
    </button>

    {#if showFilters}
      <div class="mt-1.5 space-y-1.5 pl-4">
        <div>
          <label for="search-include" class="block text-[10px] text-text-muted"
            >Files to include</label
          >
          <input
            id="search-include"
            bind:value={includePattern}
            type="text"
            class="mng-field mt-0.5 text-xs"
            placeholder="e.g. *.typ, *.md"
            spellcheck="false"
          />
        </div>
        <div>
          <label for="search-exclude" class="block text-[10px] text-text-muted"
            >Files to exclude</label
          >
          <input
            id="search-exclude"
            bind:value={excludePattern}
            type="text"
            class="mng-field mt-0.5 text-xs"
            placeholder="e.g. *.svg, dist/*"
            spellcheck="false"
          />
        </div>
      </div>
    {/if}
  </div>
</div>

<!-- Results stream -->
<div class="min-h-0 flex-1 overflow-y-auto pb-4">
  <!-- Status line -->
  <p class="m-0 px-4 py-2 text-[11px] text-text-muted" role="status" aria-live="polite">
    {#if projectRoot === ""}
      Open a folder to search its files.
    {:else if query.trim() === ""}
      Type to search file names and text.
    {:else if searching}
      Searching…
    {:else if searchError !== ""}
      {searchError}. Check regex pattern or syntax.
    {:else if nameHits.length === 0 && totalMatches === 0}
      No results for "{ranQuery}". Check the spelling, or try another query.
    {:else}
      {formatMatchCount(totalMatches, fileHits.length)}
      {#if nameHits.length > 0}
        and {nameHits.length} matching {nameHits.length === 1 ? "name" : "names"}
      {/if}
      {#if truncated}
        <span class="text-text-60"> (results truncated)</span>
      {/if}
    {/if}
  </p>

  {#if replaceMessage !== ""}
    <p class="m-0 border-b border-line px-4 pb-2 text-[11px] text-text-60">
      {replaceMessage}
    </p>
  {/if}

  <!-- File name matches -->
  {#if nameHits.length > 0}
    <section aria-label="File name matches" class="mb-2">
      <h3 class="m-0 px-4 pt-2 pb-1 text-[11px] font-normal text-text-60">
        File names
      </h3>
      <ul class="m-0 list-none p-0">
        {#each nameHits as hit (hit.path)}
          <li>
            <button
              type="button"
              class="inset-focus flex h-[26px] w-full items-center gap-2 px-4 text-left text-xs text-text-60 hover:bg-surface-hi hover:text-text"
              onclick={() => void openFile(hit.path)}
            >
              <Icon name="file" size={13} class="text-text-muted shrink-0" />
              <span class="truncate">{basename(hit.path)}</span>
              {#if dirname(hit.path) !== ""}
                <span class="truncate text-[11px] text-text-muted"
                  >{dirname(hit.path)}</span
                >
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  <!-- Text matches per file -->
  {#each fileHits as file (file.path)}
    {@const isCollapsed = collapsedFiles.has(file.path)}
    <section aria-label="Matches in {file.path}" class="mt-1">
      <button
        type="button"
        class="flex w-full items-center gap-1.5 px-4 pt-1.5 pb-1 text-left text-[11px] font-normal text-text-60 hover:bg-surface-hi hover:text-text"
        onclick={() => toggleFileCollapse(file.path)}
      >
        <Icon
          name={isCollapsed ? "chevron-right" : "chevron-down"}
          size={12}
          class="shrink-0 text-text-muted"
        />
        <span class="truncate font-normal text-text">{basename(file.path)}</span>
        {#if dirname(file.path) !== ""}
          <span class="truncate text-text-muted">{dirname(file.path)}</span>
        {/if}
        <span class="ml-auto font-mono text-[10px] text-text-muted">
          {file.matches.length}
        </span>
      </button>

      {#if !isCollapsed}
        <ul class="m-0 list-none p-0">
          {#each file.matches as match (`${file.path}:${match.line}:${match.column}`)}
            {@const segments = splitLineByRanges(
              match.text,
              match.ranges,
              match.text_offset ?? 0,
            )}
            <li>
              <button
                type="button"
                class="inset-focus flex w-full items-baseline gap-2 px-4 py-1 text-left text-xs text-text-60 hover:bg-surface-hi hover:text-text"
                onclick={() => void goToMatch(file.path, match.line, match.column)}
              >
                <span
                  class="w-6 shrink-0 text-right font-mono text-[10px] text-text-muted"
                >
                  {match.line}
                </span>
                <span class="truncate font-mono text-[11px]">
                  {#each segments as seg, idx (idx)}
                    {#if seg.isMatch}
                      <mark class="bg-math/25 text-text">{seg.text}</mark>
                    {:else}
                      {seg.text}
                    {/if}
                  {/each}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/each}
</div>
