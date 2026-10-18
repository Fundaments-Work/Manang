<script lang="ts">
  import { tick } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";
  import { goToProblem } from "$lib/state/actions";
  import { agent } from "$lib/state/agent.svelte";
  import { build } from "$lib/state/build.svelte";
  import { diagram } from "$lib/state/diagram.svelte";
  import { layout } from "$lib/state/layout.svelte";
  import { writing } from "$lib/state/writing.svelte";
  import {
    countLabel,
    groupByFile,
    sortProblems,
    type Severity,
  } from "$lib/workbench/problems";
  import type { PanelTab } from "$lib/workbench/layout";

  /**
   * The bottom panel: Problems (compile diagnostics, click to jump to the
   * line) and Output (the compile log). The Problems tab carries the count, so
   * the state is visible even when the panel is closed elsewhere (status bar).
   */

  const tabsList: readonly { id: PanelTab; label: string }[] = [
    { id: "problems", label: "Problems" },
    { id: "output", label: "Output" },
  ];

  // Compiler diagnostics, diagram checks and prose-lint findings share one
  // list; they differ by `source`, never by being dropped.
  const all = $derived(
    sortProblems([...build.problems, ...diagram.problems, ...writing.problems]),
  );
  const groups = $derived(groupByFile(all));
  const total = $derived(all.length);
  const SEVERITY_LABEL: Record<Severity, string> = {
    error: "Error: ",
    warning: "Warning: ",
    info: "Suggestion: ",
  };
  let body: HTMLElement | undefined = $state();

  // Output follows the newest line.
  $effect(() => {
    void build.log.length;
    if (layout.panelTab !== "output" || body === undefined) return;
    void tick().then(() => {
      if (body !== undefined) body.scrollTop = body.scrollHeight;
    });
  });

  function onTabKey(event: KeyboardEvent): void {
    const at = tabsList.findIndex((t) => t.id === layout.panelTab);
    const next =
      event.key === "ArrowRight" ? at + 1 : event.key === "ArrowLeft" ? at - 1 : null;
    if (next === null) return;
    event.preventDefault();
    const target = tabsList[(next + tabsList.length) % tabsList.length];
    if (target === undefined) return;
    layout.setPanelTab(target.id);
    void tick().then(() => document.getElementById(`panel-tab-${target.id}`)?.focus());
  }

  function onRowKey(event: KeyboardEvent): void {
    const step = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
    if (step === 0) return;
    event.preventDefault();
    const rows = [...(body?.querySelectorAll<HTMLElement>("[data-problem]") ?? [])];
    const at = rows.indexOf(event.target as HTMLElement);
    rows[Math.max(0, Math.min(rows.length - 1, at + step))]?.focus();
  }

  const emptyText = $derived(
    diagram.active
      ? "No problems. The diagram draws cleanly."
      : build.status === "ok"
        ? "No problems. The document compiled cleanly."
        : build.status === "failed"
          ? "Build failed. Check the Output tab for compilation messages."
          : build.status === "unsupported"
            ? "Problems appear here for Typst, LaTeX and Markdown documents. This file has nothing to compile."
            : build.status === "unavailable"
              ? "No compiler is available, so there is nothing to check."
              : build.status === "compiling"
                ? "Compiling…"
                : "Nothing has been compiled yet. Open a document and problems appear here.",
  );
</script>

<section
  id="bottom-panel"
  class="flex min-h-0 flex-col bg-surface"
  aria-label="Panel"
  style="height: {layout.panelHeight}px"
>
  <div class="flex h-8 shrink-0 items-center border-b border-line px-2">
    <div
      role="tablist"
      aria-label="Panel views"
      class="flex h-full items-stretch"
      tabindex="-1"
      onkeydown={onTabKey}
    >
      {#each tabsList as item (item.id)}
        {@const active = layout.panelTab === item.id}
        <button
          type="button"
          role="tab"
          id="panel-tab-{item.id}"
          aria-selected={active}
          aria-controls="panel-body"
          tabindex={active ? 0 : -1}
          class="inset-focus relative flex items-center gap-1.5 px-3 text-xs {active
            ? 'text-text'
            : 'text-text-60 hover:text-text'}"
          onclick={() => layout.setPanelTab(item.id)}
        >
          {item.label}
          {#if item.id === "problems" && total > 0}
            <span
              class="font-mono text-[10px] text-text-60"
              aria-label="{total} {total === 1 ? 'problem' : 'problems'}">{total}</span
            >
          {/if}
          {#if active}<span
              class="absolute inset-x-0 bottom-0 h-px bg-accent"
              aria-hidden="true"
            ></span>{/if}
        </button>
      {/each}
    </div>
    <span class="ml-auto"></span>
    {#if layout.panelTab === "output"}
      <button
        type="button"
        class="mng-btn mr-1 h-6 px-2"
        disabled={build.log.length === 0}
        onclick={() => build.clearLog()}>Clear output</button
      >
    {/if}
    <button
      type="button"
      class="mng-icon-btn h-6 w-6"
      aria-label="Close panel"
      title="Close panel (Ctrl+J)"
      onclick={() => layout.closePanel()}
    >
      <Icon name="close" size={12} />
    </button>
  </div>

  <div
    bind:this={body}
    id="panel-body"
    role="tabpanel"
    aria-labelledby="panel-tab-{layout.panelTab}"
    class="min-h-0 flex-1 overflow-y-auto"
  >
    {#if layout.panelTab === "problems"}
      {#if groups.length === 0}
        {#if build.status === "failed"}
          <div class="m-3 border border-line bg-surface p-3 text-xs text-text" role="alert">
            <div class="flex items-center gap-1.5 text-text">
              <Icon name="error" size={13} />
              <span>Compilation failed</span>
            </div>
            <p class="mt-1 mb-2 leading-relaxed text-text-60">
              The compiler reported a failure, but no source line diagnostics were generated. Check the Output tab to inspect the raw engine messages.
            </p>
            <button
              type="button"
              class="mng-btn text-xs"
              onclick={() => layout.setPanelTab("output")}
            >
              View compile output
            </button>
          </div>
        {:else}
          <p class="m-0 px-4 py-3 text-xs text-text-60" role="status">{emptyText}</p>
        {/if}
      {:else}
        <p class="sr-only" role="status" aria-live="polite">
          {countLabel(build.counts)}
        </p>
        {#each groups as group (group.file)}
          <div role="group" aria-label={group.file === "" ? "Other files" : group.file}>
            <p
              class="m-0 flex items-baseline gap-2 px-4 pt-2 pb-1 text-[11px] text-text-60"
            >
              {group.file === "" ? "Packages and other files" : group.file}
              <span class="font-mono text-[10px] text-text-muted"
                >{group.items.length}</span
              >
            </p>
            {#each group.items as problem (problem.id)}
              <div class="flex items-start hover:bg-surface-hi">
                <button
                  type="button"
                  data-problem
                  class="inset-focus flex min-w-0 flex-1 items-start gap-2.5 px-4 py-1 text-left text-xs disabled:cursor-default"
                  disabled={problem.file === ""}
                  onclick={() => void goToProblem(problem)}
                  onkeydown={onRowKey}
                >
                  <Icon
                    name={problem.severity === "error" ? "error" : "warning"}
                    size={14}
                    class="mt-px shrink-0 {problem.severity === 'error'
                      ? 'text-text'
                      : problem.severity === 'warning'
                        ? 'text-text-60'
                        : 'text-text-muted'}"
                  />
                  <span class="min-w-0 flex-1">
                    <span class="sr-only">{SEVERITY_LABEL[problem.severity]}</span>
                    <span class="text-text">{problem.message}</span>
                    {#if problem.source !== undefined}
                      <span class="ml-2 font-mono text-[10px] text-text-muted"
                        >{problem.source}{problem.code ? `: ${problem.code}` : ""}</span
                      >
                    {/if}
                    {#each problem.hints as hint (hint)}
                      <span class="block text-[11px] text-text-60">Hint: {hint}</span>
                    {/each}
                  </span>
                  {#if problem.line !== null}
                    <span class="shrink-0 font-mono text-[10px] text-text-muted"
                      >Ln {problem.line}, Col {problem.column ?? 1}</span
                    >
                  {/if}
                </button>
                {#if problem.file !== "" && problem.severity !== "info"}
                  <button
                    type="button"
                    class="mng-lime-hover inset-focus h-6 shrink-0 px-3 text-[11px] text-text-60"
                    data-fix-with-ai
                    aria-label="Fix with AI: {problem.message}"
                    onclick={() => void agent.fixProblem(problem)}>Fix with AI</button
                  >
                {/if}
              </div>
            {/each}
          </div>
        {/each}
      {/if}
    {:else}
      <div
        role="log"
        aria-label="Compile output"
        class="selectable px-4 py-2 font-mono text-[11px] leading-relaxed"
      >
        {#if build.log.length === 0}
          <div class="py-2 font-sans text-xs text-text-60">
            <p class="m-0">No output yet. Compile messages and build logs appear here when you compile a document.</p>
            {#if build.status === "unavailable"}
              <p class="mt-1 mb-0 text-text-muted">The compiler plugin is not available.</p>
            {/if}
          </div>
        {/if}
        {#each build.log as line (line.id)}
          <div class={line.level === "error" ? "text-text" : "text-text-60"}>
            <span class="text-text-muted">{line.time}</span>
            {line.text}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</section>
