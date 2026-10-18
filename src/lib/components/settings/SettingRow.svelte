<script lang="ts">
  import Icon from "$lib/components/ui/Icon.svelte";
  import type { SettingDef } from "$lib/kernel/api";
  import {
    formatCategoryLabel,
    isSettingModified,
    settingTitle,
    validateSettingValue,
  } from "$lib/workbench/settings";

  /**
   * One setting: label, description and id on the left, its control on the
   * right; stacked when the page is narrow (a container query on the page).
   *
   * Text and list values are written when the field is committed (Enter or
   * leaving it), not on every keystroke, since each write rewrites a TOML file.
   */

  interface Props {
    def: SettingDef;
    value: unknown;
    source?: "default" | "user" | "project";
    /** Set when the selected layer cannot hold this setting. */
    disabledReason?: string | undefined;
    onchange: (value: unknown) => Promise<string>;
    onreset: () => Promise<string>;
  }

  const { def, value, source, disabledReason, onchange, onreset }: Props = $props();

  let error = $state("");

  const title = $derived(settingTitle(def));
  const modified = $derived(isSettingModified(value, def.default, source));
  const control = $derived(`setting-${def.id}`);
  const disabled = $derived(disabledReason !== undefined);
  /** Short option lists read better as a segmented control than a menu. */
  const segmented = $derived(
    def.type === "enum" &&
      (def.values?.length ?? 0) <= 4 &&
      (def.values ?? []).join("").length <= 28,
  );

  async function commit(next: unknown): Promise<void> {
    const check = validateSettingValue(def, next);
    if (!check.valid) {
      error = check.error ?? "That value is not allowed.";
      return;
    }
    error = await onchange(next);
  }

  async function reset(): Promise<void> {
    error = await onreset();
  }

  function numberFrom(text: string): number {
    return def.type === "int" ? Number.parseInt(text, 10) : Number.parseFloat(text);
  }

  const listText = $derived(Array.isArray(value) ? value.map(String).join("\n") : "");
</script>

<div
  class="grid gap-x-8 gap-y-3 border-t border-line-subtle py-4 first:border-t-0 @xl:grid-cols-[minmax(0,1fr)_auto]"
  data-setting={def.id}
>
  <div class="min-w-0">
    <div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
      <label for={control} class="text-[13px] text-text">{title}</label>
      {#if modified}
        <span class="text-[11px] text-text-60">
          {source === "project" ? "Set for this project" : "Changed"}
        </span>
      {/if}
      {#if def["requires-restart"]}
        <span class="text-[11px] text-text-60">Applies after restart</span>
      {/if}
    </div>
    <p class="m-0 mt-1 max-w-[60ch] text-xs leading-relaxed text-text-60">
      {def.description}
    </p>
    <p class="selectable m-0 mt-1 font-mono text-[11px] text-text-muted">{def.id}</p>
    {#if disabledReason !== undefined}
      <p class="m-0 mt-2 text-xs text-text-60">{disabledReason}</p>
    {/if}
    {#if error !== ""}
      <p class="m-0 mt-2 flex items-center gap-1.5 text-xs text-text" role="alert">
        <Icon name="error" size={12} class="shrink-0" />
        {error}
      </p>
    {/if}
  </div>

  <div class="flex min-w-0 items-center gap-2 @xl:justify-end">
    {#if def.type === "bool"}
      <button
        id={control}
        type="button"
        role="switch"
        aria-checked={value === true}
        {disabled}
        class="inset-focus flex h-7 items-center gap-2 border border-line bg-bg px-1.5 text-xs text-text-60 hover:border-text-60 disabled:opacity-40"
        onclick={() => void commit(!(value === true))}
      >
        <span
          class="flex h-4 w-7 items-center border border-line-subtle bg-surface-hi px-0.5 {value ===
          true
            ? 'justify-end'
            : 'justify-start'}"
          aria-hidden="true"
        >
          <span class="h-2.5 w-2.5 {value === true ? 'bg-text' : 'bg-text-muted'}"
          ></span>
        </span>
        <span class="w-6 text-left {value === true ? 'text-text' : ''}"
          >{value === true ? "On" : "Off"}</span
        >
      </button>
    {:else if def.type === "int" || def.type === "float"}
      {@const current = typeof value === "number" ? value : Number(def.default)}
      {#if def.min !== undefined && def.max !== undefined}
        <input
          type="range"
          aria-label="{title} slider"
          min={def.min}
          max={def.max}
          step={def.type === "int" ? 1 : 0.05}
          value={current}
          {disabled}
          class="mng-range w-32"
          onchange={(e) => void commit(numberFrom(e.currentTarget.value))}
        />
      {/if}
      <input
        id={control}
        type="number"
        min={def.min}
        max={def.max}
        step={def.type === "int" ? 1 : 0.05}
        value={current}
        {disabled}
        class="mng-field w-20 text-right font-mono"
        onchange={(e) => void commit(numberFrom(e.currentTarget.value))}
      />
    {:else if def.type === "enum" && segmented}
      <div
        id={control}
        role="radiogroup"
        aria-label={title}
        class="flex border border-line"
      >
        {#each def.values ?? [] as option (option)}
          <button
            type="button"
            role="radio"
            aria-checked={value === option}
            {disabled}
            class="inset-focus h-7 px-3 text-xs not-last:border-r not-last:border-line disabled:opacity-40 {value ===
            option
              ? 'bg-surface-hi text-text'
              : 'bg-bg text-text-60 hover:text-text'}"
            onclick={() => void commit(option)}
          >
            {formatCategoryLabel(option)}
          </button>
        {/each}
      </div>
    {:else if def.type === "enum"}
      <select
        id={control}
        class="mng-field w-48"
        value={String(value ?? def.default)}
        {disabled}
        onchange={(e) => void commit(e.currentTarget.value)}
      >
        {#each def.values ?? [] as option (option)}
          <option value={option}>{formatCategoryLabel(option)}</option>
        {/each}
      </select>
    {:else if def.type === "list"}
      <textarea
        id={control}
        rows="3"
        class="mng-field h-auto w-64 py-1.5 font-mono leading-relaxed"
        placeholder="One per line"
        value={listText}
        {disabled}
        onchange={(e) =>
          void commit(
            e.currentTarget.value
              .split("\n")
              .map((line) => line.trim())
              .filter((line) => line !== ""),
          )}
      ></textarea>
    {:else}
      <input
        id={control}
        type="text"
        value={String(value ?? "")}
        {disabled}
        class="mng-field w-64 {def.type === 'path' ? 'font-mono' : ''}"
        onchange={(e) => void commit(e.currentTarget.value)}
      />
    {/if}

    <button
      type="button"
      class="mng-icon-btn {modified && !disabled ? '' : 'invisible'}"
      title="Reset to default"
      aria-label="Reset {title} to default"
      tabindex={modified && !disabled ? 0 : -1}
      onclick={() => void reset()}
    >
      <Icon name="undo" size={14} />
    </button>
  </div>
</div>
