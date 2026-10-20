<script lang="ts">
  import { tick } from "svelte";

  import PresenceSwatches from "$lib/components/PresenceSwatches.svelte";
  import ContextMenu, { type MenuItem } from "$lib/components/ui/ContextMenu.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import {
    createFile,
    createFolder,
    isTabDirty,
    openFile,
    renameEntry,
    trashEntry,
  } from "$lib/state/actions";
  import { canBeMain, setMainFile } from "$lib/state/mainFile";
  import { live } from "$lib/state/live.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { basename, dirname, newFileName, uniqueName } from "$lib/workbench/paths";
  import { navigate, typeAhead, type TreeKey, type TreeRow } from "$lib/workbench/tree";

  /**
   * Files: the project tree (WAI-ARIA tree view). Folders load lazily as they
   * are expanded; git state comes from `project.git-status`. Creating and
   * renaming happen inline in the tree, as in other editors.
   */

  type Editing =
    { mode: "rename"; path: string } | { mode: "file" | "folder"; dir: string };

  let editing = $state<Editing | null>(null);
  let draft = $state("");
  let editError = $state("");
  let focused = $state("");
  let menu = $state<{ x: number; y: number; items: MenuItem[]; label: string } | null>(
    null,
  );
  let typed = "";
  let typedTimer: ReturnType<typeof setTimeout> | undefined;
  let treeEl: HTMLElement | undefined = $state();

  const rows = $derived(workspace.rows);
  const filtering = $derived(workspace.filter.trim() !== "");
  const activePath = $derived(tabs.active?.path ?? "");
  const tabStop = $derived(
    rows.some((r) => r.path === focused)
      ? focused
      : (rows.find((r) => r.path === activePath)?.path ?? rows[0]?.path ?? ""),
  );

  $effect(() => {
    if (filtering) void workspace.ensureAll();
  });

  async function focusRow(path: string): Promise<void> {
    focused = path;
    await tick();
    treeEl?.querySelector<HTMLElement>(`[data-path="${CSS.escape(path)}"]`)?.focus();
  }

  /* -- Creating and renaming ---------------------------------------------- */

  async function startCreate(mode: "file" | "folder", dir: string): Promise<void> {
    workspace.filter = "";
    if (dir !== "") await workspace.expand(dir);
    draft =
      mode === "file"
        ? uniqueName(newFileName(workspace.engine), workspace.namesIn(dir))
        : "";
    editError = "";
    editing = { mode, dir };
    await tick();
    selectDraft();
  }

  function startRename(path: string): void {
    draft = basename(path);
    editError = "";
    editing = { mode: "rename", path };
    void tick().then(selectDraft);
  }

  function selectDraft(): void {
    const input = treeEl?.querySelector<HTMLInputElement>("input[data-editing]");
    if (input === null || input === undefined) return;
    input.focus();
    const dot = draft.lastIndexOf(".");
    input.setSelectionRange(0, dot > 0 ? dot : draft.length);
  }

  async function commit(): Promise<void> {
    const current = editing;
    if (current === null) return;
    const error =
      current.mode === "rename"
        ? await renameEntry(current.path, draft)
        : current.mode === "file"
          ? await createFile(current.dir, draft)
          : await createFolder(current.dir, draft);
    if (error !== null) {
      editError = error;
      return;
    }
    editing = null;
    if (current.mode === "rename")
      void focusRow(
        `${dirname(current.path) === "" ? "" : `${dirname(current.path)}/`}${draft.trim()}`,
      );
  }

  function cancelEdit(): void {
    const was = editing;
    editing = null;
    editError = "";
    if (was?.mode === "rename") void focusRow(was.path);
  }

  function onEditKey(event: KeyboardEvent): void {
    if (event.key === "Enter") {
      event.preventDefault();
      void commit();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancelEdit();
    }
  }

  /* -- Tree interaction --------------------------------------------------- */

  // A second, unhurried click on the file that was just clicked renames it, as
  // in file managers. A quick second click (a double click) just opens it.
  let lastClick = { path: "", at: 0 };

  function onRowClick(row: TreeRow): void {
    const now = performance.now();
    const slow =
      row.kind === "file" &&
      lastClick.path === row.path &&
      focused === row.path &&
      now - lastClick.at > 600 &&
      now - lastClick.at < 2000;
    lastClick = { path: row.path, at: now };
    if (slow) startRename(row.path);
    else void activate(row);
  }

  async function activate(row: TreeRow): Promise<void> {
    focused = row.path;
    if (row.kind === "dir") await workspace.toggle(row.path);
    else await openFile(row.path);
  }

  function onTreeKey(event: KeyboardEvent): void {
    if (editing !== null) return;
    const target = (event.target as HTMLElement).closest<HTMLElement>("[data-path]");
    const path = target?.dataset["path"];
    if (path === undefined) return;
    const row = rows.find((r) => r.path === path);
    if (row === undefined) return;

    if (
      ["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight", "Home", "End"].includes(
        event.key,
      )
    ) {
      event.preventDefault();
      const move = navigate(rows, path, event.key as TreeKey);
      if (move.action === "focus") void focusRow(move.path);
      else if (move.action === "expand") void workspace.expand(move.path);
      else if (move.action === "collapse") workspace.collapse(move.path);
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      void activate(row);
    } else if (event.key === "F2") {
      event.preventDefault();
      startRename(path);
    } else if (event.key === "Delete") {
      event.preventDefault();
      void trashEntry(path);
    } else if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) {
      event.preventDefault();
      const box = target?.getBoundingClientRect();
      openMenu(row, box?.left ?? 0, (box?.bottom ?? 0) - 4);
    } else if (
      event.key.length === 1 &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.altKey
    ) {
      typed += event.key;
      clearTimeout(typedTimer);
      typedTimer = setTimeout(() => (typed = ""), 700);
      const hit = typeAhead(rows, path, typed);
      if (hit !== null) void focusRow(hit);
    }
  }

  async function copy(text: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      /* Clipboard access can be denied; the path is also in the breadcrumb. */
    }
  }

  function openMenu(row: TreeRow | null, x: number, y: number): void {
    focused = row?.path ?? focused;
    const dir = row === null ? "" : row.kind === "dir" ? row.path : dirname(row.path);
    const items: MenuItem[] = [];
    if (row !== null && row.kind === "file") {
      items.push({ label: "Open", onselect: () => void openFile(row.path) });
    }
    items.push(
      { label: "New file", onselect: () => void startCreate("file", dir) },
      { label: "New folder", onselect: () => void startCreate("folder", dir) },
    );
    if (
      row !== null &&
      row.kind === "file" &&
      live.active &&
      live.scope === "project"
    ) {
      const shared = live.isLive(row.path);
      items.push({
        label: shared ? "Keep out of the live session" : "Share in the live session",
        separated: true,
        onselect: () => void (shared ? live.exclude(row.path) : live.include(row.path)),
      });
    }
    if (row !== null && row.kind === "file" && canBeMain(row.path)) {
      items.push({
        label: "Set as main file",
        onselect: () => void setMainFile(row.path),
      });
    }
    if (row !== null) {
      items.push(
        {
          label: "Rename",
          shortcut: "F2",
          separated: true,
          onselect: () => startRename(row.path),
        },
        {
          label: "Move to trash",
          shortcut: "Del",
          onselect: () => void trashEntry(row.path),
        },
        { label: "Copy path", separated: true, onselect: () => void copy(row.path) },
      );
    }
    items.push({
      label: workspace.showHidden ? "Hide hidden files" : "Show hidden files",
      separated: true,
      onselect: () => (workspace.showHidden = !workspace.showHidden),
    });
    menu = { x, y, items, label: row === null ? "Files" : basename(row.path) };
  }
</script>

{#snippet editRow(depth: number, icon: "file" | "files")}
  <div
    class="flex h-[26px] items-center gap-1.5 pr-2"
    style="padding-left: {8 + depth * 14 + 18}px"
  >
    <Icon name={icon} size={14} class="shrink-0 text-text-60" />
    <input
      data-editing
      class="mng-field h-[22px] min-w-0 flex-1 px-1.5"
      aria-label={editing?.mode === "rename"
        ? "New name"
        : editing?.mode === "folder"
          ? "Folder name"
          : "File name"}
      bind:value={draft}
      onkeydown={onEditKey}
      onblur={cancelEdit}
      spellcheck="false"
      autocomplete="off"
    />
  </div>
  {#if editError !== ""}
    <p
      role="alert"
      class="m-0 py-1 pr-3 text-[11px] leading-snug text-text"
      style="padding-left: {8 + depth * 14 + 18}px"
    >
      {editError}
    </p>
  {/if}
{/snippet}

<div class="mng-panel-head">
  <h2>Files</h2>
  <button
    type="button"
    class="mng-icon-btn"
    aria-label="New file"
    title="New file"
    onclick={() => void startCreate("file", "")}
  >
    <Icon name="file-plus" />
  </button>
  <button
    type="button"
    class="mng-icon-btn"
    aria-label="New folder"
    title="New folder"
    onclick={() => void startCreate("folder", "")}
  >
    <Icon name="folder-plus" />
  </button>
  <button
    type="button"
    class="mng-icon-btn"
    aria-label="Refresh files"
    title="Refresh files"
    onclick={() => void workspace.refresh()}
  >
    <Icon name="refresh" />
  </button>
  <button
    type="button"
    class="mng-icon-btn"
    aria-label="Collapse all folders"
    title="Collapse all folders"
    onclick={() => workspace.collapseAll()}
  >
    <Icon name="collapse-all" />
  </button>
</div>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  bind:this={treeEl}
  class="min-h-0 flex-1 overflow-y-auto pb-4"
  oncontextmenu={(event) => {
    if ((event.target as HTMLElement).closest("[data-path]") !== null) return;
    event.preventDefault();
    openMenu(null, event.clientX, event.clientY);
  }}
>
  {#if workspace.error !== "" && rows.length === 0 && editing === null}
    <div class="m-3 border border-line bg-surface p-3 text-xs text-text" role="alert">
      <div class="flex items-center gap-1.5 text-text">
        <Icon name="error" size={13} />
        <span>Could not read files</span>
      </div>
      <p class="mt-1 mb-2 leading-relaxed text-text-60">{workspace.error}</p>
      <button
        type="button"
        class="mng-btn text-xs"
        onclick={() => void workspace.refresh()}
      >
        Retry
      </button>
    </div>
  {:else if rows.length === 0 && editing === null}
    <div class="px-4 pt-4 text-xs leading-relaxed text-text-60">
      {#if filtering}
        <p class="m-0">
          No files match "{workspace.filter.trim()}". Clear the filter to see
          everything.
        </p>
        <button
          type="button"
          class="mng-btn mt-2 text-xs"
          onclick={() => {
            workspace.filter = "";
          }}
        >
          Clear filter
        </button>
      {:else}
        <p class="m-0">This folder is empty. Create a file to start writing.</p>
        <button
          type="button"
          class="mng-btn mt-3"
          onclick={() => void startCreate("file", "")}>New file</button
        >
      {/if}
    </div>
  {/if}

  <div role="tree" aria-label="Project files" tabindex="-1" onkeydown={onTreeKey}>
    {#if editing !== null && editing.mode !== "rename" && editing.dir === "" && !filtering}
      {@render editRow(0, editing.mode === "file" ? "file" : "files")}
    {/if}

    {#each rows as row (row.path)}
      {@const selected = row.path === activePath}
      {@const badge = workspace.marks.files.get(row.path)}
      {@const folderChanged =
        row.kind === "dir" && workspace.marks.folders.has(row.path)}
      {#if editing?.mode === "rename" && editing.path === row.path}
        {@render editRow(row.depth, row.kind === "dir" ? "files" : "file")}
      {:else}
        <!-- Keys are handled once, on the tree (onTreeKey). -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          role="treeitem"
          data-path={row.path}
          aria-level={row.depth + 1}
          aria-posinset={row.position}
          aria-setsize={row.setSize}
          aria-expanded={row.kind === "dir" ? row.expanded : undefined}
          aria-selected={selected}
          tabindex={row.path === tabStop ? 0 : -1}
          class="inset-focus group flex h-[26px] cursor-default items-center gap-1 pr-3 text-xs hover:bg-surface-hi/60 {selected
            ? 'bg-surface-hi text-text'
            : 'text-text-60 hover:text-text'}"
          style="padding-left: {8 + row.depth * 14}px"
          title={badge === undefined ? row.path : `${row.path} (${badge.label})`}
          onclick={() => onRowClick(row)}
          oncontextmenu={(event) => {
            event.preventDefault();
            openMenu(row, event.clientX, event.clientY);
          }}
        >
          <span class="flex h-4 w-4 shrink-0 items-center justify-center">
            {#if row.kind === "dir"}
              <Icon name={row.expanded ? "chevron-down" : "chevron-right"} size={12} />
            {/if}
          </span>
          <Icon
            name={row.kind === "dir"
              ? row.expanded
                ? "folder-open"
                : "files"
              : "file"}
            size={14}
            class="shrink-0 opacity-70"
          />
          <span class="min-w-0 flex-1 truncate">
            {row.name}
            {#if row.hint !== ""}<span class="ml-1.5 text-text-muted">{row.hint}</span
              >{/if}
          </span>
          {#if row.kind === "file" && live.active}
            <PresenceSwatches peers={live.peersAt(row.path)} where="row" />
          {/if}
          {#if row.kind === "file" && isTabDirty(row.path)}
            <span
              class="h-1.5 w-1.5 shrink-0 bg-text"
              title="Unsaved changes"
              aria-label="Unsaved changes"
            ></span>
          {/if}
          {#if badge !== undefined}
            <span
              class="w-3 shrink-0 text-right font-mono text-[10px] text-text-60"
              aria-label={badge.label}>{badge.letter}</span
            >
          {:else if folderChanged}
            <span class="h-1 w-1 shrink-0 bg-text-60" aria-label="Contains changes"
            ></span>
          {/if}
          {#if row.loading}
            <span class="text-[11px] text-text-muted" role="status">Loading</span>
          {/if}
        </div>
      {/if}
      {#if editing !== null && editing.mode !== "rename" && editing.dir === row.path && row.expanded}
        {@render editRow(row.depth + 1, editing.mode === "file" ? "file" : "files")}
      {/if}
    {/each}
  </div>
</div>

{#if menu !== null}
  <ContextMenu
    x={menu.x}
    y={menu.y}
    label={menu.label}
    items={menu.items}
    onclose={() => (menu = null)}
  />
{/if}
