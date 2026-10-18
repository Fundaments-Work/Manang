<script lang="ts">
  import { onMount, untrack } from "svelte";

  import VersionDiff from "$lib/components/VersionDiff.svelte";
  import { kernel, reportError } from "$lib/kernel";
  import { gitApi, type GitFileDiff, type GitVersionInfo } from "$lib/kernel/api";
  import { notice } from "$lib/state/notice.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { restoreFromVersion, saveVersion } from "$lib/state/versionActions";
  import { workspace } from "$lib/state/workspace.svelte";
  import { basename, dirname } from "$lib/workbench/paths";
  import {
    formatRelativeTime,
    gitActionableError,
    parseGitError,
    statusBadge,
  } from "$lib/workbench/git";
  import { fileCount, sortedFiles, versionTitle } from "$lib/workbench/versions";

  /**
   * The Versions timeline (a mode of the Source Control view): every saved
   * version with who and when, what each changed, a read-only diff, and
   * Restore for a file or the whole project. Restoring never rewrites history.
   */

  const api = gitApi(kernel);
  const PAGE = 30;

  const projectRoot = $derived(workspace.project?.root ?? "");
  const currentFile = $derived(tabs.active?.path ?? null);

  let scope = $state<"file" | "all">("all");
  const filterPath = $derived(scope === "file" ? currentFile : null);

  let versions = $state.raw<GitVersionInfo[]>([]);
  let nextCursor = $state<string | null>(null);
  let loading = $state(false);
  let tracked = $state(true);
  let problem = $state("");

  let selected = $state.raw<GitVersionInfo | null>(null);
  let files = $state.raw<GitFileDiff[]>([]);
  let activeFile = $state<string | null>(null);
  let diff = $state.raw<GitFileDiff | null>(null);
  let restoring = $state(false);

  /** Drops answers that arrive after the person moved on. */
  let generation = 0;

  const activeStatus = $derived(
    files.find((f) => f.path === activeFile)?.status ?? null,
  );

  function fail(where: string, error: unknown): void {
    reportError(where, error);
    const parsed = parseGitError(error);
    problem = gitActionableError(parsed.code, parsed.message);
    notice.error(problem);
  }

  async function load(root: string, path: string | null, more = false): Promise<void> {
    if (root === "") return;
    const mine = ++generation;
    loading = true;
    problem = "";
    try {
      const opened = await api.open(root);
      if (mine !== generation) return;
      tracked = opened.is_repo;
      if (!tracked) {
        versions = [];
        nextCursor = null;
        return;
      }
      const page = await api.versions({
        root,
        limit: PAGE,
        ...(path === null ? {} : { path }),
        ...(more && nextCursor !== null ? { cursor: nextCursor } : {}),
      });
      if (mine !== generation) return;
      versions = more ? [...versions, ...page.versions] : [...page.versions];
      nextCursor = page.next_cursor;
      if (selected !== null && !versions.some((v) => v.hash === selected?.hash)) {
        selected = null;
        files = [];
        activeFile = null;
        diff = null;
      }
    } catch (error) {
      if (mine !== generation) return;
      const parsed = parseGitError(error);
      if (parsed.code === "not-a-repo") {
        tracked = false;
        versions = [];
      } else fail("loading versions", error);
    } finally {
      if (mine === generation) loading = false;
    }
  }

  $effect(() => {
    const root = projectRoot;
    const path = filterPath;
    untrack(() => void load(root, path));
  });

  onMount(() => {
    const unsubscribe = kernel.onEvent((event) => {
      if (event.topic === "git.changed") void load(projectRoot, filterPath);
    });
    return () => {
      void unsubscribe.then((off) => off());
    };
  });

  async function openFile(version: GitVersionInfo, path: string): Promise<void> {
    activeFile = path;
    diff = null;
    try {
      const shown = await api.versionDiff({
        root: projectRoot,
        rev: version.hash,
        path,
      });
      if (selected?.hash === version.hash && activeFile === path) diff = shown;
    } catch (error) {
      fail("loading the changes of a version", error);
    }
  }

  async function select(version: GitVersionInfo): Promise<void> {
    if (selected?.hash === version.hash) {
      selected = null;
      files = [];
      activeFile = null;
      diff = null;
      return;
    }
    selected = version;
    files = [];
    activeFile = null;
    diff = null;
    try {
      const res = await api.versionFiles({ root: projectRoot, rev: version.hash });
      if (selected?.hash !== version.hash) return;
      files = sortedFiles(res.files);
      const preferred = files.find((f) => f.path === currentFile) ?? files[0];
      if (preferred !== undefined) await openFile(version, preferred.path);
    } catch (error) {
      fail("loading the files of a version", error);
    }
  }

  async function restoreFile(): Promise<void> {
    if (selected === null || activeFile === null || restoring) return;
    restoring = true;
    try {
      await restoreFromVersion(selected, { kind: "file", paths: [activeFile] });
    } finally {
      restoring = false;
    }
  }

  async function restoreWhole(): Promise<void> {
    if (selected === null || restoring) return;
    restoring = true;
    try {
      await restoreFromVersion(selected, {
        kind: "whole",
        paths: files.map((f) => f.path),
      });
    } finally {
      restoring = false;
    }
  }
</script>

<div class="flex min-h-0 flex-1 flex-col" aria-label="Versions">
  {#if projectRoot === ""}
    <div class="p-4 text-xs text-text-muted">Open a folder to see its versions.</div>
  {:else if !tracked}
    <div class="space-y-3 p-4 text-xs">
      <p class="m-0 text-text-60">
        This project is not keeping versions yet. Save one to start.
      </p>
      <button type="button" class="mng-btn" onclick={() => void saveVersion()}>
        Save a version
      </button>
    </div>
  {:else}
    <div class="flex items-center justify-between gap-2 border-b border-line px-3 py-2">
      <div
        class="flex border border-line"
        role="radiogroup"
        aria-label="Show versions of"
      >
        {#each [{ id: "file", label: "This file" }, { id: "all", label: "All files" }] as option, index (option.id)}
          <button
            type="button"
            role="radio"
            aria-checked={scope === option.id}
            disabled={option.id === "file" && currentFile === null}
            class="inset-focus h-6 px-2 text-xs disabled:text-text-muted {index > 0
              ? 'border-l border-line'
              : ''} {scope === option.id
              ? 'bg-surface-hi text-text'
              : 'text-text-60 hover:text-text'}"
            onclick={() => (scope = option.id === "file" ? "file" : "all")}
          >
            {option.label}
          </button>
        {/each}
      </div>
      <button
        type="button"
        class="mng-btn h-6 px-2 text-xs"
        onclick={() => void saveVersion()}
      >
        Save a version
      </button>
    </div>

    {#if problem !== ""}
      <p class="m-0 px-3 py-2 text-xs text-text">{problem}</p>
    {/if}

    <div class="min-h-0 flex-1 overflow-y-auto">
      {#if versions.length === 0 && !loading}
        <p class="m-0 p-4 text-xs text-text-muted">
          {scope === "file"
            ? "No saved versions have changed this file."
            : "No versions yet. Save a version to keep a copy you can return to."}
        </p>
      {/if}
      <ul class="m-0 list-none p-0" aria-label="Versions">
        {#each versions as version (version.hash)}
          {@const open = selected?.hash === version.hash}
          <li class="border-b border-line/40">
            <button
              type="button"
              class="inset-focus block w-full px-3 py-1.5 text-left hover:bg-surface-hi {open
                ? 'bg-surface-hi'
                : ''}"
              aria-expanded={open}
              onclick={() => void select(version)}
            >
              <span class="flex items-baseline justify-between gap-2 text-xs">
                <span class="truncate text-text">{versionTitle(version)}</span>
                <span class="shrink-0 font-mono text-[10px] text-text-60"
                  >{version.short}</span
                >
              </span>
              <span
                class="flex items-center justify-between gap-2 text-[10px] text-text-muted"
              >
                <span class="truncate">
                  {version.author_name} · {formatRelativeTime(version.time)} · {fileCount(
                    version.files_changed,
                  )}
                </span>
                {#if version.labelled}
                  <span class="shrink-0 border border-line px-1 text-text-60"
                    >Saved version</span
                  >
                {/if}
              </span>
            </button>

            {#if open}
              <div class="border-t border-line bg-surface">
                <ul class="m-0 list-none p-0" aria-label="Files in this version">
                  {#each files as file (file.path)}
                    {@const badge = statusBadge(file.status)}
                    <li>
                      <button
                        type="button"
                        class="inset-focus flex w-full items-center gap-2 px-3 py-1 text-left text-xs hover:bg-surface-hi {activeFile ===
                        file.path
                          ? 'bg-surface-hi'
                          : ''}"
                        aria-current={activeFile === file.path}
                        onclick={() => void openFile(version, file.path)}
                      >
                        <span class="font-mono text-[10px] text-text-60"
                          >{badge.label}</span
                        >
                        <span class="truncate text-text">{basename(file.path)}</span>
                        {#if dirname(file.path) !== ""}
                          <span class="truncate text-[10px] text-text-muted"
                            >{dirname(file.path)}</span
                          >
                        {/if}
                        <span
                          class="ml-auto shrink-0 font-mono text-[10px] text-text-muted"
                        >
                          +{file.additions} −{file.deletions}
                        </span>
                      </button>
                    </li>
                  {/each}
                </ul>
                {#if diff !== null}
                  <VersionDiff {diff} />
                {/if}
                <div class="flex flex-wrap justify-end gap-2 px-3 py-2">
                  <button
                    type="button"
                    class="mng-btn text-xs"
                    disabled={restoring ||
                      activeFile === null ||
                      activeStatus === "deleted"}
                    title={activeStatus === "deleted"
                      ? "This version removed the file"
                      : "Replace this file with its content in this version"}
                    onclick={() => void restoreFile()}
                  >
                    Restore this file
                  </button>
                  <button
                    type="button"
                    class="mng-btn text-xs"
                    disabled={restoring}
                    onclick={() => void restoreWhole()}
                  >
                    Restore whole project
                  </button>
                </div>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
      {#if nextCursor !== null}
        <div class="p-2 text-center">
          <button
            type="button"
            class="mng-btn w-full justify-center text-xs"
            disabled={loading}
            onclick={() => void load(projectRoot, filterPath, true)}
          >
            Load more
          </button>
        </div>
      {/if}
    </div>
  {/if}
</div>
