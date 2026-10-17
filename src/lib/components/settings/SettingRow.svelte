<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import type { SettingDef } from "$lib/kernel/api";
  import {
    formatSettingTitle,
    isSettingModified,
    validateSettingValue,
  } from "$lib/workbench/settings";

  interface Props {
    def: SettingDef;
    value: unknown;
    source?: "default" | "user" | "project";
    onchange: (value: unknown) => void;
    onreset: () => void;
  }

  const { def, value, source, onchange, onreset }: Props = $props();

  let inlineError = $state<string | null>(null);

  const modified = $derived(isSettingModified(value, def.default, source));
  const title = $derived(formatSettingTitle(def.id));

  function handleBoolToggle(): void {
    const next = !(value === true);
    inlineError = null;
    onchange(next);
  }

  function handleNumberInput(valStr: string, isInt: boolean): void {
    const parsed = isInt ? parseInt(valStr, 10) : parseFloat(valStr);
    const check = validateSettingValue(def, parsed);
    if (!check.valid) {
      inlineError = check.error ?? "Invalid value";
      return;
    }
    inlineError = null;
    onchange(parsed);
  }

  function handleSelect(val: string): void {
    const check = validateSettingValue(def, val);
    if (!check.valid) {
      inlineError = check.error ?? "Invalid value";
      return;
    }
    inlineError = null;
    onchange(val);
  }

  function handleTextInput(val: string): void {
    const check = validateSettingValue(def, val);
    if (!check.valid) {
      inlineError = check.error ?? "Invalid value";
      return;
    }
    inlineError = null;
    onchange(val);
  }
</script>

<div class="border-b border-line/60 px-4 py-3 hover:bg-surface-hi/40">
  <div class="flex items-start justify-between gap-4">
    <div class="flex-1 min-w-0">
      <div class="flex items-center gap-2">
        <label for={`setting-${def.id}`} class="text-xs font-medium text-text select-none">
          {title}
        </label>
        {#if modified}
          <span
            class="flex items-center gap-1 font-mono text-[9px] text-accent"
            title="Modified from default"
          >
            <span class="h-1.5 w-1.5 bg-accent" aria-hidden="true"></span>
            <span>{source === "project" ? "project" : "modified"}</span>
          </span>
        {/if}
        {#if def.requires_restart}
          <span class="font-mono text-[9px] text-text-muted" title="Requires app restart to take effect">
            (restart)
          </span>
        {/if}
      </div>

      <p class="m-0 mt-0.5 text-[11px] leading-relaxed text-text-60">
        {def.description}
      </p>

      <div class="mt-1 font-mono text-[10px] text-text-muted">
        {def.id}
      </div>

      {#if inlineError}
        <div class="mt-1 flex items-center gap-1 text-[11px] text-red-400">
          <Icon name="error" size={12} />
          <span>{inlineError}</span>
        </div>
      {/if}
    </div>

    <!-- Control by type -->
    <div class="flex shrink-0 items-center gap-2">
      {#if def.type === "bool"}
        <button
          id={`setting-${def.id}`}
          type="button"
          role="switch"
          aria-label={`Toggle ${title}`}
          title={`Toggle ${title}`}
          aria-checked={value === true}
          class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer items-center border border-line bg-surface p-0.5 transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent {value === true
            ? 'bg-accent/20 border-accent'
            : ''}"
          onclick={handleBoolToggle}
        >
          <span
            class="h-3.5 w-3.5 transition-transform {value === true
              ? 'translate-x-4 bg-accent'
              : 'translate-x-0 bg-text-muted'}"
          ></span>
        </button>
      {:else if def.type === "int" || def.type === "float"}
        <div class="flex items-center gap-2">
          {#if def.min !== undefined && def.max !== undefined}
            <input
              type="range"
              min={def.min}
              max={def.max}
              step={def.type === "int" ? 1 : 0.05}
              value={typeof value === "number" ? value : def.default}
              class="w-20 accent-accent"
              oninput={(e) => handleNumberInput(e.currentTarget.value, def.type === "int")}
            />
          {/if}
          <input
            id={`setting-${def.id}`}
            type="number"
            min={def.min}
            max={def.max}
            step={def.type === "int" ? 1 : 0.05}
            value={typeof value === "number" ? value : def.default}
            class="mng-input w-20 text-right font-mono text-xs"
            oninput={(e) => handleNumberInput(e.currentTarget.value, def.type === "int")}
          />
        </div>
      {:else if def.type === "enum"}
        <select
          id={`setting-${def.id}`}
          class="mng-input max-w-44 text-xs"
          value={String(value ?? def.default)}
          onchange={(e) => handleSelect(e.currentTarget.value)}
        >
          {#each def.values ?? [] as opt (opt)}
            <option value={opt}>{opt}</option>
          {/each}
        </select>
      {:else}
        <input
          id={`setting-${def.id}`}
          type="text"
          value={String(value ?? "")}
          class="mng-input w-44 text-xs"
          oninput={(e) => handleTextInput(e.currentTarget.value)}
        />
      {/if}

      <!-- Reset button -->
      {#if modified}
        <button
          type="button"
          class="mng-icon-btn text-text-60 hover:text-text"
          title="Reset to default value"
          aria-label={`Reset ${title} to default`}
          onclick={onreset}
        >
          <Icon name="refresh" size={13} />
        </button>
      {/if}
    </div>
  </div>
</div>
