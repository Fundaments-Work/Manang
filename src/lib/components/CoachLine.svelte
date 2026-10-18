<script lang="ts">
  import {
    coachLines,
    type CoachTarget,
    COACH_MESSAGES,
  } from "$lib/state/coachLines.svelte";

  interface Props {
    target: CoachTarget;
    message?: string;
  }

  const { target, message = COACH_MESSAGES[target] }: Props = $props();
</script>

{#if coachLines.isVisible(target)}
  <div
    class="flex items-center justify-between gap-3 border-b border-line bg-surface px-3 py-1.5 text-xs text-text-60"
    role="region"
    aria-label="Onboarding tip"
    data-testid="coach-line-{target}"
  >
    <p class="m-0 min-w-0 flex-1 font-sans">
      {message}
    </p>
    <button
      type="button"
      class="cursor-pointer text-[11px] text-text-muted hover:text-text"
      onclick={() => coachLines.dismiss(target)}
    >
      Dismiss
    </button>
  </div>
{/if}
