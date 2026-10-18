<script lang="ts">
  import { onMount } from "svelte";

  import { hasNativeDialogs, pickFolder } from "$lib/kernel/native";
  import type { Engine } from "$lib/kernel/types";
  import { createProject } from "$lib/state/actions";
  import { home } from "$lib/state/home.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { slugify, suggestLocation } from "$lib/workbench/home";

  /**
   * New project: a name, a language and where to put it. On the desktop the
   * location comes from the native folder picker; in a browser it is typed.
   */
  interface Props {
    engine: Engine;
    template?: string | null | undefined;
    onclose: () => void;
  }

  const {
    engine: initial,
    template: initialTemplate = null,
    onclose,
  }: Props = $props();

  const engines: readonly { id: Engine; label: string; note: string }[] = [
    { id: "typst", label: "Typst", note: "Fast, modern markup with instant preview." },
    {
      id: "latex",
      label: "LaTeX",
      note: "For journal templates and existing .tex files.",
    },
    { id: "markdown", label: "Markdown", note: "Plain notes and simple documents." },
  ];

  let name = $state("");
  // The dialog opens on the language that was asked for; it is not meant to follow.
  // svelte-ignore state_referenced_locally
  let engine = $state<Engine>(initial);
  // svelte-ignore state_referenced_locally
  let selectedTemplate = $state<string | null>(initialTemplate);
  let location = $state(
    suggestLocation(home.entries) ||
      (workspace.recent.find((r) => r.exists)?.path.replace(/\/[^/]*$/, "") ?? ""),
  );
  let nameInput: HTMLInputElement | undefined = $state();
  let box: HTMLDivElement | undefined = $state();
  const returnTo =
    document.activeElement instanceof HTMLElement ? document.activeElement : null;

  const templatesForEngine = $derived(
    home.templates.filter((t) => t.engine === engine),
  );
  const activeTemplate = $derived(
    home.templates.find((t) => t.id === selectedTemplate) ?? null,
  );

  const slug = $derived(slugify(name));
  const target = $derived(`${location.replace(/\/+$/, "")}/${slug}`);
  const canCreate = $derived(
    slug !== "" && location.trim() !== "" && !workspace.opening,
  );

  onMount(() => {
    workspace.error = "";
    nameInput?.focus();
  });

  function close(): void {
    onclose();
    returnTo?.focus();
  }

  function selectEngine(id: Engine): void {
    engine = id;
    if (selectedTemplate !== null) {
      const tmpl = home.templates.find((t) => t.id === selectedTemplate);
      if (tmpl && tmpl.engine !== id) {
        selectedTemplate = null;
      }
    }
  }

  function onTemplateChange(id: string | null): void {
    selectedTemplate = id;
    if (id !== null) {
      const tmpl = home.templates.find((t) => t.id === id);
      if (tmpl && tmpl.engine !== engine) {
        engine = tmpl.engine;
      }
    }
  }

  async function browse(): Promise<void> {
    try {
      const picked = await pickFolder("Choose where to create the project");
      if (picked !== null) location = picked;
    } catch {
      /* The field still accepts a typed path. */
    }
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!canCreate) return;
    if (await createProject(target, name.trim(), engine, selectedTemplate ?? undefined))
      onclose();
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
      return;
    }
    if (event.key !== "Tab" || box === undefined) return;
    const items = [
      ...box.querySelectorAll<HTMLElement>("input, button:not(:disabled)"),
    ].filter((el) => el.tabIndex >= 0);
    const first = items[0];
    const last = items[items.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }
</script>

<div
  class="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto pt-[10vh]"
  {onkeydown}
  role="presentation"
>
  <button
    type="button"
    tabindex="-1"
    class="absolute inset-0 cursor-default"
    aria-label="Cancel"
    onclick={close}
  ></button>
  <div
    bind:this={box}
    role="dialog"
    aria-modal="true"
    aria-labelledby="new-project-title"
    class="relative w-[520px] max-w-[94vw] border border-line bg-surface p-5"
  >
    <h2
      id="new-project-title"
      class="m-0 text-[15px] font-light text-text"
      style="letter-spacing: var(--tracking-headline)"
    >
      New project
    </h2>
    <form class="mt-4 flex flex-col gap-4" onsubmit={submit} aria-label="New project">
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
          class="grid grid-cols-3 gap-px max-[640px]:grid-cols-1"
        >
          {#each engines as option (option.id)}
            <button
              type="button"
              role="radio"
              aria-checked={engine === option.id}
              class="mng-btn h-auto flex-col items-start gap-1 px-3 py-2.5 text-left whitespace-normal"
              onclick={() => selectEngine(option.id)}
            >
              <span class="text-[13px]">{option.label}</span>
              <span class="text-[11px] leading-snug text-text-60">{option.note}</span>
            </button>
          {/each}
        </div>
      </fieldset>
      <div class="flex flex-col gap-1.5 text-xs text-text-60">
        <div class="flex items-center justify-between">
          <label for="new-project-template">Template</label>
          {#if selectedTemplate !== null}
            <button
              type="button"
              class="border-0 bg-transparent p-0 text-[11px] text-text-60 underline hover:text-text cursor-pointer"
              onclick={() => (selectedTemplate = null)}
            >
              Clear template
            </button>
          {/if}
        </div>
        <select
          id="new-project-template"
          aria-label="Template"
          class="mng-field h-9 text-[12px]"
          value={selectedTemplate ?? ""}
          onchange={(e) => onTemplateChange(e.currentTarget.value || null)}
        >
          <option value="">Blank (default starter)</option>
          {#each templatesForEngine as tmpl (tmpl.id)}
            <option value={tmpl.id}>{tmpl.name}</option>
          {/each}
        </select>
        {#if activeTemplate}
          <p class="m-0 text-[11px] leading-snug text-text-60">
            {activeTemplate.description}
          </p>
        {/if}
      </div>
      <div class="flex flex-col gap-1.5 text-xs text-text-60">
        <label for="new-project-location">Create in</label>
        <div class="flex gap-2">
          <input
            id="new-project-location"
            bind:value={location}
            class="mng-field selectable h-9 font-mono text-[12px]"
            placeholder="/home/you/papers"
            spellcheck="false"
            autocomplete="off"
          />
          {#if hasNativeDialogs()}
            <button
              type="button"
              class="mng-btn h-9 shrink-0"
              onclick={() => void browse()}>Browse</button
            >
          {/if}
        </div>
      </div>
      {#if slug !== ""}
        <p class="m-0 text-[11px] text-text-muted">
          The project will be created at <span class="selectable font-mono"
            >{target}</span
          >
        </p>
      {/if}
      {#if workspace.error !== ""}
        <p
          role="alert"
          class="m-0 border-l border-text-60 pl-3 text-xs leading-relaxed text-text"
        >
          {workspace.error}
        </p>
      {/if}
      <div class="flex justify-end gap-2">
        <button type="button" class="mng-btn h-9 px-4" onclick={close}>Cancel</button>
        <button type="submit" class="mng-btn h-9 px-4" disabled={!canCreate}
          >Create project</button
        >
      </div>
    </form>
  </div>
</div>
