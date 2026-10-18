<script lang="ts">
  import { agent } from "$lib/state/agent.svelte";

  /**
   * First use, in three steps: what the agent does, which provider and key it uses (with a
   * connection test), and your consent to send data to that provider. One lime action at a
   * time: "Save and test" while the connection is not proven, then "Agree and start". The same
   * choices live in Settings, Agent. Off, policy and offline states say what to do instead.
   */

  const status = $derived(agent.status);
  const problems = $derived(status?.problems ?? []);
  const off = $derived(status !== null && !status.enabled);
  const policy = $derived(problems.includes("policy"));
  const offlineBlocked = $derived(problems.includes("offline_blocked"));
  const keyNeeded = $derived(status?.key_required === true);
  const keyPresent = $derived(status?.key?.present === true);
  const consentGiven = $derived(status?.consent === true);

  let key = $state("");
  let busy = $state(false);
  let message = $state("");
  let tested = $state(false);

  /** Step 2 is done when no key is missing and the connection test passed. */
  const connected = $derived(
    problems.length > 0 && problems.every((p) => p === "consent_required")
      ? true
      : tested &&
          !problems.includes("key_missing") &&
          !problems.includes("model_missing"),
  );
  const keyOk = $derived(!keyNeeded || keyPresent);
  const step = $derived(connected ? 3 : 2);

  async function saveAndTest(): Promise<void> {
    if (status === null) return;
    busy = true;
    message = "";
    if (key.trim() !== "") {
      const error = await agent.saveKey(status.provider, key);
      if (error !== "") {
        message = error;
        busy = false;
        return;
      }
      key = "";
    } else if (keyNeeded && !keyPresent) {
      message = "Paste your key first.";
      busy = false;
      return;
    }
    const out = await agent.testConnection();
    tested = out.ok;
    message = out.ok
      ? `Connected. ${out.model ?? "The model"} answered.`
      : (out.message ?? "The connection failed. Check the key, balance and address.");
    busy = false;
  }

  async function agree(): Promise<void> {
    if (status === null) return;
    busy = true;
    await agent.grantConsent(status.provider);
    busy = false;
  }

  const row = "border-t border-line px-4 py-3";
  const lime = "mng-btn border-accent bg-accent text-accent-ink";
</script>

<div class="border-b border-line" data-agent-setup-card>
  {#if off}
    <div class="px-4 py-4">
      <h3 class="m-0 text-[13px] font-normal text-text">The AI agent is turned off</h3>
      <p class="m-0 mt-1 text-xs leading-relaxed text-text-60">
        Nothing is sent anywhere while it is off. Turn it on to chat, rewrite text and
        fix problems with your own key.
      </p>
      <div class="mt-3 flex items-center gap-3">
        <button
          type="button"
          class={lime}
          data-agent-turn-on
          onclick={() => void agent.turnOn()}>Turn on</button
        >
        <button
          type="button"
          class="mng-lime-hover inset-focus text-[11px] text-text-60 underline"
          data-agent-setup
          onclick={() => agent.openSetup()}>Open Settings</button
        >
      </div>
    </div>
  {:else if policy || offlineBlocked}
    <div class="px-4 py-4">
      <h3 class="m-0 text-[13px] font-normal text-text">
        {policy ? "This project does not allow this provider" : "Offline mode is on"}
      </h3>
      <p class="m-0 mt-1 text-xs leading-relaxed text-text-60">
        {#if policy}
          The project's settings limit which providers the agent may use. Choose another
          provider, or ask whoever set up the project.
        {:else}
          Offline mode only allows a model running on this computer. Choose a local
          provider such as Ollama, or turn offline mode off in Settings.
        {/if}
      </p>
      <div class="mt-3">
        <button
          type="button"
          class={lime}
          data-agent-setup
          onclick={() => agent.openSetup()}>Open Settings</button
        >
      </div>
    </div>
  {:else}
    <div class="px-4 pt-4 pb-2">
      <h3 class="m-0 text-[13px] font-normal text-text">Set up the agent</h3>
      <p class="m-0 mt-1 text-xs text-text-60">Three steps, about a minute.</p>
    </div>

    <section class={row} aria-labelledby="setup-1">
      <h4 id="setup-1" class="m-0 text-xs font-normal text-text">
        <span class="font-mono text-text-60">1</span> What it does
      </h4>
      <p class="m-0 mt-1 text-xs leading-relaxed text-text-60">
        Rewrite, shorten, translate and explain your text, and fix compile problems. It
        uses your own account with a provider you choose. Every change is a proposal you
        accept or reject.
      </p>
    </section>

    <section class={row} aria-labelledby="setup-2">
      <h4 id="setup-2" class="m-0 text-xs font-normal text-text">
        <span class="font-mono text-text-60">2</span> Choose a provider and add your key
        {#if connected}<span class="text-text-60"> (done)</span>{/if}
      </h4>
      <div class="mt-2 grid gap-2">
        <label class="grid gap-1 text-[11px] text-text-60"
          >Provider
          <select
            class="mng-field h-8"
            value={status?.provider}
            data-agent-provider
            onchange={(e) => {
              tested = false;
              message = "";
              void agent.chooseProvider(e.currentTarget.value);
            }}
          >
            {#each status?.presets ?? [] as p (p.id)}
              <option value={p.id}>{p.name}</option>
            {/each}
          </select>
        </label>
        {#if keyNeeded}
          <label class="grid gap-1 text-[11px] text-text-60"
            >API key
            <input
              type="password"
              class="mng-field h-8 font-mono"
              autocomplete="off"
              autocapitalize="off"
              spellcheck="false"
              placeholder={keyPresent
                ? "A key is saved. Paste a new one to replace it"
                : "Paste the key"}
              bind:value={key}
              onkeydown={(e) => {
                if (e.key === "Enter") void saveAndTest();
              }}
              data-agent-card-key
            />
          </label>
          <p class="m-0 text-[11px] text-text-muted">
            The key stays in your system keychain and is never shown again.
          </p>
        {:else}
          <p class="m-0 text-xs text-text-60">
            {status?.provider_name ?? "This provider"} needs no key.
          </p>
        {/if}
        <div class="flex flex-wrap items-center gap-3">
          <button
            type="button"
            class={connected ? "mng-btn" : lime}
            disabled={busy || (keyNeeded && !keyOk && key.trim() === "")}
            data-agent-card-test
            onclick={() => void saveAndTest()}
            >{busy
              ? "Testing"
              : keyNeeded && key.trim() !== ""
                ? "Save key and test"
                : "Test connection"}</button
          >
          {#if message !== ""}
            <span class="text-xs text-text" role="status" data-agent-card-result
              >{message}</span
            >
          {/if}
        </div>
      </div>
    </section>

    <section class={row} aria-labelledby="setup-3">
      <h4
        id="setup-3"
        class="m-0 text-xs font-normal {step === 3 ? 'text-text' : 'text-text-60'}"
      >
        <span class="font-mono text-text-60">3</span> Agree to share what you send
      </h4>
      <p
        class="selectable m-0 mt-1 text-xs leading-relaxed text-text-60"
        data-agent-consent-text
      >
        {status?.consent_text ?? ""}
      </p>
      <div class="mt-2 flex items-center gap-3">
        {#if consentGiven}
          <span class="text-xs text-text-60">You agreed.</span>
        {:else}
          <button
            type="button"
            class={step === 3 ? lime : "mng-btn"}
            disabled={busy || !connected}
            data-agent-agree
            onclick={() => void agree()}>Agree and start</button
          >
          {#if !connected}
            <span class="text-[11px] text-text-muted">Test the connection first.</span>
          {/if}
        {/if}
      </div>
    </section>

    <p class="m-0 border-t border-line px-4 py-2 text-[11px] text-text-60">
      More choices, such as the model, privacy and cost limits, are in
      <button
        type="button"
        class="mng-lime-hover inset-focus text-[11px] text-text-60 underline"
        data-agent-setup
        onclick={() => agent.openSetup()}>Settings, Agent</button
      >.
    </p>
  {/if}
</div>
