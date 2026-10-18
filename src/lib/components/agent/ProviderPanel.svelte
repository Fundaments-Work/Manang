<script lang="ts">
  import { agent } from "$lib/state/agent.svelte";
  import { settings } from "$lib/state/settings.svelte";

  /**
   * Provider and key, inside the agent plugin's Settings section (after its ordinary rows).
   * The key is write-only: it goes to the system keychain and is never shown again. The panel
   * says where it is held, tests the connection, and records or withdraws consent.
   */

  let key = $state("");
  let saving = $state(false);
  let keyError = $state("");
  let testing = $state(false);
  let result = $state<{ ok: boolean; text: string } | null>(null);

  const provider = $derived(String(settings.value("agent.provider") ?? "anthropic"));
  const status = $derived(agent.status);
  const preset = $derived(status?.presets.find((p) => p.id === provider));
  const keyInfo = $derived(status?.key ?? null);

  // Keep the status in step with the provider chosen in the rows above.
  $effect(() => {
    void provider;
    void settings.value("agent.model");
    void settings.value("agent.base-url");
    void settings.value("agent.offline");
    void agent.refreshStatus();
    result = null;
  });

  const keyLine = $derived(
    status === null
      ? ""
      : !status.key_required && keyInfo?.present !== true
        ? "This provider needs no key."
        : keyInfo?.present !== true
          ? "No key saved."
          : keyInfo.saved
            ? keyInfo.store === "keychain"
              ? "Key saved in your keychain."
              : (keyInfo.notice ?? "Key saved by the system.")
            : "Not saved: stored for this session only.",
  );

  async function save(): Promise<void> {
    if (key.trim() === "") return;
    saving = true;
    keyError = await agent.saveKey(provider, key);
    saving = false;
    if (keyError === "") key = "";
  }

  async function test(): Promise<void> {
    testing = true;
    result = null;
    const out = await agent.testConnection();
    testing = false;
    result = out.ok
      ? { ok: true, text: `Connected. Model ${out.model ?? ""} answered.` }
      : { ok: false, text: out.message ?? "The connection failed." };
  }

  async function pickModel(model: string): Promise<void> {
    await settings.set("agent.model", model, "user");
    void agent.refreshStatus();
  }
</script>

<div class="mt-4 border-t border-line pt-4" data-group="agent-provider">
  <h4 class="m-0 text-[13px] font-normal text-text">Provider and key</h4>
  <p class="m-0 mt-1 max-w-[60ch] text-xs leading-relaxed text-text-60">
    Choose the provider, model and address in the rows above. The key is kept in your
    system keychain and is never shown again. Nothing is sent until you press Send in
    the Agent view.
  </p>

  {#if preset !== undefined && preset.models.length > 0}
    <p class="m-0 mt-3 text-xs text-text-60">
      Models:
      {#each preset.models as model, i (model)}
        <button
          type="button"
          class="mng-lime-hover inset-focus font-mono text-[11px] text-text"
          onclick={() => void pickModel(model)}>{model}</button
        >{i < preset.models.length - 1 ? ", " : ""}
      {/each}
    </p>
  {/if}

  <div class="mt-3 grid max-w-[34rem] gap-1.5">
    <label for="agent-key" class="text-[13px] text-text"
      >API key{preset !== undefined ? ` for ${preset.name}` : ""}</label
    >
    <div class="flex gap-2">
      <input
        id="agent-key"
        type="password"
        class="mng-field h-8 flex-1 font-mono"
        autocomplete="off"
        autocapitalize="off"
        spellcheck="false"
        placeholder={keyInfo?.present === true
          ? "A key is saved; paste a new one to replace it"
          : "Paste the key"}
        bind:value={key}
        onkeydown={(e) => {
          if (e.key === "Enter") void save();
        }}
        data-agent-key
      />
      <button
        type="button"
        class="mng-btn h-8"
        disabled={saving || key.trim() === ""}
        data-agent-key-save
        onclick={() => void save()}>Save key</button
      >
      <button
        type="button"
        class="mng-btn h-8"
        disabled={keyInfo?.present !== true}
        onclick={() => void agent.clearKey(provider)}>Remove</button
      >
    </div>
    <p class="m-0 text-[11px] text-text-60" role="status" data-agent-key-status>
      {keyError !== "" ? keyError : keyLine}
    </p>
  </div>

  <div class="mt-3 flex flex-wrap items-center gap-3">
    <button
      type="button"
      class="mng-btn h-8"
      disabled={testing}
      data-agent-test
      onclick={() => void test()}>{testing ? "Testing" : "Test connection"}</button
    >
    {#if result !== null}
      <span
        class="text-xs {result.ok ? 'text-text' : 'text-text'}"
        role="status"
        data-agent-test-result>{result.text}</span
      >
    {/if}
  </div>

  <p class="m-0 mt-3 text-[11px] text-text-60" data-agent-consent>
    {#if status?.consent === true}
      You agreed to send data to {status.provider_name}.
      <button
        type="button"
        class="mng-lime-hover inset-focus text-[11px] text-text-60 underline"
        onclick={() => void agent.revokeConsent(provider)}>Withdraw</button
      >
    {:else}
      You will be asked to agree before the first request.
    {/if}
  </p>
  <p class="m-0 mt-1 max-w-[60ch] text-[11px] leading-relaxed text-text-60">
    Costs shown are estimates from published prices and about 3.5 characters per token.
    The provider's bill is the truth.
  </p>
</div>
