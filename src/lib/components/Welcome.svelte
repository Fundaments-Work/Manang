<script lang="ts">
  import { untrack } from "svelte";

  import Mark from "./Mark.svelte";
  import { createProject, openProject } from "$lib/state/actions";
  import { workspace } from "$lib/state/workspace.svelte";
  import type { Engine } from "$lib/kernel/types";

  /**
   * Welcome: shown when no project is open. Open a folder, start a new project
   * in Typst, LaTeX or Markdown, or pick up a recent one. There is no native
   * folder picker yet (it needs the dialog plugin), so a folder is opened by its
   * path; the recent list is the quick way back.
   */

  type Mode = "choose" | "open" | "create";

  let mode = $state<Mode>("choose");
  let folder = $state("");
  let name = $state("");
  let location = $state("");
  let engine = $state<Engine>("typst");
  let nameInput: HTMLInputElement | undefined = $state();
  let folderInput: HTMLInputElement | undefined = $state();

  const engines: readonly { id: Engine; label: string; note: string }[] = [
    { id: "typst", label: "Typst", note: "Fast, modern markup with instant preview." },
    {
      id: "latex",
      label: "LaTeX",
      note: "For journal templates and existing .tex files.",
    },
    { id: "markdown", label: "Markdown", note: "Plain notes and simple documents." },
  ];

  // Suggest the folder the newest project lives in, once the recent list arrives.
  $effect(() => {
    const newest = workspace.recent.find((r) => r.exists);
    if (newest !== undefined && untrack(() => location) === "") {
      location = newest.path.slice(0, newest.path.lastIndexOf("/")) || "/";
    }
  });

  const slug = $derived(
    name
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, ""),
  );
  const target = $derived(`${location.replace(/\/+$/, "")}/${slug}`);
  const canCreate = $derived(
    slug !== "" && location.trim() !== "" && !workspace.opening,
  );

  function start(next: Mode): void {
    workspace.error = "";
    mode = next;
    setTimeout(() => (next === "open" ? folderInput : nameInput)?.focus(), 0);
  }

  async function submitOpen(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (folder.trim() === "") return;
    await openProject(folder.trim());
  }

  async function submitCreate(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!canCreate) return;
    await createProject(target, name.trim(), engine);
  }

  function when(seconds: number): string {
    return new Date(seconds * 1000).toLocaleDateString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
    });
  }

  /** Say what to do, not just what happened. */
  const errorHint = $derived(
    workspace.error.includes("not a Manang project")
      ? `${workspace.error}. Choose the folder that contains manang.toml, or create a new project.`
      : workspace.error,
  );
</script>

<div class="h-full overflow-y-auto">
  <div
    class="mx-auto w-full max-w-[620px] px-8 pt-[12vh] pb-16 max-[800px]:px-5 max-[800px]:pt-10"
  >
    <div class="flex items-center gap-3 text-text">
      <Mark size={28} />
      <h1
        class="m-0 text-[28px] font-light"
        style="letter-spacing: var(--tracking-display)"
      >
        Manang
      </h1>
    </div>
    <p class="mt-3 mb-8 max-w-[46ch] text-[13px] leading-relaxed text-text-60">
      Write papers, notes and slide decks in Typst, LaTeX or Markdown. Everything stays
      in plain files on your disk.
    </p>

    {#if mode === "choose"}
      <div class="flex flex-wrap gap-2">
        <button
          type="button"
          class="mng-btn h-9 px-4 text-[13px]"
          onclick={() => start("open")}>Open folder</button
        >
        <button
          type="button"
          class="mng-btn h-9 px-4 text-[13px]"
          onclick={() => start("create")}>New project</button
        >
      </div>
    {:else if mode === "open"}
      <form class="flex flex-col gap-3" onsubmit={submitOpen} aria-label="Open folder">
        <label class="flex flex-col gap-1.5 text-xs text-text-60">
          Folder path
          <input
            bind:this={folderInput}
            bind:value={folder}
            class="mng-field selectable h-9 font-mono text-[12px]"
            placeholder="/home/you/papers/my-paper"
            spellcheck="false"
            autocomplete="off"
          />
        </label>
        <div class="flex gap-2">
          <button
            type="submit"
            class="mng-btn h-9 px-4 text-[13px]"
            disabled={folder.trim() === "" || workspace.opening}>Open</button
          >
          <button
            type="button"
            class="mng-btn h-9 border-transparent bg-transparent px-3 text-[13px] text-text-60"
            onclick={() => (mode = "choose")}>Back</button
          >
        </div>
      </form>
    {:else}
      <form
        class="flex flex-col gap-4"
        onsubmit={submitCreate}
        aria-label="New project"
      >
        <label class="flex flex-col gap-1.5 text-xs text-text-60">
          Project name
          <input
            bind:this={nameInput}
            bind:value={name}
            class="mng-field h-9 text-[13px]"
            placeholder="Entropy paper"
            autocomplete="off"
          />
        </label>
        <fieldset class="m-0 border-0 p-0">
          <legend class="mb-1.5 p-0 text-xs text-text-60">Language</legend>
          <div
            role="radiogroup"
            aria-label="Language"
            class="grid grid-cols-3 gap-px max-[800px]:grid-cols-1"
          >
            {#each engines as option (option.id)}
              <button
                type="button"
                role="radio"
                aria-checked={engine === option.id}
                class="mng-btn h-auto flex-col items-start gap-1 px-3 py-2.5 text-left whitespace-normal"
                onclick={() => (engine = option.id)}
              >
                <span class="text-[13px]">{option.label}</span>
                <span class="text-[11px] leading-snug text-text-60">{option.note}</span>
              </button>
            {/each}
          </div>
        </fieldset>
        <label class="flex flex-col gap-1.5 text-xs text-text-60">
          Create in
          <input
            bind:value={location}
            class="mng-field selectable h-9 font-mono text-[12px]"
            placeholder="/home/you/papers"
            spellcheck="false"
            autocomplete="off"
          />
        </label>
        {#if slug !== ""}
          <p class="m-0 text-[11px] text-text-muted">
            The project will be created at <span class="selectable font-mono"
              >{target}</span
            >
          </p>
        {/if}
        <div class="flex gap-2">
          <button
            type="submit"
            class="mng-btn h-9 px-4 text-[13px]"
            disabled={!canCreate}>Create project</button
          >
          <button
            type="button"
            class="mng-btn h-9 border-transparent bg-transparent px-3 text-[13px] text-text-60"
            onclick={() => (mode = "choose")}>Back</button
          >
        </div>
      </form>
    {/if}

    {#if workspace.error !== ""}
      <p
        role="alert"
        class="mt-4 max-w-[60ch] border-l border-text-60 pl-3 text-xs leading-relaxed text-text"
      >
        {errorHint}
      </p>
    {/if}

    {#if workspace.recent.length > 0}
      <section class="mt-12" aria-labelledby="recent-heading">
        <h2 id="recent-heading" class="m-0 mb-2 text-xs font-normal text-text-60">
          Recent projects
        </h2>
        <ul class="m-0 list-none border-t border-line p-0">
          {#each workspace.recent as project (project.path)}
            <li class="border-b border-line">
              <button
                type="button"
                class="inset-focus flex w-full items-baseline gap-4 py-2.5 text-left hover:bg-surface disabled:cursor-default disabled:hover:bg-transparent"
                disabled={!project.exists || workspace.opening}
                onclick={() => void openProject(project.path)}
              >
                <span
                  class="shrink-0 text-[13px] {project.exists
                    ? 'text-text'
                    : 'text-text-muted'}">{project.name}</span
                >
                <span
                  class="min-w-0 flex-1 truncate font-mono text-[11px] text-text-muted"
                  >{project.path}</span
                >
                <span class="shrink-0 text-[11px] text-text-muted"
                  >{project.exists ? when(project.opened_at) : "Folder not found"}</span
                >
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>
</div>
