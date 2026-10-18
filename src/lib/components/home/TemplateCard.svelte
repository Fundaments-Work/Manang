<script lang="ts">
  import type { TemplateSummary } from "$lib/kernel/types";
  import { ENGINE_LABEL } from "$lib/workbench/home";

  interface Props {
    template: TemplateSummary;
    onuse: (template: TemplateSummary) => void;
  }

  const { template, onuse }: Props = $props();
</script>

<article
  class="group flex flex-col border border-line bg-surface p-3.5 transition-colors hover:bg-surface-hi"
  data-template={template.id}
>
  <!-- Thumbnail placeholder wireframe -->
  <div
    class="relative mb-3 flex h-24 w-full items-center justify-center overflow-hidden border border-line bg-base"
    aria-hidden="true"
  >
    {#if template.id.includes("slides")}
      <!-- 16:9 Presentation slide layout -->
      <svg class="h-16 w-28 text-line" viewBox="0 0 112 63" fill="none">
        <rect
          x="0.5"
          y="0.5"
          width="111"
          height="62"
          stroke="currentColor"
          fill="#18181b"
        />
        <line x1="8" y1="12" x2="50" y2="12" stroke="currentColor" stroke-width="2" />
        <line
          x1="8"
          y1="22"
          x2="90"
          y2="22"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <line
          x1="8"
          y1="30"
          x2="75"
          y2="30"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <rect x="8" y="38" width="40" height="16" stroke="currentColor" />
        <circle cx="14" cy="46" r="2" fill="currentColor" />
        <line x1="20" y1="46" x2="42" y2="46" stroke="currentColor" />
      </svg>
    {:else if template.id.includes("poster")}
      <!-- 3-column poster layout -->
      <svg class="h-20 w-16 text-line" viewBox="0 0 64 80" fill="none">
        <rect
          x="0.5"
          y="0.5"
          width="63"
          height="79"
          stroke="currentColor"
          fill="#18181b"
        />
        <rect x="4" y="4" width="56" height="12" stroke="currentColor" />
        <rect
          x="4"
          y="20"
          width="16"
          height="54"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <rect
          x="24"
          y="20"
          width="16"
          height="54"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <rect
          x="44"
          y="20"
          width="16"
          height="54"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
      </svg>
    {:else if template.id.includes("letter")}
      <!-- Letter layout -->
      <svg class="h-20 w-16 text-line" viewBox="0 0 64 80" fill="none">
        <rect
          x="0.5"
          y="0.5"
          width="63"
          height="79"
          stroke="currentColor"
          fill="#18181b"
        />
        <line x1="6" y1="8" x2="24" y2="8" stroke="currentColor" />
        <line
          x1="6"
          y1="13"
          x2="20"
          y2="13"
          stroke="currentColor"
          stroke-dasharray="1 1"
        />
        <line
          x1="38"
          y1="8"
          x2="58"
          y2="8"
          stroke="currentColor"
          stroke-dasharray="1 1"
        />
        <line x1="6" y1="26" x2="58" y2="26" stroke="currentColor" />
        <line
          x1="6"
          y1="33"
          x2="54"
          y2="33"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <line
          x1="6"
          y1="39"
          x2="58"
          y2="39"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <line
          x1="6"
          y1="45"
          x2="48"
          y2="45"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <line x1="6" y1="62" x2="26" y2="62" stroke="currentColor" />
      </svg>
    {:else if template.id.includes("thesis")}
      <!-- Thesis chapter layout -->
      <svg class="h-20 w-16 text-line" viewBox="0 0 64 80" fill="none">
        <rect
          x="0.5"
          y="0.5"
          width="63"
          height="79"
          stroke="currentColor"
          fill="#18181b"
        />
        <line x1="6" y1="8" x2="20" y2="8" stroke="currentColor" stroke-width="0.75" />
        <line x1="6" y1="14" x2="48" y2="14" stroke="currentColor" stroke-width="2" />
        <line x1="6" y1="22" x2="30" y2="22" stroke="currentColor" />
        <line
          x1="6"
          y1="28"
          x2="58"
          y2="28"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <line
          x1="6"
          y1="34"
          x2="58"
          y2="34"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <line
          x1="6"
          y1="40"
          x2="44"
          y2="40"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <line x1="6" y1="50" x2="32" y2="50" stroke="currentColor" />
        <line
          x1="6"
          y1="56"
          x2="58"
          y2="56"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
      </svg>
    {:else if template.id.includes("notes")}
      <!-- Notes with diagram layout -->
      <svg class="h-20 w-16 text-line" viewBox="0 0 64 80" fill="none">
        <rect
          x="0.5"
          y="0.5"
          width="63"
          height="79"
          stroke="currentColor"
          fill="#18181b"
        />
        <line x1="6" y1="8" x2="38" y2="8" stroke="currentColor" stroke-width="1.5" />
        <line
          x1="6"
          y1="16"
          x2="54"
          y2="16"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <!-- Miniature flowchart -->
        <rect x="8" y="24" width="16" height="10" stroke="currentColor" />
        <line x1="24" y1="29" x2="36" y2="29" stroke="currentColor" />
        <rect x="36" y="24" width="18" height="10" stroke="currentColor" />
        <line x1="45" y1="34" x2="45" y2="44" stroke="currentColor" />
        <rect x="36" y="44" width="18" height="10" stroke="currentColor" />
        <line
          x1="6"
          y1="62"
          x2="50"
          y2="62"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
      </svg>
    {:else if template.id.includes("lab-report")}
      <!-- Lab report layout -->
      <svg class="h-20 w-16 text-line" viewBox="0 0 64 80" fill="none">
        <rect
          x="0.5"
          y="0.5"
          width="63"
          height="79"
          stroke="currentColor"
          fill="#18181b"
        />
        <line x1="12" y1="8" x2="52" y2="8" stroke="currentColor" stroke-width="1.5" />
        <line
          x1="20"
          y1="14"
          x2="44"
          y2="14"
          stroke="currentColor"
          stroke-dasharray="1 1"
        />
        <line x1="6" y1="24" x2="24" y2="24" stroke="currentColor" />
        <line
          x1="6"
          y1="30"
          x2="58"
          y2="30"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <line
          x1="6"
          y1="36"
          x2="50"
          y2="36"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <!-- Table mock -->
        <rect x="6" y="44" width="52" height="18" stroke="currentColor" />
        <line x1="6" y1="50" x2="58" y2="50" stroke="currentColor" />
        <line x1="32" y1="44" x2="32" y2="62" stroke="currentColor" />
      </svg>
    {:else if template.id.includes("article")}
      <!-- Standard article layout -->
      <svg class="h-20 w-16 text-line" viewBox="0 0 64 80" fill="none">
        <rect
          x="0.5"
          y="0.5"
          width="63"
          height="79"
          stroke="currentColor"
          fill="#18181b"
        />
        <line x1="12" y1="8" x2="52" y2="8" stroke="currentColor" stroke-width="1.5" />
        <line
          x1="20"
          y1="14"
          x2="44"
          y2="14"
          stroke="currentColor"
          stroke-dasharray="1 1"
        />
        <!-- Abstract inset box -->
        <rect
          x="10"
          y="20"
          width="44"
          height="12"
          stroke="currentColor"
          stroke-dasharray="1 1"
        />
        <line x1="6" y1="38" x2="26" y2="38" stroke="currentColor" />
        <line
          x1="6"
          y1="44"
          x2="58"
          y2="44"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <line
          x1="6"
          y1="50"
          x2="58"
          y2="50"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
        <line
          x1="6"
          y1="56"
          x2="48"
          y2="56"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
      </svg>
    {:else}
      <!-- Blank document layout -->
      <svg class="h-20 w-16 text-line" viewBox="0 0 64 80" fill="none">
        <rect
          x="0.5"
          y="0.5"
          width="63"
          height="79"
          stroke="currentColor"
          fill="#18181b"
        />
        <line x1="6" y1="8" x2="22" y2="8" stroke="currentColor" />
        <line
          x1="6"
          y1="16"
          x2="18"
          y2="16"
          stroke="currentColor"
          stroke-dasharray="2 2"
        />
      </svg>
    {/if}
  </div>

  <!-- Header: name & engine badge -->
  <div class="flex items-start justify-between gap-2">
    <h3 class="m-0 text-[13px] font-normal text-text">
      {template.name}
    </h3>
    <span
      class="shrink-0 border border-line px-1.5 py-0.5 font-mono text-[10px] text-text-muted"
    >
      {ENGINE_LABEL[template.engine] ?? template.engine}
    </span>
  </div>

  <!-- Description -->
  <p class="m-0 mt-1.5 text-[11px] leading-snug text-text-60">
    {template.description}
  </p>

  <!-- Tags -->
  {#if template.tags.length > 0}
    <div class="mt-2.5 flex flex-wrap gap-1">
      {#each template.tags as tag (tag)}
        <span
          class="border border-line bg-surface-hi px-1.5 py-0.5 font-mono text-[10px] text-text-muted"
        >
          {tag}
        </span>
      {/each}
    </div>
  {/if}

  <!-- Action button -->
  <div class="mt-4 pt-2 border-t border-line">
    <button
      type="button"
      class="mng-btn w-full justify-center text-xs"
      onclick={() => onuse(template)}
    >
      Use template
    </button>
  </div>
</article>
