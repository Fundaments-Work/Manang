<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";

  import Icon from "$lib/components/ui/Icon.svelte";
  import { kernel, type KernelCommand } from "$lib/kernel";
  import { groupPlugins } from "$lib/workbench/plugins";

  /**
   * Plugins: what the kernel has registered, grouped by plugin, with each
   * plugin's commands. This is the live list from `kernel.commands()`, not a
   * catalogue. Every command is also in the command palette.
   */

  let commands = $state.raw<KernelCommand[] | null>(null);
  let failed = $state("");
  const open = new SvelteSet<string>();

  onMount(() => {
    kernel
      .commands()
      .then((list) => (commands = list))
      .catch((error: unknown) => {
        failed = error instanceof Error ? error.message : "The kernel did not answer.";
        commands = [];
      });
  });

  const groups = $derived(groupPlugins(commands ?? []));

  function toggle(name: string): void {
    if (open.has(name)) open.delete(name);
    else open.add(name);
  }
</script>

<div class="mng-panel-head"><h2>Plugins</h2></div>

<div class="min-h-0 flex-1 overflow-y-auto pb-4">
  {#if commands === null}
    <p class="m-0 px-4 text-xs text-text-muted" role="status">Asking the kernel</p>
  {:else if groups.length === 0}
    <div class="px-4 text-xs leading-relaxed text-text-60">
      <p class="m-0">No plugins are registered.</p>
      <p class="mt-2 mb-0">
        {failed !== "" ? `${failed} ` : ""}Restart Manang to reconnect to the kernel.
        Its built-in plugins (editor, project and Typst) load at start-up.
      </p>
    </div>
  {:else}
    <p class="m-0 px-4 pb-2 text-[11px] text-text-muted">
      {groups.length}
      {groups.length === 1 ? "plugin" : "plugins"}, {commands?.length ?? 0} commands
    </p>
    <ul class="m-0 list-none p-0" aria-label="Registered plugins">
      {#each groups as group (group.name)}
        {@const expanded = open.has(group.name)}
        <li>
          <button
            type="button"
            class="inset-focus flex h-8 w-full items-center gap-1.5 px-3 text-left text-xs text-text hover:bg-surface-hi"
            aria-expanded={expanded}
            aria-controls="plugin-{group.name}"
            onclick={() => toggle(group.name)}
          >
            <Icon
              name={expanded ? "chevron-down" : "chevron-right"}
              size={12}
              class="shrink-0 text-text-60"
            />
            <span class="flex-1 truncate">{group.name}</span>
            <span class="font-mono text-[10px] text-text-muted"
              >{group.commands.length}</span
            >
          </button>
          {#if expanded}
            <ul
              id="plugin-{group.name}"
              class="m-0 list-none p-0 pb-1"
              aria-label="{group.name} commands"
            >
              {#each group.commands as command (command.id)}
                <li class="px-4 py-1 pl-9">
                  <span class="block truncate text-xs text-text-60"
                    >{command.title}</span
                  >
                  <span
                    class="selectable block truncate font-mono text-[10px] text-text-muted"
                    >{command.id}</span
                  >
                </li>
              {/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  <p
    class="m-0 mt-4 border-t border-line px-4 pt-3 text-[11px] leading-relaxed text-text-muted"
  >
    Third-party plugins are WebAssembly modules that run in a sandbox and can only use
    the access their manifest declares. Installing them is not available yet.
  </p>
</div>
