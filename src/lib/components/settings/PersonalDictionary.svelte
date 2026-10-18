<script lang="ts">
  import { onMount } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";
  import { dictionary } from "$lib/state/dictionary.svelte";

  /**
   * The writing plugin's personal dictionary: the words added with "Add to
   * dictionary", each removable. Shown in the Writing plugin's section.
   */

  onMount(() => {
    void dictionary.load();
  });
</script>

<div class="border-t border-line-subtle py-4" data-dictionary>
  <div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
    <h4 id="personal-dictionary-title" class="m-0 text-[13px] font-normal text-text">
      Personal dictionary
    </h4>
    {#if dictionary.words.length > 0}
      <span class="font-mono text-[11px] text-text-muted"
        >{dictionary.words.length}</span
      >
    {/if}
  </div>
  <p class="m-0 mt-1 max-w-[60ch] text-xs leading-relaxed text-text-60">
    Words added with Add to dictionary are never flagged. Remove one to have it checked
    again.
  </p>

  {#if dictionary.error !== ""}
    <p class="m-0 mt-2 flex items-center gap-1.5 text-xs text-text" role="alert">
      <Icon name="error" size={12} class="shrink-0" />
      {dictionary.error}
    </p>
  {:else if !dictionary.loaded}
    <p class="m-0 mt-2 text-xs text-text-muted">Reading the dictionary…</p>
  {:else if dictionary.words.length === 0}
    <p class="m-0 mt-2 text-xs text-text-muted" data-dictionary-empty>
      No words yet. Choose Add to dictionary on a flagged word in the editor.
    </p>
  {:else}
    <ul
      class="m-0 mt-3 max-h-64 max-w-md list-none overflow-y-auto border border-line p-0"
      aria-labelledby="personal-dictionary-title"
    >
      {#each dictionary.words as word (word)}
        <li
          class="flex h-8 items-center justify-between gap-2 pr-1 pl-3 not-last:border-b not-last:border-line-subtle"
          data-word={word}
        >
          <span class="selectable min-w-0 truncate font-mono text-xs text-text"
            >{word}</span
          >
          <button
            type="button"
            class="mng-icon-btn"
            title="Remove from the dictionary"
            aria-label="Remove {word} from the personal dictionary"
            disabled={dictionary.removing.includes(word)}
            onclick={() => void dictionary.remove(word)}
          >
            <Icon name="close" size={12} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
