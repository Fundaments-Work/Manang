<script lang="ts">
  import { onMount, tick } from "svelte";

  import EditProject from "./home/EditProject.svelte";
  import NewProject from "./home/NewProject.svelte";
  import ProjectCard from "./home/ProjectCard.svelte";
  import ContextMenu, { type MenuItem } from "./ui/ContextMenu.svelte";
  import Icon, { type IconName } from "./ui/Icon.svelte";
  import { closeProject, openFolder, openProject } from "$lib/state/actions";
  import { dialog } from "$lib/state/dialog.svelte";
  import { home } from "$lib/state/home.svelte";
  import { settingsPage } from "$lib/state/settingsPage.svelte";
  import { notice } from "$lib/state/notice.svelte";
  import type { ProjectEntry } from "$lib/kernel/types";
  import { workspace } from "$lib/state/workspace.svelte";
  import {
    ENGINE_LABEL,
    SECTIONS,
    SORTS,
    STARTERS,
    emptyState,
    type HomeSection,
  } from "$lib/workbench/home";

  /**
   * Home: every project in one place. The surface when no project is open, and
   * reachable from inside one (the Home button, Ctrl+Shift+H) without closing
   * it. The list is the kernel's project registry; search, sections and order
   * are applied here (`workbench/home.ts`) so typing never waits on IPC.
   */

  const icons: Record<HomeSection, IconName> = {
    projects: "files",
    recent: "recent",
    pinned: "pin",
    archive: "archive",
    templates: "templates",
    trash: "trash",
  };

  let now = $state(Math.floor(Date.now() / 1000));
  let folderInput: HTMLInputElement | undefined = $state();
  let folder = $state("");
  let menu = $state<{ x: number; y: number; entry: ProjectEntry } | null>(null);
  let editing = $state<{ mode: "name" | "tags"; entry: ProjectEntry } | null>(null);

  const label = $derived(SECTIONS.find((s) => s.id === home.section)?.label ?? "");
  const empty = $derived(emptyState(home.section, home.query, home.entries.length));
  const currentRoot = $derived(workspace.project?.root ?? null);

  onMount(() => {
    void home.load();
    const timer = setInterval(() => (now = Math.floor(Date.now() / 1000)), 60_000);
    return () => clearInterval(timer);
  });

  /** Say what to do, not just what happened. */
  const errorHint = $derived(
    workspace.error.includes("not a Manang project")
      ? `${workspace.error}. Choose the folder that contains manang.toml, or create a new project.`
      : workspace.error,
  );

  // The typed-path form, opened by "Open folder" (top bar or empty state).
  $effect(() => {
    if (home.typingPath) {
      folder = "";
      workspace.error = "";
      void tick().then(() => {
        folderInput?.focus();
        folderInput?.select();
      });
    }
  });

  async function submitFolder(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    const path = folder.trim();
    if (path === "") return;
    if (await openProject(path)) {
      home.typingPath = false;
      folder = "";
    }
  }

  async function open(entry: ProjectEntry): Promise<void> {
    if (!entry.exists || entry.trashed_at) return;
    workspace.error = "";
    if (entry.path === currentRoot) {
      home.hide();
      return;
    }
    await openProject(entry.path);
  }

  async function copyPath(path: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(path);
      notice.show("Copied the folder path.");
    } catch {
      notice.error("Could not copy the path.");
    }
  }

  async function trashProject(entry: ProjectEntry): Promise<void> {
    if (workspace.project?.root === entry.path) {
      const closed = await closeProject();
      if (!closed) return;
      home.show();
    }
    const ok = await home.trash(entry.path);
    if (ok) {
      notice.show(`Moved ${entry.name} to trash.`);
    }
  }

  async function restoreProject(entry: ProjectEntry): Promise<void> {
    const ok = await home.restoreProject(entry.path);
    if (ok) {
      notice.show(`Restored ${entry.name}.`);
    }
  }

  async function promptDelete(entry: ProjectEntry): Promise<void> {
    const choice = await dialog.ask({
      title: `Delete “${entry.name}”?`,
      message:
        "You can remove the project from Manang while keeping its folder, or move the folder to your system trash.",
      actions: [
        { id: "remove", label: "Remove from Manang" },
        { id: "system-trash", label: "Move folder to system trash" },
      ],
    });
    if (choice === "remove") {
      const ok = await home.purge(entry.path, false);
      if (ok) notice.show(`Removed ${entry.name} from Manang.`);
    } else if (choice === "system-trash") {
      const ok = await home.purge(entry.path, true);
      if (ok) notice.show(`Moved ${entry.name} to system trash.`);
    }
  }

  async function promptEmptyTrash(): Promise<void> {
    const choice = await dialog.ask({
      title: "Empty trash?",
      message:
        "Projects in trash will be removed from Manang. Their folders on your computer will stay where they are.",
      actions: [{ id: "empty", label: "Empty trash" }],
    });
    if (choice === "empty") {
      const ok = await home.emptyTrash();
      if (ok) notice.show("Emptied trash.");
    }
  }

  function menuItems(entry: ProjectEntry): MenuItem[] {
    if (entry.trashed_at) {
      return [
        { label: "Restore", onselect: () => void restoreProject(entry) },
        { label: "Delete…", onselect: () => void promptDelete(entry) },
        {
          label: "Copy path",
          separated: true,
          onselect: () => void copyPath(entry.path),
        },
      ];
    }
    return [
      { label: "Open", disabled: !entry.exists, onselect: () => void open(entry) },
      { label: "Rename", onselect: () => (editing = { mode: "name", entry }) },
      { label: "Edit tags", onselect: () => (editing = { mode: "tags", entry }) },
      {
        label: entry.pinned ? "Unpin" : "Pin",
        onselect: () => void home.patch(entry.path, { pinned: !entry.pinned }),
      },
      {
        label: entry.archived ? "Unarchive" : "Archive",
        onselect: () => void home.patch(entry.path, { archived: !entry.archived }),
      },
      {
        label: "Copy path",
        separated: true,
        onselect: () => void copyPath(entry.path),
      },
      {
        label: "Move to trash",
        onselect: () => void trashProject(entry),
      },
    ];
  }
</script>

<section class="flex h-full min-w-0" aria-label="Home">
  <nav
    class="flex w-[200px] shrink-0 flex-col border-r border-line bg-surface max-[800px]:w-12"
    aria-label="Home sections"
  >
    <ul class="m-0 flex list-none flex-col p-0 pt-3">
      {#each SECTIONS.filter((s) => s.id !== "trash") as section (section.id)}
        {@const on = home.section === section.id}
        {@const count = home.counts[section.id]}
        <li>
          <button
            type="button"
            class="inset-focus relative flex h-8 w-full items-center gap-3 px-3 text-left text-xs max-[800px]:justify-center max-[800px]:px-0 {on
              ? 'bg-surface-hi text-text'
              : 'text-text-60 hover:bg-surface-hi hover:text-text'}"
            aria-current={on ? "page" : undefined}
            aria-label={section.label}
            title={section.label}
            onclick={() => home.setSection(section.id)}
          >
            <Icon name={icons[section.id]} size={14} />
            <span class="flex-1 max-[800px]:hidden">{section.label}</span>
            {#if count !== undefined && count > 0}
              <span class="font-mono text-[11px] text-text-muted max-[800px]:hidden"
                >{count}</span
              >
            {/if}
          </button>
        </li>
      {/each}
    </ul>
    <div class="mt-auto flex flex-col border-t border-line">
      <button
        type="button"
        class="inset-focus relative flex h-8 w-full items-center gap-3 px-3 text-left text-xs max-[800px]:justify-center max-[800px]:px-0 {home.section ===
        'trash'
          ? 'bg-surface-hi text-text'
          : 'text-text-60 hover:bg-surface-hi hover:text-text'}"
        aria-current={home.section === "trash" ? "page" : undefined}
        aria-label="Trash"
        title="Trash"
        onclick={() => home.setSection("trash")}
      >
        <Icon name="trash" size={14} />
        <span class="flex-1 max-[800px]:hidden">Trash</span>
        {#if home.counts.trash !== undefined && home.counts.trash > 0}
          <span class="font-mono text-[11px] text-text-muted max-[800px]:hidden"
            >{home.counts.trash}</span
          >
        {/if}
      </button>
      {#if workspace.project !== null}
        <button
          type="button"
          class="inset-focus flex h-8 items-center gap-3 px-3 text-left text-xs text-text-60 hover:bg-surface-hi hover:text-text max-[800px]:justify-center max-[800px]:px-0"
          title="Back to {workspace.project.name}"
          aria-label="Back to {workspace.project.name}"
          onclick={() => home.hide()}
        >
          <Icon name="editor" size={14} />
          <span class="min-w-0 flex-1 truncate max-[800px]:hidden"
            >Back to {workspace.project.name}</span
          >
        </button>
      {/if}
      <button
        type="button"
        class="inset-focus flex h-8 items-center gap-3 px-3 text-left text-xs text-text-60 hover:bg-surface-hi hover:text-text max-[800px]:justify-center max-[800px]:px-0"
        aria-label="Settings"
        title="Settings"
        onclick={() => settingsPage.show()}
      >
        <Icon name="settings" size={14} />
        <span class="max-[800px]:hidden">Settings</span>
      </button>
    </div>
  </nav>

  <div class="flex min-w-0 flex-1 flex-col">
    <div class="min-h-0 flex-1 overflow-y-auto px-6 pt-6 pb-10 max-[800px]:px-4">
      {#if home.typingPath}
        <form
          class="mb-5 flex max-w-[640px] items-end gap-2"
          onsubmit={submitFolder}
          aria-label="Open folder"
        >
          <label class="flex min-w-0 flex-1 flex-col gap-1.5 text-xs text-text-60">
            Folder path
            <input
              bind:this={folderInput}
              bind:value={folder}
              class="mng-field selectable font-mono"
              placeholder="/home/you/papers/my-paper"
              spellcheck="false"
              autocomplete="off"
            />
          </label>
          <button
            type="submit"
            class="mng-btn"
            disabled={folder.trim() === "" || workspace.opening}>Open</button
          >
          <button
            type="button"
            class="mng-btn"
            onclick={() => {
              home.typingPath = false;
              folder = "";
              workspace.error = "";
            }}>Cancel</button
          >
        </form>
      {/if}

      {#if workspace.error !== "" && home.creating === null}
        <p
          role="alert"
          class="mb-5 max-w-[60ch] border-l border-text-60 pl-3 text-xs leading-relaxed text-text"
        >
          {errorHint}
        </p>
      {/if}

      <div class="mb-4 flex flex-wrap items-center gap-x-3 gap-y-2">
        <h1
          class="m-0 text-[15px] font-light text-text"
          style="letter-spacing: var(--tracking-headline)"
        >
          {label}
        </h1>
        {#if home.section !== "templates"}
          <span class="text-xs text-text-60" aria-live="polite">
            {home.shown.length}{home.query.trim() !== ""
              ? ` matching “${home.query.trim()}”`
              : ""}
          </span>
          <div class="ml-auto flex items-center gap-2">
            {#if home.section === "trash" && home.shown.length > 0}
              <button
                type="button"
                class="mng-btn text-xs"
                onclick={() => void promptEmptyTrash()}
              >
                Empty trash
              </button>
            {/if}
            <label class="flex items-center max-[800px]:hidden">
              <select
                class="mng-field w-auto"
                aria-label="Sort projects"
                value={home.sort}
                onchange={(event) =>
                  home.setSort(
                    event.currentTarget.value as "modified" | "name" | "created",
                  )}
              >
                {#each SORTS as sort (sort.id)}
                  <option value={sort.id}>{sort.label}</option>
                {/each}
              </select>
            </label>
            <div role="radiogroup" aria-label="View" class="flex gap-px">
              <button
                type="button"
                role="radio"
                aria-checked={home.view === "grid"}
                aria-label="Grid"
                title="Grid"
                class="mng-btn px-2"
                onclick={() => home.setView("grid")}
                ><Icon name="grid" size={14} /></button
              >
              <button
                type="button"
                role="radio"
                aria-checked={home.view === "list"}
                aria-label="List"
                title="List"
                class="mng-btn px-2"
                onclick={() => home.setView("list")}
                ><Icon name="list" size={14} /></button
              >
            </div>
          </div>
        {/if}
      </div>

      {#if home.section === "templates"}
        <p class="m-0 mb-4 max-w-[60ch] text-xs leading-relaxed text-text-60">
          Starters set up a project with a main document and a references file. Pick
          one, name it, and start writing.
        </p>
        <ul
          class="m-0 grid list-none gap-px p-0"
          style="grid-template-columns: repeat(auto-fill, minmax(220px, 1fr))"
        >
          {#each STARTERS as starter (starter.engine)}
            <li class="flex">
              <button
                type="button"
                class="inset-focus flex w-full flex-col gap-2 border border-line bg-surface p-4 text-left hover:bg-surface-hi"
                onclick={() => home.openNew(starter.engine)}
              >
                <span class="font-mono text-[11px] text-text-muted"
                  >{ENGINE_LABEL[starter.engine]}</span
                >
                <span class="text-[13px] text-text">{starter.title}</span>
                <span class="text-[11px] leading-snug text-text-60">{starter.note}</span
                >
              </button>
            </li>
          {/each}
        </ul>
      {:else if home.shown.length === 0}
        {#if !home.loaded}
          <p class="m-0 text-xs text-text-60" role="status">Loading projects</p>
        {:else}
          <div class="flex max-w-[46ch] flex-col items-start gap-3" role="status">
            <p class="m-0 text-[13px] text-text">{empty.title}</p>
            <p class="m-0 text-xs leading-relaxed text-text-60">{empty.hint}</p>
            {#if empty.action === "new"}
              <button type="button" class="mng-btn" onclick={() => home.openNew()}
                >New project</button
              >
            {:else if empty.action === "open"}
              <button type="button" class="mng-btn" onclick={() => void openFolder()}
                >Open folder</button
              >
            {:else if empty.action === "clear"}
              <button type="button" class="mng-btn" onclick={() => (home.query = "")}
                >Clear search</button
              >
            {/if}
          </div>
        {/if}
      {:else if home.view === "grid"}
        <div
          class="grid gap-3"
          style="grid-template-columns: repeat(auto-fill, minmax(208px, 1fr))"
          role="list"
          aria-label={label}
        >
          {#each home.shown as entry (entry.path)}
            <div role="listitem" class="contents">
              <ProjectCard
                {entry}
                view="grid"
                current={entry.path === currentRoot}
                {now}
                onopen={() => void open(entry)}
                onmenu={(x, y) => (menu = { x, y, entry })}
                onpin={() => void home.patch(entry.path, { pinned: !entry.pinned })}
                onrestore={() => void restoreProject(entry)}
                ondelete={() => void promptDelete(entry)}
              />
            </div>
          {/each}
        </div>
      {:else}
        <div class="border-t border-line" role="list" aria-label={label}>
          {#each home.shown as entry (entry.path)}
            <div role="listitem" class="contents">
              <ProjectCard
                {entry}
                view="list"
                current={entry.path === currentRoot}
                {now}
                onopen={() => void open(entry)}
                onmenu={(x, y) => (menu = { x, y, entry })}
                onpin={() => void home.patch(entry.path, { pinned: !entry.pinned })}
                onrestore={() => void restoreProject(entry)}
                ondelete={() => void promptDelete(entry)}
              />
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  {#if menu !== null}
    <ContextMenu
      x={menu.x}
      y={menu.y}
      label="Actions for {menu.entry.name}"
      items={menuItems(menu.entry)}
      onclose={() => (menu = null)}
    />
  {/if}
  {#if editing !== null}
    <EditProject
      mode={editing.mode}
      entry={editing.entry}
      onclose={() => (editing = null)}
    />
  {/if}
  {#if home.creating !== null}
    <NewProject engine={home.creating.engine} onclose={() => (home.creating = null)} />
  {/if}
</section>
