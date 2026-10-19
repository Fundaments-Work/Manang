<script lang="ts">
  import PluginIcon from "$lib/components/plugins/PluginIcon.svelte";
  import { formatShortcut } from "$lib/commands/shortcut";
  import { keymap } from "$lib/commands/keymap.svelte";
  import { errorMessage, kernel, reportError } from "$lib/kernel";
  import type { AgentTool } from "$lib/kernel/client";
  import { markdownApi } from "$lib/kernel/api";
  import {
    marketApi,
    type Contributions,
    type PluginDetail,
    type RegistryVersion,
  } from "$lib/kernel/marketApi";
  import { openExternal } from "$lib/kernel/native";
  import { market } from "$lib/state/market.svelte";
  import { plugins } from "$lib/state/plugins.svelte";
  import { settingsPage } from "$lib/state/settingsPage.svelte";
  import { detectPlatform } from "$lib/util/keyboard";
  import {
    NO_PERMISSIONS,
    browseProblem,
    errorText,
    formatCount,
    formatSize,
    isNewer,
    isoDate,
    permissionItems,
    publishedDate,
  } from "$lib/workbench/market";
  import {
    accessDifference,
    canToggle,
    canUninstall,
    disabledReason,
    installedDate,
    stateLabel,
    tierLabel,
  } from "$lib/workbench/pluginManager";

  /**
   * A plugin's page, opened as a tab in the editor area like an extension's
   * page in VS Code. It works for an installed plugin (what the kernel knows)
   * and for a registry plugin that is not installed yet (what the signed index
   * says), and merges both when it is both. The header holds the identity and
   * the actions; below, tabs: Details (the readme), Features (what it adds),
   * Permissions (what it may do, in plain words), Changelog and Versions; a
   * facts column sits beside them, or under the header in a narrow window.
   */

  interface Props {
    id: string;
  }

  const { id }: Props = $props();

  type PageTab = "details" | "features" | "permissions" | "changelog" | "versions";
  const PAGE_TABS: readonly { readonly id: PageTab; readonly label: string }[] = [
    { id: "details", label: "Details" },
    { id: "features", label: "Features" },
    { id: "permissions", label: "Permissions" },
    { id: "changelog", label: "Changelog" },
    { id: "versions", label: "Versions" },
  ];

  const platform = detectPlatform();
  const api = marketApi(kernel);

  let tab = $state<PageTab>("details");
  let detail = $state.raw<PluginDetail | null>(null);
  let detailState = $state<"loading" | "ready" | "unavailable">("loading");
  let detailMessage = $state("");
  let readmeHtml = $state("");
  let readmeFailed = $state(false);
  let contributions = $state.raw<Contributions | null>(null);
  let tools = $state.raw<readonly AgentTool[]>([]);

  const installed = $derived(plugins.get(id));
  const update = $derived(market.updateFor(id));
  const working = $derived(market.working.includes(id));
  const busy = $derived(working || plugins.busy.includes(id));
  const progress = $derived(market.progress[id]);

  const name = $derived(detail?.plugin.name ?? installed?.name ?? id);
  const author = $derived(
    detail?.plugin.author ??
      (installed?.tier === "native" ? "Manang" : installed?.publisher) ??
      null,
  );
  const description = $derived(
    detail?.plugin.description ?? installed?.description ?? "",
  );
  const installedVersion = $derived(
    installed === undefined ? null : (installed.running_version ?? installed.version),
  );
  const latestVersion = $derived(
    detail?.versions.find((v) => v.version === detail?.plugin.latest),
  );
  const shownVersion = $derived(installedVersion ?? detail?.plugin.latest ?? "");
  const withdrawn = $derived(
    detail !== null &&
      installedVersion !== null &&
      detail.versions.some((v) => v.version === installedVersion && v.yanked),
  );
  const history = $derived([...(detail?.versions ?? [])].reverse());

  /** What the Permissions tab lists: the installed plugin's ask, else the registry's. */
  const requested = $derived(
    installed !== undefined
      ? installed.permissions
      : (detail?.permissions ?? { filesystem: [], network: [], buffer: [] }),
  );
  const grantedSet = $derived(
    new Set(permissionItems(installed?.granted ?? requested).map((i) => i.text)),
  );
  const items = $derived(
    permissionItems(
      requested,
      installed?.update_pending === true
        ? installed.new_permissions
        : update?.permissions_grew === true
          ? update.new_permissions
          : undefined,
    ),
  );
  const shortcutOf = (command: string): string => {
    const first = keymap.shortcuts(command)[0];
    return first === undefined ? "" : formatShortcut(first, platform);
  };

  // Load the registry's side of the page whenever the plugin changes.
  $effect(() => {
    const wanted = id;
    detail = null;
    detailState = "loading";
    detailMessage = "";
    readmeHtml = "";
    readmeFailed = false;
    let live = true;
    void api
      .detail(wanted)
      .then(async (d) => {
        if (!live) return;
        detail = d;
        detailState = "ready";
        if (d.readme !== null && d.readme.trim() !== "") {
          try {
            const rendered = await markdownApi(kernel).renderHtml(d.readme);
            if (live) readmeHtml = rendered.html;
          } catch (e) {
            reportError("a plugin readme could not be shown", e);
            if (live) readmeFailed = true;
          }
        }
      })
      .catch((e: unknown) => {
        if (!live) return;
        detailState = "unavailable";
        detailMessage = errorMessage(e);
      });
    return () => {
      live = false;
    };
  });

  // What an installed plugin adds beyond commands.
  $effect(() => {
    const wanted = id;
    if (installed === undefined) return;
    let live = true;
    void api
      .contributions()
      .then((c) => {
        if (live) contributions = c;
      })
      .catch(() => undefined);
    void (kernel.agentTools?.() ?? Promise.resolve({ tools: [], problems: [] }))
      .then((list) => {
        if (live) tools = list.tools.filter((t) => t.plugin === wanted);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  });

  $effect(() => {
    if (!plugins.loaded) void plugins.refresh();
  });

  const features = $derived({
    adapters: (contributions?.format_adapters ?? []).filter((a) => a.plugin === id),
    editors: (contributions?.editors ?? []).filter((e) => e.plugin === id),
    toolbars: (contributions?.toolbars ?? []).filter((t) => t.plugin === id),
  });

  function onTabKeys(event: KeyboardEvent): void {
    const at = PAGE_TABS.findIndex((t) => t.id === tab);
    const next =
      event.key === "ArrowRight"
        ? PAGE_TABS[(at + 1) % PAGE_TABS.length]
        : event.key === "ArrowLeft"
          ? PAGE_TABS[(at + PAGE_TABS.length - 1) % PAGE_TABS.length]
          : event.key === "Home"
            ? PAGE_TABS[0]
            : event.key === "End"
              ? PAGE_TABS[PAGE_TABS.length - 1]
              : undefined;
    if (next === undefined) return;
    event.preventDefault();
    tab = next.id;
    (event.currentTarget as HTMLElement)
      .querySelector<HTMLElement>(`[data-page-tab="${next.id}"]`)
      ?.focus();
  }

  function install(version: RegistryVersion | undefined, pin: boolean): void {
    if (detail === null || version === undefined) return;
    const running = installedVersion;
    void market.install(id, {
      name,
      version: version.version,
      description,
      author: author ?? "Unknown",
      permissions: version.permissions,
      pinVersion: pin,
      ...(running === null
        ? {}
        : {
            update: {
              running,
              added:
                update?.latest === version.version
                  ? update.new_permissions
                  : accessDifference(
                      version.permissions,
                      installed?.granted ?? version.permissions,
                    ),
            },
          }),
    });
  }

  function open(url: string): void {
    void openExternal(url).catch((e: unknown) =>
      reportError("a link could not be opened", e),
    );
  }

  const repoLabel = $derived(detail?.repo.replace(/^https:\/\//, "") ?? "");
  const canInstallLatest = $derived(
    installed === undefined &&
      latestVersion !== undefined &&
      !latestVersion.yanked &&
      detail?.plugin.compatible !== false,
  );
</script>

<section
  class="@container flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto bg-bg"
  aria-label="Plugin {name}"
  data-plugin-page={id}
>
  <header class="border-b border-line px-6 pt-6 pb-4">
    <div class="flex flex-wrap items-start gap-4">
      <PluginIcon {name} size={64} />
      <div class="min-w-0 flex-1" style="min-width: 14rem">
        <h1
          class="m-0 flex flex-wrap items-baseline gap-x-3 text-[20px] font-light text-text"
          style="letter-spacing: var(--tracking-headline)"
          data-page-title
        >
          {name}
          {#if shownVersion !== ""}
            <span class="font-mono text-xs text-text-muted" data-page-version
              >{shownVersion}</span
            >
          {/if}
        </h1>
        <p class="m-0 mt-1 flex flex-wrap items-center gap-x-2 text-xs text-text-60">
          {#if author !== null}<span data-page-author>{author}</span>{/if}
          {#if installed !== undefined}
            <span class="text-text-muted" aria-hidden="true">·</span>
            <span data-page-state>{stateLabel(installed)}</span>
          {/if}
        </p>
        {#if description !== ""}
          <p class="m-0 mt-2 max-w-[60ch] text-[13px] leading-relaxed text-text">
            {description}
          </p>
        {/if}
        {#if detail !== null && detail.plugin.tags.length > 0}
          <p class="m-0 mt-2 flex flex-wrap gap-1" aria-label="Tags">
            {#each detail.plugin.tags as tag (tag)}
              <span class="border border-line px-1.5 py-px text-[11px] text-text-60"
                >{tag}</span
              >
            {/each}
          </p>
        {/if}
      </div>

      <div class="flex flex-wrap items-center gap-2" data-page-actions>
        {#if installed !== undefined && installed.status === "needs_approval"}
          <button
            type="button"
            class="inset-focus h-7 bg-accent px-3 text-xs text-accent-ink disabled:opacity-50"
            disabled={busy}
            data-action="review"
            onclick={() => void plugins.review(installed)}>Review access</button
          >
        {:else if installed !== undefined && update !== undefined && update.compatible}
          <button
            type="button"
            class="inset-focus h-7 bg-accent px-3 text-xs text-accent-ink disabled:opacity-50"
            disabled={busy}
            data-action="update"
            onclick={() =>
              install(
                detail?.versions.find((v) => v.version === update.latest),
                false,
              )}>{working ? "Updating" : `Update to ${update.latest}`}</button
          >
        {:else if installed === undefined && canInstallLatest}
          <button
            type="button"
            class="inset-focus h-7 bg-accent px-3 text-xs text-accent-ink disabled:opacity-50"
            disabled={busy}
            data-action="install"
            onclick={() => install(latestVersion, false)}
            >{working ? "Installing" : "Install"}</button
          >
        {/if}
        {#if installed !== undefined && canToggle(installed)}
          <button
            type="button"
            class="mng-btn text-xs"
            disabled={busy || installed.disabled_by === "project"}
            data-action="toggle"
            onclick={() => void plugins.toggle(installed)}
            >{installed.enabled ? "Disable" : "Enable"}</button
          >
        {/if}
        {#if installed !== undefined && canUninstall(installed)}
          <button
            type="button"
            class="mng-btn text-xs"
            disabled={busy}
            data-action="uninstall"
            onclick={() => void plugins.uninstall(installed)}>Uninstall</button
          >
        {/if}
      </div>
    </div>

    {#if progress !== undefined && progress.total > 0}
      <p
        class="m-0 mt-3 font-mono text-[11px] text-text-60"
        role="status"
        data-progress
      >
        {progress.phase === "downloading"
          ? `Downloading ${Math.round((progress.received / progress.total) * 100)}%`
          : progress.phase === "verifying"
            ? "Checking the download"
            : progress.phase === "installing"
              ? "Installing"
              : "Done"}
      </p>
    {/if}
    {#if installed !== undefined}
      {#if installed.core}
        <p class="m-0 mt-3 text-[11px] text-text-muted" data-page-note>
          Required by Manang: it cannot be disabled or removed.
        </p>
      {:else if installed.tier === "native"}
        <p class="m-0 mt-3 text-[11px] text-text-muted" data-page-note>
          Built in: can be switched off but not removed.
        </p>
      {/if}
      {#if installed.status === "disabled"}
        <p class="m-0 mt-3 text-[11px] text-text-muted">{disabledReason(installed)}</p>
      {/if}
      {#if installed.status === "failed" && installed.error !== undefined}
        <p
          class="selectable m-0 mt-3 border-l border-text-60 pl-3 text-[11px] leading-relaxed text-text"
          data-plugin-error
        >
          {installed.error}
        </p>
      {/if}
    {/if}
    {#if withdrawn}
      <p
        class="m-0 mt-3 border-l border-text-60 pl-3 text-[11px] leading-relaxed text-text"
        data-withdrawn
      >
        The registry withdrew version {installedVersion}. It keeps running, but consider
        updating or uninstalling it.
      </p>
    {/if}
    {#if update !== undefined && update.permissions_grew}
      <p class="m-0 mt-3 text-[11px] leading-relaxed text-text" data-permissions-grew>
        Version {update.latest} asks for access it did not have. You will be asked before
        it installs.
      </p>
    {/if}
    {#if detail !== null && detail.plugin.compatible === false}
      <p class="m-0 mt-3 text-[11px] text-text-60">
        The latest version needs a newer Manang than this one.
      </p>
    {/if}
  </header>

  <div class="flex min-h-0 flex-1 flex-col @2xl:flex-row-reverse">
    <aside
      class="shrink-0 border-b border-line px-6 py-4 @2xl:w-64 @2xl:border-b-0 @2xl:border-l"
      aria-label="Facts"
      data-facts
    >
      <dl class="m-0 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-[11px]">
        <dt class="text-text-muted">Version</dt>
        <dd class="m-0 font-mono">{shownVersion || "—"}</dd>
        {#if author !== null}
          <dt class="text-text-muted">Publisher</dt>
          <dd class="m-0 text-text-60">{author}</dd>
        {/if}
        {#if detail !== null}
          <dt class="text-text-muted">Repository</dt>
          <dd class="m-0 min-w-0">
            <button
              type="button"
              class="inset-focus max-w-full truncate text-left text-text underline decoration-line underline-offset-2 hover:decoration-text"
              title={detail.repo}
              data-repo-link
              onclick={() => open(detail?.repo ?? "")}>{repoLabel}</button
            >
          </dd>
          {#if detail.homepage !== null}
            <dt class="text-text-muted">Homepage</dt>
            <dd class="m-0 min-w-0">
              <button
                type="button"
                class="inset-focus max-w-full truncate text-left text-text underline decoration-line underline-offset-2 hover:decoration-text"
                onclick={() => open(detail?.homepage ?? "")}
                >{detail.homepage.replace(/^https:\/\//, "")}</button
              >
            </dd>
          {/if}
          <dt class="text-text-muted">Licence</dt>
          <dd class="m-0 text-text-60">{detail.plugin.license}</dd>
          <dt class="text-text-muted">Size</dt>
          <dd class="m-0 text-text-60">{formatSize(detail.plugin.size)}</dd>
          <dt class="text-text-muted">Last updated</dt>
          <dd class="m-0 font-mono">{publishedDate(detail.plugin.published)}</dd>
          {#if detail.plugin.downloads !== null}
            <dt class="text-text-muted">Downloads</dt>
            <dd class="m-0 text-text-60">{formatCount(detail.plugin.downloads)}</dd>
          {/if}
        {/if}
        <dt class="text-text-muted">Id</dt>
        <dd class="selectable m-0 font-mono">{id}</dd>
        {#if installed !== undefined}
          <dt class="text-text-muted">Type</dt>
          <dd class="m-0 text-text-60">{tierLabel(installed)}</dd>
          {#if installed.installed_at !== undefined}
            <dt class="text-text-muted">Installed</dt>
            <dd class="m-0 font-mono">{installedDate(installed.installed_at)}</dd>
          {/if}
        {:else if detail !== null}
          <dt class="text-text-muted">Type</dt>
          <dd class="m-0 text-text-60">Sandboxed</dd>
        {/if}
        {#if latestVersion?.min_app !== undefined && latestVersion.min_app !== null}
          <dt class="text-text-muted">Needs Manang</dt>
          <dd class="m-0 font-mono">{latestVersion.min_app}</dd>
        {/if}
      </dl>
    </aside>

    <div class="min-w-0 flex-1">
      <div
        class="flex gap-5 border-b border-line px-6"
        role="tablist"
        aria-label="Plugin sections"
        tabindex="-1"
        onkeydown={onTabKeys}
      >
        {#each PAGE_TABS as t (t.id)}
          <button
            type="button"
            role="tab"
            id="page-tab-{t.id}"
            aria-selected={tab === t.id}
            aria-controls="page-panel"
            tabindex={tab === t.id ? 0 : -1}
            data-page-tab={t.id}
            class="inset-focus -mb-px h-9 border-b text-xs {tab === t.id
              ? 'border-text text-text'
              : 'border-transparent text-text-60 hover:text-text'}"
            onclick={() => (tab = t.id)}>{t.label}</button
          >
        {/each}
      </div>

      <div
        id="page-panel"
        role="tabpanel"
        aria-labelledby="page-tab-{tab}"
        class="px-6 py-5 text-xs leading-relaxed text-text-60"
      >
        {#if tab === "details"}
          {#if readmeHtml !== ""}
            <article
              class="mng-paper-prose block max-w-[760px] border border-line bg-paper px-8 py-7 text-paper-ink"
              aria-label="Readme"
              data-readme
            >
              <!-- The HTML is sanitised in Rust (`markdown.render-html`): raw HTML
                   is filtered to an allow-list and URLs are checked there. -->
              <!-- eslint-disable-next-line svelte/no-at-html-tags -->
              {@html readmeHtml}
            </article>
          {:else if detailState === "loading" && installed === undefined}
            <p class="m-0" role="status">Loading…</p>
          {:else}
            <p class="m-0 max-w-[60ch] text-text" data-no-readme>
              {description !== "" ? description : "This plugin has no description."}
            </p>
            <p class="m-0 mt-2 text-text-muted">
              {readmeFailed
                ? "Its readme could not be shown."
                : detail?.readme_error != null
                  ? `Its readme could not be loaded: ${errorText(detail.readme_error)}`
                  : "It has no readme."}
            </p>
          {/if}
          {#if detailState === "unavailable" && installed === undefined}
            <div class="mt-3" role="alert" data-detail-problem>
              <p class="m-0 text-text">
                {browseProblem(detailMessage) === "off"
                  ? "Browsing plugins is turned off."
                  : "This plugin's page could not be loaded."}
              </p>
              <p class="m-0 mt-1">{errorText(detailMessage)}</p>
            </div>
          {/if}
        {:else if tab === "features"}
          {#if installed === undefined}
            <p class="m-0 max-w-[60ch]" data-features-later>
              What a plugin adds (its commands, settings and tools) is shown once it is
              installed.
            </p>
          {:else}
            <section aria-label="Commands" data-features-commands>
              <h2 class="m-0 mb-2 text-xs font-normal text-text">
                Commands <span class="font-mono text-[10px] text-text-muted"
                  >{installed.commands.length}</span
                >
              </h2>
              {#if installed.commands.length === 0}
                <p class="m-0">It adds no commands.</p>
              {:else}
                <ul class="m-0 list-none p-0">
                  {#each installed.commands as command (command.id)}
                    <li
                      class="flex items-baseline gap-3 border-b border-line-subtle py-1.5"
                    >
                      <span class="min-w-0 flex-1">
                        <span class="block text-text">{command.title}</span>
                        <span
                          class="selectable block font-mono text-[10px] text-text-muted"
                          >{command.id}</span
                        >
                      </span>
                      {#if shortcutOf(command.id) !== ""}
                        <span class="mng-kbd">{shortcutOf(command.id)}</span>
                      {/if}
                    </li>
                  {/each}
                </ul>
              {/if}
            </section>

            {#if installed.settings.length > 0}
              <section class="mt-5" aria-label="Settings" data-features-settings>
                <h2 class="m-0 mb-2 text-xs font-normal text-text">
                  Settings <span class="font-mono text-[10px] text-text-muted"
                    >{installed.settings.length}</span
                  >
                </h2>
                <ul class="m-0 list-none p-0">
                  {#each installed.settings as setting (setting)}
                    <li class="selectable py-0.5 font-mono text-[11px]">{setting}</li>
                  {/each}
                </ul>
                <button
                  type="button"
                  class="mng-btn mt-2 text-xs"
                  data-action="settings"
                  onclick={() => settingsPage.show(installed.name)}
                  >Open settings</button
                >
              </section>
            {/if}

            {#if tools.length > 0}
              <section class="mt-5" aria-label="Agent tools" data-features-tools>
                <h2 class="m-0 mb-2 text-xs font-normal text-text">
                  Agent tools <span class="font-mono text-[10px] text-text-muted"
                    >{tools.length}</span
                  >
                </h2>
                <ul class="m-0 list-none p-0">
                  {#each tools as tool (tool.name)}
                    <li class="border-b border-line-subtle py-1.5">
                      <span class="block text-text">{tool.title}</span>
                      <span class="block">{tool.description}</span>
                      <span
                        class="selectable block font-mono text-[10px] text-text-muted"
                        >{tool.name}</span
                      >
                    </li>
                  {/each}
                </ul>
              </section>
            {/if}

            {#if features.adapters.length > 0 || features.editors.length > 0 || features.toolbars.length > 0}
              <section
                class="mt-5"
                aria-label="Formats and editors"
                data-features-formats
              >
                <h2 class="m-0 mb-2 text-xs font-normal text-text">
                  Formats and editors
                </h2>
                <ul class="m-0 list-none p-0">
                  {#each features.adapters as a (a.id)}
                    <li class="py-0.5">
                      <span class="text-text">{a.title === "" ? a.id : a.title}</span>
                      <span class="text-text-muted"
                        >format{a.extensions.length > 0
                          ? `, .${a.extensions.join(", .")}`
                          : ""}</span
                      >
                    </li>
                  {/each}
                  {#each features.editors as e (e.id)}
                    <li class="py-0.5">
                      <span class="text-text">{e.title === "" ? e.id : e.title}</span>
                      <span class="text-text-muted">editor</span>
                    </li>
                  {/each}
                  {#each features.toolbars as t (t.editor)}
                    <li class="py-0.5">
                      <span class="text-text">Toolbar for {t.editor}</span>
                      <span class="text-text-muted"
                        >{t.items.length}
                        {t.items.length === 1 ? "button" : "buttons"}</span
                      >
                    </li>
                  {/each}
                </ul>
              </section>
            {/if}
          {/if}
        {:else if tab === "permissions"}
          <p class="m-0 max-w-[62ch]">
            A plugin runs in a sandbox and can only do what is listed here. Everything
            else on your computer is out of its reach. You approve this list before it
            installs, and again if an update asks for more.
          </p>
          {#if detailState === "loading" && installed === undefined}
            <p class="m-0 mt-3" role="status">Loading…</p>
          {:else if items.length === 0}
            <p class="m-0 mt-3 text-text" data-permissions-none>{NO_PERMISSIONS}</p>
          {:else}
            <ul class="m-0 mt-3 list-none p-0" data-permissions>
              {#each items as item (item.text)}
                <li class="border-b border-line-subtle py-2" data-permission>
                  <span class="flex flex-wrap items-baseline gap-2">
                    <span class="text-[13px] text-text">{item.text}</span>
                    {#if item.isNew}
                      <span
                        class="border border-text-60 px-1 font-mono text-[10px] text-text"
                        >New</span
                      >
                    {/if}
                    {#if installed !== undefined && installed.status !== "needs_approval"}
                      <span class="font-mono text-[10px] text-text-muted"
                        >{grantedSet.has(item.text) ? "Allowed" : "Not approved"}</span
                      >
                    {/if}
                  </span>
                  <span class="mt-0.5 block">{item.meaning}</span>
                </li>
              {/each}
            </ul>
          {/if}
        {:else if tab === "changelog"}
          {#if history.length === 0}
            <p class="m-0" data-no-changelog>
              {installed !== undefined && detail === null
                ? "No changelog is available for a plugin that is not in the registry."
                : "No changelog yet."}
            </p>
          {:else}
            <ol class="m-0 list-none p-0" data-changelog>
              {#each history as v (v.version)}
                <li class="border-b border-line-subtle py-3">
                  <h2
                    class="m-0 flex items-baseline gap-3 text-xs font-normal text-text"
                  >
                    <span class="font-mono">{v.version}</span>
                    <span class="font-mono text-[10px] text-text-muted"
                      >{publishedDate(v.published)}</span
                    >
                  </h2>
                  <p class="m-0 mt-1 max-w-[62ch]">
                    {v.changelog === "" ? "No notes for this version." : v.changelog}
                  </p>
                </li>
              {/each}
            </ol>
          {/if}
        {:else if history.length === 0}
          <p class="m-0" data-no-versions>
            {installed === undefined
              ? "Loading…"
              : `Installed version ${installedVersion ?? ""}. The registry does not list this plugin, so its history is not available.`}
          </p>
        {:else}
          <ol class="m-0 list-none p-0" data-versions>
            {#each history as v (v.version)}
              {@const isInstalled = v.version === installedVersion}
              {@const offer =
                !v.yanked &&
                v.compatible &&
                !isInstalled &&
                (installedVersion === null || isNewer(v.version, installedVersion))}
              <li
                class="flex flex-wrap items-center gap-x-4 gap-y-1 border-b border-line-subtle py-2"
                data-version={v.version}
              >
                <span class="w-20 font-mono text-xs text-text">{v.version}</span>
                <span class="w-24 font-mono text-[10px] text-text-muted"
                  >{publishedDate(v.published)}</span
                >
                <span class="w-16 text-[11px]">{formatSize(v.size)}</span>
                <span class="min-w-0 flex-1 text-[11px]">
                  {#if isInstalled}<span class="text-text" data-version-installed
                      >Installed</span
                    >{/if}
                  {#if v.version === detail?.plugin.latest}<span class="ml-2"
                      >Latest</span
                    >{/if}
                  {#if v.yanked}<span class="ml-2 text-text" data-version-withdrawn
                      >Withdrawn</span
                    >{/if}
                  {#if !v.compatible && v.min_app !== null}<span class="ml-2"
                      >Needs Manang {v.min_app}</span
                    >{/if}
                </span>
                {#if offer}
                  <button
                    type="button"
                    class="mng-btn text-[11px]"
                    disabled={busy}
                    data-install-version={v.version}
                    onclick={() => install(v, true)}>Install this version</button
                  >
                {/if}
              </li>
            {/each}
          </ol>
          {#if installedVersion !== null}
            <p class="m-0 mt-3 text-[11px] text-text-muted">
              To go back to an older version, uninstall the plugin and install that
              version.
            </p>
          {/if}
        {/if}
        {#if detail !== null && detail.freshness.state === "offline"}
          <p class="m-0 mt-4 text-[11px] text-text-muted" data-offline-note>
            Showing the registry's list from {isoDate(detail.freshness.fetched_at)}.
          </p>
        {/if}
      </div>
    </div>
  </div>
</section>
