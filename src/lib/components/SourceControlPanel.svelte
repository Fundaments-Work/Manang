<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";

  import Icon from "$lib/components/ui/Icon.svelte";
  import { kernel } from "$lib/kernel";
  import {
    gitApi,
    type GitBranchInfo,
    type GitCommitInfo,
    type GitFileDiff,
    type GitStatusFile,
    type GitStatusReport,
  } from "$lib/kernel/api";
  import VersionsPanel from "$lib/components/VersionsPanel.svelte";
  import { notice } from "$lib/state/notice.svelte";
  import { scm } from "$lib/state/scm.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { basename, dirname } from "$lib/workbench/paths";
  import {
    formatRelativeTime,
    gitActionableError,
    groupGitFiles,
    parseGitError,
    statusBadge,
  } from "$lib/workbench/git";

  /**
   * SourceControlPanel: status, stage/unstage, discard with confirmation,
   * commit with amend toggle, branch switcher, log history, sync, and diff viewer.
   */

  const api = gitApi(kernel);

  let status = $state.raw<GitStatusReport | null>(null);
  let branches = $state.raw<GitBranchInfo[]>([]);
  let commits = $state.raw<GitCommitInfo[]>([]);
  let hasMoreCommits = $state(false);

  let loading = $state(false);
  let syncing = $state(false);
  let committing = $state(false);
  let isRepo = $state(true);
  let errorMessageText = $state("");
  let commitReason = $state("");

  // Commit form
  let commitMessage = $state("");
  let amend = $state(false);

  // Branch creation / switching
  let showBranchMenu = $state(false);
  let newBranchName = $state("");
  let creatingBranch = $state(false);

  // Diff inspection
  let activeDiff = $state.raw<GitFileDiff | null>(null);

  // Discard confirmation
  let confirmingDiscardPath = $state<string | null>(null);

  // Collapsed sections
  let collapsed = new SvelteSet<string>();
  let showHistory = $state(false);

  const projectRoot = $derived(workspace.project?.root ?? "");
  const groups = $derived(groupGitFiles(status?.files ?? []));
  const stagedCount = $derived(groups.staged.length);
  const changesCount = $derived(groups.changes.length);
  const untrackedCount = $derived(groups.untracked.length);
  const conflictedCount = $derived(groups.conflicted.length);

  const canCommit = $derived(
    commitMessage.trim() !== "" && (stagedCount > 0 || amend) && !committing,
  );

  $effect(() => {
    if (commitMessage.trim() === "") {
      commitReason = "Enter a commit message.";
    } else if (stagedCount === 0 && !amend) {
      commitReason = "No staged changes to commit. Stage changes first.";
    } else {
      commitReason = "";
    }
  });

  onMount(() => {
    void refresh();

    const unsubGit = kernel.onEvent((ev) => {
      if (ev.topic === "git.changed" || ev.topic === "project.changed") {
        void refresh();
      }
    });

    return () => {
      void unsubGit.then((u) => u());
    };
  });

  async function refresh(): Promise<void> {
    const root = projectRoot;
    if (root === "") return;

    loading = true;
    errorMessageText = "";

    try {
      const openRes = await api.open(root);
      isRepo = openRes.is_repo;
      if (!isRepo) {
        status = null;
        return;
      }

      const [statusRes, branchesRes, logRes] = await Promise.all([
        api.status({ root }),
        api.branches({ root }),
        api.log({ root, limit: 15 }),
      ]);

      status = statusRes;
      branches = branchesRes as GitBranchInfo[];
      commits = logRes.commits as GitCommitInfo[];
      hasMoreCommits = logRes.has_more;
      isRepo = true;
    } catch (error) {
      const parsed = parseGitError(error);
      if (parsed.code === "not-a-repo") {
        isRepo = false;
        status = null;
      } else {
        errorMessageText = gitActionableError(parsed.code, parsed.message);
      }
    } finally {
      loading = false;
    }
  }

  async function handleInit(): Promise<void> {
    const root = projectRoot;
    if (root === "") return;
    try {
      loading = true;
      await api.init({ root });
      notice.show("Initialized Git repository.");
      await refresh();
    } catch (error) {
      const parsed = parseGitError(error);
      errorMessageText = gitActionableError(parsed.code, parsed.message);
      notice.error(errorMessageText);
    } finally {
      loading = false;
    }
  }

  async function handleStage(paths: string[]): Promise<void> {
    const root = projectRoot;
    if (root === "") return;
    try {
      await api.stage({ paths, root });
      await refresh();
    } catch (error) {
      const parsed = parseGitError(error);
      notice.error(gitActionableError(parsed.code, parsed.message));
    }
  }

  async function handleUnstage(paths: string[]): Promise<void> {
    const root = projectRoot;
    if (root === "") return;
    try {
      await api.unstage({ paths, root });
      await refresh();
    } catch (error) {
      const parsed = parseGitError(error);
      notice.error(gitActionableError(parsed.code, parsed.message));
    }
  }

  async function handleDiscard(paths: string[]): Promise<void> {
    const root = projectRoot;
    if (root === "") return;
    try {
      await api.discard({ paths, confirm: true, root });
      confirmingDiscardPath = null;
      notice.show(
        `Discarded changes in ${paths.length === 1 ? basename(paths[0]!) : `${paths.length} files`}.`,
      );
      await refresh();
    } catch (error) {
      const parsed = parseGitError(error);
      notice.error(gitActionableError(parsed.code, parsed.message));
    }
  }

  async function handleCommit(): Promise<void> {
    const root = projectRoot;
    const msg = commitMessage.trim();
    if (root === "" || msg === "" || committing) return;

    committing = true;
    errorMessageText = "";

    try {
      const res = await api.commit({
        message: msg,
        amend,
        root,
      });
      commitMessage = "";
      amend = false;
      notice.show(`Committed ${res.short}: ${res.subject}`);
      await refresh();
    } catch (error) {
      const parsed = parseGitError(error);
      errorMessageText = gitActionableError(parsed.code, parsed.message);
      notice.error(errorMessageText);
    } finally {
      committing = false;
    }
  }

  async function handleCheckout(name: string, create = false): Promise<void> {
    const root = projectRoot;
    if (root === "") return;
    try {
      await api.checkoutBranch({ name, create, root });
      showBranchMenu = false;
      creatingBranch = false;
      newBranchName = "";
      notice.show(`Switched to branch ${name}.`);
      await refresh();
    } catch (error) {
      const parsed = parseGitError(error);
      notice.error(gitActionableError(parsed.code, parsed.message));
    }
  }

  async function handleSync(): Promise<void> {
    const root = projectRoot;
    if (root === "" || syncing) return;

    syncing = true;
    try {
      const res = await api.sync({ action: "pull", root });
      notice.show(res.output || "Sync completed successfully.");
      await refresh();
    } catch (error) {
      const parsed = parseGitError(error);
      notice.error(gitActionableError(parsed.code, parsed.message));
    } finally {
      syncing = false;
    }
  }

  async function inspectDiff(file: GitStatusFile, staged: boolean): Promise<void> {
    const root = projectRoot;
    if (root === "") return;
    try {
      const res = await api.diff({ root, path: file.path, staged });
      activeDiff = res.files[0] ?? null;
    } catch (error) {
      const parsed = parseGitError(error);
      notice.error(gitActionableError(parsed.code, parsed.message));
    }
  }

  async function loadMoreCommits(): Promise<void> {
    const root = projectRoot;
    if (root === "") return;
    try {
      const res = await api.log({ root, limit: 15, offset: commits.length });
      commits = [...commits, ...(res.commits as GitCommitInfo[])];
      hasMoreCommits = res.has_more;
    } catch (error) {
      const parsed = parseGitError(error);
      notice.error(gitActionableError(parsed.code, parsed.message));
    }
  }

  function toggleSection(key: string): void {
    if (collapsed.has(key)) {
      collapsed.delete(key);
    } else {
      collapsed.add(key);
    }
  }
</script>

<div class="mng-panel-head">
  <h2>Source control</h2>
  <div class="flex items-center gap-1">
    <button
      type="button"
      class="mng-icon-btn"
      title="Refresh status"
      aria-label="Refresh status"
      disabled={loading}
      onclick={() => void refresh()}
    >
      <Icon name="refresh" size={14} class={loading ? "animate-spin" : ""} />
    </button>
  </div>
</div>

<div
  class="mx-4 mb-2 flex shrink-0 border border-line"
  role="radiogroup"
  aria-label="Source control mode"
>
  {#each [{ id: "changes", label: "Changes" }, { id: "versions", label: "Versions" }] as option, index (option.id)}
    <button
      type="button"
      role="radio"
      aria-checked={scm.mode === option.id}
      class="inset-focus h-6 min-w-0 flex-1 text-xs {index > 0
        ? 'border-l border-line'
        : ''} {scm.mode === option.id
        ? 'bg-surface-hi text-text'
        : 'text-text-60 hover:text-text'}"
      onclick={() => scm.show(option.id === "versions" ? "versions" : "changes")}
    >
      {option.label}
    </button>
  {/each}
</div>

{#if scm.mode === "versions"}
  <VersionsPanel />
{/if}

<div class="min-h-0 flex-1 overflow-y-auto" hidden={scm.mode === "versions"}>
  {#if projectRoot === ""}
    <div class="p-4 text-xs text-text-muted">Open a folder to use source control.</div>
  {:else if !isRepo}
    <!-- Not a repository state -->
    <div class="space-y-3 p-4 text-xs">
      <p class="m-0 text-text-60">The open folder is not a Git repository.</p>
      <button
        type="button"
        class="mng-btn w-full justify-center"
        disabled={loading}
        onclick={() => void handleInit()}
      >
        Initialize repository
      </button>
    </div>
  {:else if activeDiff !== null}
    <!-- Read-only Diff View -->
    <div class="flex flex-col border-b border-line bg-surface-hi/40">
      <div
        class="flex items-center justify-between border-b border-line px-3 py-1.5 text-xs"
      >
        <span class="truncate font-mono text-text">
          {basename(activeDiff.path)}
        </span>
        <button
          type="button"
          class="mng-icon-btn text-text-60 hover:text-text"
          title="Close diff view"
          aria-label="Close diff view"
          onclick={() => (activeDiff = null)}
        >
          <Icon name="close" size={14} />
        </button>
      </div>

      <div class="max-h-96 overflow-auto font-mono text-[11px] leading-relaxed">
        {#each activeDiff.hunks as hunk, hIdx (hIdx)}
          <div
            class="border-y border-line/60 bg-surface px-2 py-0.5 text-[10px] text-text-muted"
          >
            {hunk.header}
          </div>
          {#each hunk.lines as line, lIdx (lIdx)}
            <div
              class="flex items-baseline px-2 py-0.5 {line.kind === 'add'
                ? 'bg-math/20 text-text'
                : line.kind === 'remove'
                  ? 'bg-text/20 text-text'
                  : 'text-text-60'}"
            >
              <span
                class="w-7 shrink-0 select-none text-right font-mono text-[10px] text-text-muted"
              >
                {line.old_line ?? ""}
              </span>
              <span
                class="w-7 shrink-0 select-none text-right font-mono text-[10px] text-text-muted"
              >
                {line.new_line ?? ""}
              </span>
              <span class="w-4 select-none text-center">
                {line.kind === "add" ? "+" : line.kind === "remove" ? "-" : " "}
              </span>
              <span class="whitespace-pre">{line.text}</span>
            </div>
          {/each}
        {/each}
      </div>
    </div>
  {:else}
    <!-- Branch header & Sync -->
    <div class="border-b border-line px-3 py-2 space-y-2">
      <div class="flex items-center justify-between gap-1 text-xs">
        <button
          type="button"
          class="flex items-center gap-1.5 text-left text-text hover:text-text-60"
          onclick={() => (showBranchMenu = !showBranchMenu)}
          aria-expanded={showBranchMenu}
          title="Switch branch"
        >
          <Icon name="branch" size={14} class="shrink-0 text-text-60" />
          <span class="truncate">{status?.branch ?? "HEAD"}</span>
          <Icon
            name={showBranchMenu ? "chevron-down" : "chevron-right"}
            size={12}
            class="text-text-muted"
          />
        </button>

        {#if status?.upstream}
          <div class="flex items-center gap-2 text-[10px] font-mono text-text-muted">
            {#if status.upstream.ahead > 0}
              <span title="{status.upstream.ahead} commits ahead"
                >↑{status.upstream.ahead}</span
              >
            {/if}
            {#if status.upstream.behind > 0}
              <span title="{status.upstream.behind} commits behind"
                >↓{status.upstream.behind}</span
              >
            {/if}
            <button
              type="button"
              class="mng-btn px-2 py-0.5 text-[10px]"
              disabled={syncing}
              onclick={() => void handleSync()}
            >
              {syncing ? "Syncing…" : "Sync"}
            </button>
          </div>
        {/if}
      </div>

      <!-- Branch switch / create popover -->
      {#if showBranchMenu}
        <div class="border border-line bg-surface p-2 text-xs space-y-2">
          <div class="text-[11px] text-text-60">Branches</div>
          <ul class="m-0 max-h-36 list-none overflow-y-auto p-0 space-y-0.5">
            {#each branches as b (b.name)}
              <li>
                <button
                  type="button"
                  class="flex w-full items-center justify-between px-2 py-1 text-left hover:bg-surface-hi {b.current
                    ? 'text-text'
                    : 'text-text-60'}"
                  onclick={() => void handleCheckout(b.name)}
                >
                  <span class="truncate">{b.name}</span>
                  {#if b.current}
                    <span class="text-[10px] text-text-60">current</span>
                  {/if}
                </button>
              </li>
            {/each}
          </ul>

          <div class="border-t border-line pt-2">
            {#if !creatingBranch}
              <button
                type="button"
                class="text-[11px] text-text-60 underline hover:text-text"
                onclick={() => (creatingBranch = true)}
              >
                + Create new branch…
              </button>
            {:else}
              <div class="flex items-center gap-1">
                <input
                  bind:value={newBranchName}
                  type="text"
                  placeholder="Branch name"
                  class="mng-field flex-1 text-xs"
                />
                <button
                  type="button"
                  class="mng-btn"
                  disabled={newBranchName.trim() === ""}
                  onclick={() => void handleCheckout(newBranchName.trim(), true)}
                >
                  Create
                </button>
              </div>
            {/if}
          </div>
        </div>
      {/if}

      <!-- Commit box -->
      <div class="space-y-1.5 pt-1">
        <textarea
          bind:value={commitMessage}
          placeholder="Message (Ctrl+Enter to commit)"
          class="mng-field h-16 w-full resize-none text-xs"
          spellcheck="false"
          onkeydown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              if (canCommit) void handleCommit();
            }
          }}
        ></textarea>

        <div class="flex items-center justify-between">
          <label
            class="flex items-center gap-1.5 text-[11px] text-text-60 cursor-pointer"
          >
            <input type="checkbox" bind:checked={amend} class="accent-accent" />
            <span>Amend</span>
          </label>

          <button
            type="button"
            class="mng-btn"
            disabled={!canCommit}
            title={!canCommit ? commitReason : "Commit staged changes"}
            onclick={() => void handleCommit()}
          >
            {committing ? "Committing…" : "Commit"}
          </button>
        </div>

        {#if commitReason !== "" && (commitMessage !== "" || amend)}
          <p class="m-0 text-[10px] text-text-muted">{commitReason}</p>
        {/if}

        {#if errorMessageText !== ""}
          <p class="m-0 text-[11px] text-text">{errorMessageText}</p>
        {/if}
      </div>
    </div>

    <!-- Groups: Staged, Changes, Untracked, Conflicts -->
    <div class="divide-y divide-line text-xs">
      <!-- Staged Changes -->
      <div>
        <div
          class="flex items-center justify-between px-3 py-1.5 text-text-60 hover:bg-surface-hi"
        >
          <button
            type="button"
            class="flex items-center gap-1.5 text-[11px]"
            onclick={() => toggleSection("staged")}
          >
            <Icon
              name={collapsed.has("staged") ? "chevron-right" : "chevron-down"}
              size={12}
            />
            <span>Staged changes</span>
            <span class="ml-1 font-mono text-[10px] text-text-muted"
              >({stagedCount})</span
            >
          </button>
          {#if stagedCount > 0}
            <button
              type="button"
              class="mng-icon-btn text-text-60 hover:text-text"
              title="Unstage all changes"
              aria-label="Unstage all changes"
              onclick={() => void handleUnstage(["."])}
            >
              −
            </button>
          {/if}
        </div>

        {#if !collapsed.has("staged") && stagedCount > 0}
          <ul class="m-0 list-none p-0">
            {#each groups.staged as f (f.path)}
              {@const badge = statusBadge(f.staged)}
              <li
                class="flex items-center justify-between px-3 py-1 hover:bg-surface-hi"
              >
                <button
                  type="button"
                  class="flex flex-1 items-center gap-2 truncate text-left text-xs"
                  onclick={() => void inspectDiff(f, true)}
                >
                  <span class="font-mono text-[10px] {badge.textClass}"
                    >{badge.label}</span
                  >
                  <span class="truncate text-text">{basename(f.path)}</span>
                  {#if dirname(f.path) !== ""}
                    <span class="truncate text-[10px] text-text-muted"
                      >{dirname(f.path)}</span
                    >
                  {/if}
                </button>
                <button
                  type="button"
                  class="mng-icon-btn ml-1 shrink-0 text-text-60 hover:text-text"
                  title="Unstage change"
                  aria-label="Unstage change"
                  onclick={() => void handleUnstage([f.path])}
                >
                  −
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>

      <!-- Changes (Unstaged) -->
      <div>
        <div
          class="flex items-center justify-between px-3 py-1.5 text-text-60 hover:bg-surface-hi"
        >
          <button
            type="button"
            class="flex items-center gap-1.5 text-[11px]"
            onclick={() => toggleSection("changes")}
          >
            <Icon
              name={collapsed.has("changes") ? "chevron-right" : "chevron-down"}
              size={12}
            />
            <span>Changes</span>
            <span class="ml-1 font-mono text-[10px] text-text-muted"
              >({changesCount})</span
            >
          </button>
          {#if changesCount > 0}
            <div class="flex items-center gap-1">
              <button
                type="button"
                class="mng-icon-btn text-text-60 hover:text-text"
                title="Discard all changes"
                aria-label="Discard all changes"
                onclick={() => (confirmingDiscardPath = "*")}
              >
                <Icon name="undo" size={13} />
              </button>
              <button
                type="button"
                class="mng-icon-btn text-text-60 hover:text-text"
                title="Stage all changes"
                aria-label="Stage all changes"
                onclick={() => void handleStage(["."])}
              >
                +
              </button>
            </div>
          {/if}
        </div>

        {#if confirmingDiscardPath === "*"}
          <div class="border-y border-line bg-surface p-2 text-xs space-y-1.5">
            <p class="m-0 text-text-60">
              Discard all unstaged changes? This cannot be undone.
            </p>
            <div class="flex justify-end gap-1.5">
              <button
                type="button"
                class="mng-btn"
                onclick={() => (confirmingDiscardPath = null)}>Cancel</button
              >
              <button
                type="button"
                class="mng-btn border-text-60 text-text"
                onclick={() => void handleDiscard(groups.changes.map((f) => f.path))}
              >
                Discard all
              </button>
            </div>
          </div>
        {/if}

        {#if !collapsed.has("changes") && changesCount > 0}
          <ul class="m-0 list-none p-0">
            {#each groups.changes as f (f.path)}
              {@const badge = statusBadge(f.unstaged)}
              <li
                class="flex items-center justify-between px-3 py-1 hover:bg-surface-hi"
              >
                <button
                  type="button"
                  class="flex flex-1 items-center gap-2 truncate text-left text-xs"
                  onclick={() => void inspectDiff(f, false)}
                >
                  <span class="font-mono text-[10px] {badge.textClass}"
                    >{badge.label}</span
                  >
                  <span class="truncate text-text">{basename(f.path)}</span>
                  {#if dirname(f.path) !== ""}
                    <span class="truncate text-[10px] text-text-muted"
                      >{dirname(f.path)}</span
                    >
                  {/if}
                </button>
                <div class="flex items-center gap-0.5 ml-1 shrink-0">
                  <button
                    type="button"
                    class="mng-icon-btn text-text-60 hover:text-text"
                    title="Discard changes"
                    aria-label="Discard changes"
                    onclick={() => (confirmingDiscardPath = f.path)}
                  >
                    <Icon name="undo" size={13} />
                  </button>
                  <button
                    type="button"
                    class="mng-icon-btn text-text-60 hover:text-text"
                    title="Stage change"
                    aria-label="Stage change"
                    onclick={() => void handleStage([f.path])}
                  >
                    +
                  </button>
                </div>
              </li>

              {#if confirmingDiscardPath === f.path}
                <div
                  class="border-y border-line bg-surface px-3 py-2 text-xs space-y-1.5"
                >
                  <p class="m-0 text-text-60">Discard changes in {basename(f.path)}?</p>
                  <div class="flex justify-end gap-1.5">
                    <button
                      type="button"
                      class="mng-btn"
                      onclick={() => (confirmingDiscardPath = null)}>Cancel</button
                    >
                    <button
                      type="button"
                      class="mng-btn border-text-60 text-text"
                      onclick={() => void handleDiscard([f.path])}
                    >
                      Discard
                    </button>
                  </div>
                </div>
              {/if}
            {/each}
          </ul>
        {/if}
      </div>

      <!-- Untracked Files -->
      {#if untrackedCount > 0}
        <div>
          <div
            class="flex items-center justify-between px-3 py-1.5 text-text-60 hover:bg-surface-hi"
          >
            <button
              type="button"
              class="flex items-center gap-1.5 text-[11px]"
              onclick={() => toggleSection("untracked")}
            >
              <Icon
                name={collapsed.has("untracked") ? "chevron-right" : "chevron-down"}
                size={12}
              />
              <span>Untracked</span>
              <span class="ml-1 font-mono text-[10px] text-text-muted"
                >({untrackedCount})</span
              >
            </button>
            <button
              type="button"
              class="mng-icon-btn text-text-60 hover:text-text"
              title="Stage all untracked files"
              aria-label="Stage all untracked files"
              onclick={() => void handleStage(groups.untracked.map((f) => f.path))}
            >
              +
            </button>
          </div>

          {#if !collapsed.has("untracked")}
            <ul class="m-0 list-none p-0">
              {#each groups.untracked as f (f.path)}
                <li
                  class="flex items-center justify-between px-3 py-1 hover:bg-surface-hi"
                >
                  <span
                    class="flex flex-1 items-center gap-2 truncate text-left text-xs"
                  >
                    <span class="font-mono text-[10px] text-text-60">U</span>
                    <span class="truncate text-text">{basename(f.path)}</span>
                    {#if dirname(f.path) !== ""}
                      <span class="truncate text-[10px] text-text-muted"
                        >{dirname(f.path)}</span
                      >
                    {/if}
                  </span>
                  <div class="flex items-center gap-0.5 ml-1 shrink-0">
                    <button
                      type="button"
                      class="mng-icon-btn text-text-60 hover:text-text"
                      title="Discard untracked file"
                      aria-label="Discard untracked file"
                      onclick={() => void handleDiscard([f.path])}
                    >
                      <Icon name="trash" size={13} />
                    </button>
                    <button
                      type="button"
                      class="mng-icon-btn text-text-60 hover:text-text"
                      title="Stage file"
                      aria-label="Stage file"
                      onclick={() => void handleStage([f.path])}
                    >
                      +
                    </button>
                  </div>
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}

      <!-- Conflicted Files -->
      {#if conflictedCount > 0}
        <div>
          <div
            class="flex items-center justify-between px-3 py-1.5 text-text hover:bg-surface-hi"
          >
            <span class="text-[11px]">CONFLICTS ({conflictedCount})</span>
          </div>
          <ul class="m-0 list-none p-0">
            {#each groups.conflicted as f (f.path)}
              <li class="flex items-center justify-between px-3 py-1 text-text">
                <span class="truncate">{f.path}</span>
                <span class="font-mono text-[10px]">!</span>
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      <!-- Commit History Section -->
      <div>
        <div
          class="flex items-center justify-between px-3 py-1.5 text-text-60 hover:bg-surface-hi"
        >
          <button
            type="button"
            class="flex items-center gap-1.5 text-[11px]"
            aria-expanded={showHistory}
            aria-label="Toggle history"
            onclick={() => (showHistory = !showHistory)}
          >
            <Icon name={showHistory ? "chevron-down" : "chevron-right"} size={12} />
            <span>History</span>
          </button>
        </div>

        {#if showHistory}
          <ul class="m-0 list-none p-0">
            {#each commits as commit (commit.hash)}
              <li
                class="border-b border-line/40 px-3 py-1.5 text-xs hover:bg-surface-hi"
              >
                <div class="flex items-baseline justify-between gap-2">
                  <span class="truncate text-text">{commit.subject}</span>
                  <span class="shrink-0 font-mono text-[10px] text-text-60"
                    >{commit.short}</span
                  >
                </div>
                <div
                  class="flex items-center justify-between text-[10px] text-text-muted"
                >
                  <span class="truncate">{commit.author_name}</span>
                  <span>{formatRelativeTime(commit.time)}</span>
                </div>
              </li>
            {/each}
          </ul>
          {#if hasMoreCommits}
            <div class="p-2 text-center">
              <button
                type="button"
                class="mng-btn w-full justify-center text-xs"
                onclick={() => void loadMoreCommits()}
              >
                Load more
              </button>
            </div>
          {/if}
        {/if}
      </div>
    </div>
  {/if}
</div>
