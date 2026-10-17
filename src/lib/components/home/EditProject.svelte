<script lang="ts">
  import { onMount } from "svelte";

  import type { ProjectEntry } from "$lib/kernel/types";
  import { home } from "$lib/state/home.svelte";
  import { parseTags } from "$lib/workbench/home";

  /**
   * Rename a project or edit its tags. The name is the title shown on Home; the
   * folder on disk keeps its name. Enter saves, Esc cancels.
   */
  interface Props {
    mode: "name" | "tags";
    entry: ProjectEntry;
    onclose: () => void;
  }

  const { mode, entry, onclose }: Props = $props();

  // The dialog opens on one project; it is not meant to follow later changes.
  // svelte-ignore state_referenced_locally
  let value = $state(mode === "name" ? entry.name : entry.tags.join(", "));
  let input: HTMLInputElement | undefined = $state();
  let box: HTMLDivElement | undefined = $state();
  let saving = $state(false);
  const returnTo =
    document.activeElement instanceof HTMLElement ? document.activeElement : null;

  const valid = $derived(mode === "tags" || value.trim() !== "");

  onMount(() => {
    input?.focus();
    input?.select();
  });

  function close(): void {
    onclose();
    returnTo?.focus();
  }

  async function submit(event: SubmitEvent): Promise<void> {
    event.preventDefault();
    if (!valid || saving) return;
    saving = true;
    const ok = await home.patch(
      entry.path,
      mode === "name" ? { name: value.trim() } : { tags: parseTags(value) },
    );
    saving = false;
    if (ok) close();
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
    ];
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
  class="fixed inset-0 z-50 flex items-start justify-center pt-[22vh]"
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
    aria-labelledby="edit-title"
    class="relative w-[420px] max-w-[92vw] border border-line bg-surface p-5"
  >
    <h2
      id="edit-title"
      class="m-0 text-[15px] font-light text-text"
      style="letter-spacing: var(--tracking-headline)"
    >
      {mode === "name" ? "Rename project" : "Edit tags"}
    </h2>
    <form class="mt-4 flex flex-col gap-3" onsubmit={submit}>
      <label class="flex flex-col gap-1.5 text-xs text-text-60">
        {mode === "name" ? "Name" : "Tags, separated by commas"}
        <input
          bind:this={input}
          bind:value
          class="mng-field h-8 text-[13px]"
          placeholder={mode === "name" ? "Entropy paper" : "thesis, draft"}
          autocomplete="off"
          spellcheck="false"
        />
      </label>
      {#if mode === "name"}
        <p class="m-0 text-[11px] leading-snug text-text-muted">
          Only the title on Home changes. The folder keeps its name.
        </p>
      {/if}
      <div class="mt-1 flex justify-end gap-2">
        <button type="button" class="mng-btn" onclick={close}>Cancel</button>
        <button type="submit" class="mng-btn" disabled={!valid || saving}>Save</button>
      </div>
    </form>
  </div>
</div>
