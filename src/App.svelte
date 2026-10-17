<script lang="ts">
  import { onMount } from "svelte";

  import Workbench from "$lib/components/Workbench.svelte";
  import { boot } from "$lib/state/actions";

  /**
   * Manang's shell. It composes the workbench and starts it; engine work stays
   * in the Rust kernel, and each panel owns its own small piece of state.
   */
  onMount(() => void boot());
</script>

<Workbench />
