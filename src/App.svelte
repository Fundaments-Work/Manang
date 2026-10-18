<script lang="ts">
  import { onMount } from "svelte";

  import Workbench from "$lib/components/Workbench.svelte";
  import { setWindowTitle } from "$lib/kernel/native";
  import { boot } from "$lib/state/actions";
  import { context } from "$lib/state/context.svelte";
  import { tabs } from "$lib/state/tabs.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { windowTitle } from "$lib/workbench/windowTitle";

  /**
   * Manang's shell. It composes the workbench and starts it; engine work stays
   * in the Rust kernel, and each panel owns its own small piece of state.
   */
  onMount(() => void boot());

  // The title says what is open (taskbar, alt-tab); the top bar has no breadcrumb.
  $effect(() => {
    void setWindowTitle(
      windowTitle({
        surface: context.surface,
        project: workspace.project?.name ?? null,
        file: tabs.active?.path ?? null,
      }),
    );
  });
</script>

<Workbench />
