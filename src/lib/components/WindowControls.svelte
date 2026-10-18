<script lang="ts">
  import { onMount } from "svelte";

  import Icon from "$lib/components/ui/Icon.svelte";
  import { onMaximizedChange, windowAction } from "$lib/kernel/native";

  /**
   * Minimise, maximise and close, drawn by Manang because the window has no
   * system title bar (`decorations: false`). Square 46x40 targets flush with
   * the top bar's edge, so a fling of the pointer to the corner closes the
   * window, as on Windows and most Linux desktops.
   */

  let maximized = $state(false);

  onMount(() => {
    let stop: (() => void) | undefined;
    void onMaximizedChange((value) => (maximized = value)).then((s) => (stop = s));
    return () => stop?.();
  });
</script>

<div class="flex h-full shrink-0 items-stretch" role="group" aria-label="Window">
  <button
    type="button"
    class="inset-focus flex w-[46px] items-center justify-center text-text-60 hover:bg-surface-hi hover:text-text"
    aria-label="Minimise"
    title="Minimise"
    onclick={() => void windowAction("minimize")}
  >
    <Icon name="window-minimize" size={14} />
  </button>
  <button
    type="button"
    class="inset-focus flex w-[46px] items-center justify-center text-text-60 hover:bg-surface-hi hover:text-text"
    aria-label={maximized ? "Restore" : "Maximise"}
    title={maximized ? "Restore" : "Maximise"}
    onclick={() => void windowAction("toggle-maximize")}
  >
    <Icon name={maximized ? "window-restore" : "window-maximize"} size={14} />
  </button>
  <button
    type="button"
    class="inset-focus flex w-[46px] items-center justify-center text-text-60 hover:bg-text hover:text-bg"
    aria-label="Close window"
    title="Close"
    onclick={() => void windowAction("close")}
  >
    <Icon name="window-close" size={14} />
  </button>
</div>
