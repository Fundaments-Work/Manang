<script lang="ts">
  import { engine } from "$lib/state/engine.svelte";
  import { formatSize, progressFraction, progressLabel } from "$lib/workbench/engine";

  /**
   * The consent dialog for the optional built-in LaTeX engine. It says exactly
   * what is downloaded, from where, how big it is, its SHA-256, where it is
   * saved, and that LaTeX packages are fetched on first use. Nothing is
   * downloaded until Download is pressed. Esc cancels; while a download runs,
   * Esc or Cancel stops it.
   */

  let box: HTMLDivElement | undefined = $state();
  let returnTo: HTMLElement | null = null;

  const status = $derived(engine.status);

  $effect(() => {
    if (engine.step === "closed") return;
    returnTo =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    return () => returnTo?.focus();
  });

  // Move focus to the first button whenever the step changes.
  $effect(() => {
    void engine.step;
    queueMicrotask(() => box?.querySelector<HTMLElement>("button")?.focus());
  });

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      void engine.cancel();
      return;
    }
    if (event.key !== "Tab" || box === undefined) return;
    const buttons = [...box.querySelectorAll<HTMLElement>("button")];
    const first = buttons[0];
    const last = buttons[buttons.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last?.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first?.focus();
    }
  }
</script>

{#if engine.step !== "closed" && status !== null}
  <div
    class="fixed inset-0 z-50 flex items-start justify-center pt-[14vh]"
    {onkeydown}
    role="presentation"
    data-engine-dialog={engine.step}
  >
    <button
      type="button"
      tabindex="-1"
      class="absolute inset-0 cursor-default"
      aria-label="Close"
      onclick={() => void engine.cancel()}
    ></button>
    <div
      bind:this={box}
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="engine-title"
      aria-describedby="engine-message"
      class="relative max-h-[78vh] w-[480px] max-w-[92vw] overflow-y-auto border border-line bg-surface p-5"
    >
      <h2
        id="engine-title"
        class="m-0 text-[15px] font-light text-text"
        style="letter-spacing: var(--tracking-headline)"
      >
        {#if engine.step === "done"}
          The built-in engine is ready
        {:else if engine.step === "failed"}
          The download did not finish
        {:else if engine.step === "running"}
          Downloading the built-in engine
        {:else}
          Download the built-in LaTeX engine
        {/if}
      </h2>

      {#if engine.step === "consent"}
        <div id="engine-message" class="mt-2 text-xs leading-relaxed text-text-60">
          <p class="m-0">
            This is a program that compiles LaTeX on your computer, so you do not need
            to install TeX yourself. It is optional: Manang uses your own TeX
            installation first when it finds one.
          </p>
          <dl class="mt-3 mb-0 grid grid-cols-[6.5rem_1fr] gap-x-3 gap-y-1.5">
            <dt class="text-text-muted">Downloaded from</dt>
            <dd class="selectable m-0 font-mono text-[11px] text-text" data-engine-host>
              {status.url_host}
            </dd>
            <dt class="text-text-muted">Size</dt>
            <dd class="m-0 font-mono text-[11px] text-text" data-engine-size>
              {formatSize(status.size ?? 0)}
            </dd>
            <dt class="text-text-muted">SHA-256</dt>
            <dd
              class="selectable m-0 font-mono text-[11px] break-all text-text"
              data-engine-sha
            >
              {status.sha256}
            </dd>
            <dt class="text-text-muted">Saved in</dt>
            <dd
              class="selectable m-0 font-mono text-[11px] break-all text-text"
              data-engine-path
            >
              {status.stored_in}
            </dd>
          </dl>
          <p class="mt-3 mb-0">
            Manang checks the file against the SHA-256 above before it uses it. If it
            does not match, nothing is installed.
          </p>
          <p class="mt-2 mb-0" data-engine-packages>
            The first time a document uses a LaTeX package, the engine fetches it from
            <span class="font-mono text-[11px] text-text">{status.package_host}</span>
            (the Tectonic project's package server) and keeps it on your computer. Packages
            you have already used work offline. You can turn this off in Settings under LaTeX.
          </p>
        </div>
        <div class="mt-5 flex flex-wrap justify-end gap-2">
          <button type="button" class="mng-btn" onclick={() => void engine.cancel()}
            >Cancel</button
          >
          <button
            type="button"
            class="mng-btn border-accent bg-accent text-accent-ink"
            data-engine-confirm
            onclick={() => void engine.confirm()}
            >Download ({formatSize(status.size ?? 0)})</button
          >
        </div>
      {:else if engine.step === "running"}
        <div id="engine-message" class="mt-3 text-xs text-text-60">
          <div
            class="h-px w-full bg-line"
            role="progressbar"
            aria-label="Download progress"
            aria-valuemin="0"
            aria-valuemax="100"
            aria-valuenow={Math.round(progressFraction(engine.progress) * 100)}
          >
            <div
              class="h-px bg-accent"
              style="width: {progressFraction(engine.progress) * 100}%"
              data-engine-bar
            ></div>
          </div>
          <p class="mt-2 mb-0 font-mono text-[11px]" role="status" data-engine-progress>
            {progressLabel(engine.progress)}
          </p>
        </div>
        <div class="mt-5 flex justify-end gap-2">
          <button
            type="button"
            class="mng-btn"
            data-engine-cancel
            onclick={() => void engine.cancel()}>Cancel</button
          >
        </div>
      {:else if engine.step === "done"}
        <p id="engine-message" class="mt-2 mb-0 text-xs leading-relaxed text-text-60">
          Manang will use it to compile LaTeX whenever no TeX installation is found. The
          document is compiling now.
        </p>
        <div class="mt-5 flex justify-end gap-2">
          <button
            type="button"
            class="mng-btn border-accent bg-accent text-accent-ink"
            data-engine-close
            onclick={() => engine.close()}>Done</button
          >
        </div>
      {:else}
        <p
          id="engine-message"
          class="mt-2 mb-0 text-xs leading-relaxed text-text-60"
          data-engine-error
        >
          {engine.error}
        </p>
        <div class="mt-5 flex justify-end gap-2">
          <button type="button" class="mng-btn" onclick={() => engine.close()}
            >Close</button
          >
          <button
            type="button"
            class="mng-btn border-accent bg-accent text-accent-ink"
            data-engine-confirm
            onclick={() => void engine.confirm()}>Try again</button
          >
        </div>
      {/if}
    </div>
  </div>
{/if}
