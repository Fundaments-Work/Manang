<script lang="ts">
  import scowlLicenseText from "../../../assets/licenses/LICENSE-SCOWL-en_US.txt?raw";

  /**
   * About & Licences View for Manang:
   * Displays application version, git commit, core architectural components,
   * open-source licenses, and the SCOWL word-list license text from
   * crates/manang-writing/dict/ (served from static assets).
   */

  const APP_VERSION = "0.1.0";
  const GIT_COMMIT = "7df53d9";

  let showFullScowl = $state(false);
</script>

<div class="p-4 space-y-6 text-xs">
  <!-- App Identity Header -->
  <div class="border-b border-line pb-4">
    <div class="flex items-baseline gap-2">
      <h3 class="m-0 text-base font-medium text-text tracking-tight">Manang</h3>
      <span class="font-mono text-[11px] text-accent">v{APP_VERSION}</span>
    </div>
    <p class="m-0 mt-1 text-[11px] text-text-60 leading-relaxed">
      The High-Performance Scientific Authoring &amp; Illustration Suite.
    </p>
  </div>

  <!-- Build & Runtime Metadata -->
  <div>
    <div class="font-medium text-text mb-2">Build Details</div>
    <div class="border border-line bg-surface-hi/30 divide-y divide-line font-mono text-[11px]">
      <div class="flex justify-between px-3 py-1.5">
        <span class="text-text-muted">Version</span>
        <span class="text-text">{APP_VERSION}</span>
      </div>
      <div class="flex justify-between px-3 py-1.5">
        <span class="text-text-muted">Commit</span>
        <span class="text-accent">{GIT_COMMIT}</span>
      </div>
      <div class="flex justify-between px-3 py-1.5">
        <span class="text-text-muted">Kernel</span>
        <span class="text-text">Tauri v2 + Rust Plugin Host</span>
      </div>
      <div class="flex justify-between px-3 py-1.5">
        <span class="text-text-muted">Frontend</span>
        <span class="text-text">Svelte 5 Runes + CodeMirror 6</span>
      </div>
      <div class="flex justify-between px-3 py-1.5">
        <span class="text-text-muted">Design System</span>
        <span class="text-text">Fundaments.work</span>
      </div>
    </div>
  </div>

  <!-- Open Source Licences Overview -->
  <div>
    <div class="font-medium text-text mb-1">Open-Source Licences</div>
    <p class="m-0 text-[11px] text-text-60 leading-relaxed mb-3">
      Manang is built on open-source foundations. Key technologies include:
    </p>
    <div class="border border-line divide-y divide-line text-[11px]">
      <div class="flex items-center justify-between px-3 py-1.5">
        <span class="text-text">CodeMirror 6, Lezer &amp; Legacy Modes</span>
        <span class="font-mono text-[10px] text-text-muted">MIT</span>
      </div>
      <div class="flex items-center justify-between px-3 py-1.5">
        <span class="text-text">Svelte 5 &amp; Vite</span>
        <span class="font-mono text-[10px] text-text-muted">MIT</span>
      </div>
      <div class="flex items-center justify-between px-3 py-1.5">
        <span class="text-text">Typst Compiler &amp; Typesetting Engine</span>
        <span class="font-mono text-[10px] text-text-muted">Apache-2.0</span>
      </div>
      <div class="flex items-center justify-between px-3 py-1.5">
        <span class="text-text">Wasmtime Sandboxed Plugin Runtime</span>
        <span class="font-mono text-[10px] text-text-muted">Apache-2.0 / LLVM</span>
      </div>
      <div class="flex items-center justify-between px-3 py-1.5">
        <span class="text-text">DM Sans &amp; DM Mono Typefaces</span>
        <span class="font-mono text-[10px] text-text-muted">OFL-1.1</span>
      </div>
      <div class="flex items-center justify-between px-3 py-1.5">
        <span class="text-text">Perfect Freehand Ink Geometry</span>
        <span class="font-mono text-[10px] text-text-muted">MIT</span>
      </div>
    </div>
  </div>

  <!-- SCOWL Word List License (Mandatory) -->
  <div class="border border-line bg-surface-hi/20 p-3 space-y-2">
    <div class="flex items-center justify-between">
      <div>
        <div class="font-medium text-text">SCOWL Word Lists &amp; Dictionaries</div>
        <div class="text-[11px] text-text-muted">Hunspell en_US &amp; en_GB spellchecking dictionaries</div>
      </div>
      <button
        type="button"
        class="mng-btn px-2 py-0.5 text-[11px]"
        onclick={() => (showFullScowl = !showFullScowl)}
      >
        {showFullScowl ? "Hide license" : "View license"}
      </button>
    </div>

    <p class="m-0 text-[11px] text-text-60 leading-relaxed">
      Copyright &copy; 2000&ndash;2018 Kevin Atkinson and contributors. Included under the SCOWL / Ispell BSD-style license.
    </p>

    {#if showFullScowl}
      <div class="mt-2 border border-line bg-surface p-2">
        <div class="flex items-center justify-between pb-1.5 mb-1.5 border-b border-line text-[10px] text-text-muted font-mono">
          <span>crates/manang-writing/dict/LICENSE-SCOWL-en_US.txt</span>
          <span>SCOWL Size 60</span>
        </div>
        <pre class="m-0 max-h-56 overflow-y-auto whitespace-pre-wrap font-mono text-[10px] text-text-60 leading-relaxed select-all">
{scowlLicenseText}
        </pre>
      </div>
    {/if}
  </div>
</div>
