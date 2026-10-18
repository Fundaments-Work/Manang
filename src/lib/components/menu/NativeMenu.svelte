<script lang="ts">
  import { onMount } from "svelte";

  import { reportError } from "$lib/kernel";

  import { menuModel } from "./menuModel.svelte";
  import { applyNativeMenu } from "./nativeMenu";

  /**
   * macOS only: keeps the application menu in step with the menu model
   * (enabled states, titles, shortcuts changed in Settings). Renders nothing.
   * Rebuilds are coalesced, since a burst of state changes is common.
   */

  const REBUILD_MS = 150;

  onMount(() => menuModel.load());

  $effect(() => {
    const menus = menuModel.build();
    const timer = setTimeout(() => {
      applyNativeMenu(menus).catch((error: unknown) =>
        reportError("the application menu could not be updated", error),
      );
    }, REBUILD_MS);
    return () => clearTimeout(timer);
  });
</script>
