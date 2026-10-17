<script lang="ts">
  import { onMount } from "svelte";

  import { goToMatch, openFile } from "$lib/state/actions";
  import { documents } from "$lib/state/documents.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { basename, dirname, fileKind } from "$lib/workbench/paths";
  import { searchNames, searchText, type TextMatch } from "$lib/workbench/search";

  /**
   * Search: file names (fuzzy) and text inside files. Open documents are
   * searched as they are on screen, unsaved edits included; other text files are
   * read through the project plugin, up to a cap, so a huge project cannot stall
   * the panel.
   */

  const MAX_FILES = 80;
  const TEXT_KINDS = new Set(["typst", "latex", "markdown", "bibliography", "text"]);

  interface FileHits {
    readonly path: string;
    readonly matches: readonly TextMatch[];
  }

  let query = $state("");
  let names = $state.raw<string[]>([]);
  let hits = $state.raw<FileHits[]>([]);
  let searching = $state(false);
  let capped = $state(false);
  let ran = $state("");
  let input: HTMLInputElement | undefined = $state();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let token = 0;

  onMount(() => {
    void workspace.ensureAll();
    input?.focus();
  });

  $effect(() => {
    const q = query.trim();
    // Re-run when the file list arrives, too.
    void workspace.paths.length;
    clearTimeout(timer);
    if (q === "") {
      names = [];
      hits = [];
      ran = "";
      return;
    }
    timer = setTimeout(() => void run(q), 200);
    return () => clearTimeout(timer);
  });

  async function run(q: string): Promise<void> {
    const mine = ++token;
    searching = true;
    names = searchNames(workspace.paths, q);
    const textFiles = workspace.paths.filter((p) => TEXT_KINDS.has(fileKind(p)));
    capped = textFiles.length > MAX_FILES;
    const found: FileHits[] = [];
    for (const path of textFiles.slice(0, MAX_FILES)) {
      const open = documents.get(path);
      let text: string;
      if (open !== undefined) text = open.text();
      else {
        try {
          text = await workspace.readFile(path);
        } catch {
          continue;
        }
      }
      if (mine !== token) return;
      const matches = searchText(text, q);
      if (matches.length > 0) found.push({ path, matches });
    }
    if (mine !== token) return;
    hits = found;
    ran = q;
    searching = false;
  }

  const total = $derived(hits.reduce((n, h) => n + h.matches.length, 0));
</script>

<div class="mng-panel-head"><h2>Search</h2></div>

<div class="px-3 pb-2">
  <input
    bind:this={input}
    bind:value={query}
    type="search"
    class="mng-field"
    placeholder="Search file names and text"
    aria-label="Search project"
    spellcheck="false"
    onkeydown={(event) => {
      if (event.key === "Escape") query = "";
    }}
  />
</div>

<div class="min-h-0 flex-1 overflow-y-auto pb-4">
  <p class="m-0 px-4 pb-2 text-[11px] text-text-muted" role="status" aria-live="polite">
    {#if query.trim() === ""}
      Type to search. Open documents are searched with your unsaved edits.
    {:else if searching}
      Searching
    {:else if names.length === 0 && total === 0}
      No results for "{ran}". Check the spelling, or try a shorter word.
    {:else}
      {total}
      {total === 1 ? "match" : "matches"} in {hits.length}
      {hits.length === 1 ? "file" : "files"}
      {#if names.length > 0}and {names.length} matching {names.length === 1
          ? "name"
          : "names"}{/if}
      {#if capped}, first {MAX_FILES} files only{/if}
    {/if}
  </p>

  {#if names.length > 0}
    <section aria-label="File name matches">
      <h3 class="m-0 px-4 pt-2 pb-1 text-[11px] font-normal text-text-60">
        File names
      </h3>
      <ul class="m-0 list-none p-0">
        {#each names as path (path)}
          <li>
            <button
              type="button"
              class="inset-focus flex h-[26px] w-full items-center gap-2 px-4 text-left text-xs text-text-60 hover:bg-surface-hi hover:text-text"
              onclick={() => void openFile(path)}
            >
              <span class="truncate">{basename(path)}</span>
              {#if dirname(path) !== ""}<span class="truncate text-text-muted"
                  >{dirname(path)}</span
                >{/if}
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#each hits as file (file.path)}
    <section aria-label="Matches in {file.path}" class="mt-1">
      <h3
        class="m-0 flex items-baseline gap-2 px-4 pt-2 pb-1 text-[11px] font-normal text-text-60"
      >
        <span class="truncate">{basename(file.path)}</span>
        {#if dirname(file.path) !== ""}<span class="truncate text-text-muted"
            >{dirname(file.path)}</span
          >{/if}
        <span class="ml-auto font-mono text-text-muted">{file.matches.length}</span>
      </h3>
      <ul class="m-0 list-none p-0">
        {#each file.matches as match (`${match.line}:${match.column}`)}
          <li>
            <button
              type="button"
              class="inset-focus flex w-full items-baseline gap-2 px-4 py-1 text-left text-xs text-text-60 hover:bg-surface-hi hover:text-text"
              onclick={() => void goToMatch(file.path, match.line, match.column)}
            >
              <span
                class="w-6 shrink-0 text-right font-mono text-[10px] text-text-muted"
                >{match.line}</span
              >
              <span class="truncate font-mono text-[11px]"
                >{match.before}<mark class="bg-math/25 text-text">{match.hit}</mark
                >{match.after}</span
              >
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/each}
</div>
