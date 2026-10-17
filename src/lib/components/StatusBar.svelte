<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import { inTauri } from "$lib/kernel";
  import { build, formatMs } from "$lib/state/build.svelte";
  import { documents } from "$lib/state/documents.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { notice } from "$lib/state/notice.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { countLabel } from "$lib/workbench/problems";

  /**
   * Status bar: what is true right now. Engine, compile state with timing, git
   * branch, caret position, word count and the kernel connection. Items that
   * lead somewhere are buttons; the rest are plain text.
   */

  const ENGINE_NAMES = {
    typst: "Typst",
    latex: "LaTeX",
    markdown: "Markdown",
  } as const;

  const isDocument = $derived(
    tabs.active?.kind === "document" && tabs.active.view === "text",
  );
  const engine = $derived(
    tabs.active === undefined ? null : workspace.engineFor(tabs.active.path),
  );
  const showsCompile = $derived(workspace.project !== null && build.status !== "idle");

  const compileText = $derived(
    build.status === "compiling"
      ? "Compiling"
      : build.status === "ok"
        ? `Compiled in ${formatMs(build.durationMs)}`
        : build.status === "failed"
          ? countLabel(build.counts)
          : build.status === "unsupported"
            ? "No compiler for this file"
            : build.status === "unavailable"
              ? "No compiler available"
              : "",
  );
  const warnings = $derived(build.status === "ok" && build.counts.warnings > 0);

  const kernelText = $derived(
    !documents.kernelOnline
      ? "Kernel offline"
      : inTauri()
        ? "Kernel connected"
        : "Demo kernel",
  );
</script>

<footer
  class="flex h-6 shrink-0 items-stretch border-t border-line bg-surface text-[11px] text-text-60"
  aria-label="Status"
>
  {#if workspace.branch !== null}
    <button
      type="button"
      class="mng-status"
      title="Git branch. Show changed files"
      onclick={() => layout.showSide("files")}
    >
      <Icon name="branch" size={12} />{workspace.branch}
    </button>
  {/if}

  {#if showsCompile}
    <button
      type="button"
      class="mng-status"
      class:text-text={build.status === "failed"}
      title="Show problems"
      onclick={() => layout.showPanel("problems")}
    >
      {#if build.status === "failed"}<Icon
          name="error"
          size={12}
        />{:else if warnings}<Icon name="warning" size={12} />{/if}
      <span role="status"
        >{compileText}{warnings ? `, ${countLabel(build.counts)}` : ""}</span
      >
    </button>
  {/if}

  <span
    class="min-w-0 flex-1 truncate px-3 leading-6 {notice.kind === 'error'
      ? 'text-text'
      : ''}"
    role="status"
    aria-live="polite"
  >
    {notice.text}
  </span>

  {#if isDocument}
    <span class="mng-status-text font-mono"
      >Ln {documents.cursor.line}, Col {documents.cursor.column}</span
    >
    <span class="mng-status-text"
      >{documents.words} {documents.words === 1 ? "word" : "words"}</span
    >
  {/if}
  {#if engine !== null}
    <span class="mng-status-text">{ENGINE_NAMES[engine]}</span>
  {/if}
  <span
    class="mng-status-text"
    class:text-text={!documents.kernelOnline}
    title={inTauri()
      ? "Connected to the Manang kernel"
      : "Running against an in-memory demo kernel, not the Rust engine"}
  >
    {kernelText}
  </span>
</footer>

<style>
  .mng-status {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 12px;
    color: inherit;
    white-space: nowrap;
  }
  .mng-status:hover {
    background: var(--mng-surface-hi);
    color: var(--mng-text);
  }
  .mng-status-text {
    display: inline-flex;
    align-items: center;
    padding: 0 12px;
    white-space: nowrap;
  }
</style>
