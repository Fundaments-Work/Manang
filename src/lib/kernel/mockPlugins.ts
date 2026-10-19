/**
 * In-memory `plugins` plugin for the browser kernel (`pnpm dev`, tests).
 *
 * Reproduces `crates/manang-plugins`'s command surface and its rules: an
 * install waits for approval, an update asking for more is held beside the
 * running version, core plugins cannot be disabled or removed, built-ins can
 * be switched off but not uninstalled, errors are `[code] message`. Built-ins
 * come from the first-party manifests; a few sample installed plugins (one
 * running, one with an update waiting, one needing approval, one failed, one
 * disabled) give every state something to show. A browser has no file
 * system, so installing takes one of the canned paths below.
 */

import { isNewer } from "$lib/workbench/market";
import {
  EMPTY_ACCESS,
  accessDifference,
  accessIsEmpty,
  accessWithin,
} from "$lib/workbench/pluginManager";

import type { MockPlugin, MockPluginFactory } from "./mock";
import type {
  BrowseResult,
  Contributions,
  Freshness,
  RegistryStatus,
  UpdateCheck,
  UpdateInfo,
} from "./marketApi";
import { MOCK_MANIFESTS } from "./mockManifests";
import { MOCK_REGISTRY, latestOf, toBrowseEntry, toDetail } from "./mockRegistry";
import type {
  PluginAccess,
  PluginInspect,
  PluginList,
  PluginPermissionReport,
  PluginState,
} from "./pluginsApi";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["plugins.list", "List plugins"],
  ["plugins.inspect", "Inspect a plugin archive or folder without installing it"],
  ["plugins.install", "Install a plugin from a folder or .manangplugin archive"],
  ["plugins.approve", "Approve a plugin's permissions and load it"],
  ["plugins.enable", "Enable a plugin"],
  ["plugins.disable", "Disable a plugin"],
  ["plugins.uninstall", "Uninstall a plugin"],
  ["plugins.reload", "Reload a plugin from disk"],
  ["plugins.permissions", "Show what a plugin may access"],
  ["plugins.use-project", "Apply the open project's plugin overrides"],
  ["plugins.configure", "Apply plugin settings"],
  ["plugins.registry-status", "Show whether the plugin registry is available"],
  ["plugins.browse", "Browse the plugin registry"],
  ["plugins.detail", "Show a registry plugin's page"],
  ["plugins.check-updates", "Check for plugin updates"],
  ["plugins.install-from-registry", "Install or update a plugin from the registry"],
  ["plugins.cancel-install", "Cancel a plugin download"],
];

/**
 * How the sample registry behaves, for trying every state of the Browse tab:
 * set `globalThis.__MANANG_MOCK_REGISTRY__` to one of these (default `ok`).
 */
export type MockRegistryMode =
  "ok" | "offline" | "unreachable" | "signature" | "off" | "empty";

function registryMode(): MockRegistryMode {
  return (
    (globalThis as { __MANANG_MOCK_REGISTRY__?: MockRegistryMode })
      .__MANANG_MOCK_REGISTRY__ ?? "ok"
  );
}

/** Built-ins that cannot be disabled or removed (`CORE_PLUGINS` in Rust). */
export const MOCK_CORE_PLUGINS: readonly string[] = [
  "editor",
  "project",
  "settings",
  "keybindings",
  "theme",
  "plugins",
];

/** What the browser's install picker offers: a path, and what is at it. */
export const MOCK_PLUGIN_SOURCES: Readonly<Record<string, PluginInspect>> = {
  "/mock/plugins/word-stats.manangplugin": {
    name: "word-stats",
    version: "0.1.0",
    description: "Reading time and readability statistics for the open document.",
    tier: "wasm",
    publisher: "Example Labs",
    signed: false,
    kind: "archive",
    permissions: { filesystem: [], network: [], buffer: ["read:active"] },
    new_permissions: { filesystem: [], network: [], buffer: ["read:active"] },
    commands: [{ id: "word-stats.show", title: "Show document statistics" }],
    installed_version: null,
  },
};

type AccessInput = { [K in keyof PluginAccess]?: readonly string[] | undefined };

const access = (a: AccessInput): PluginAccess => ({
  filesystem: a.filesystem ?? [],
  network: a.network ?? [],
  buffer: a.buffer ?? [],
});

/** `state` without the optional `keys` (they must be absent, not undefined). */
function omit(
  state: PluginState,
  ...keys: readonly (keyof PluginState)[]
): PluginState {
  const copy: Record<string, unknown> = { ...state };
  for (const key of keys) delete copy[key];
  return copy as unknown as PluginState;
}

function fail(code: string, message: string): { message: string } {
  return { message: `[${code}] ${message}` };
}

function nameArg(args: Record<string, unknown>): string {
  const name = args["name"];
  if (typeof name !== "string" || !/^[a-z][a-z0-9-]*[a-z0-9]$|^[a-z]$/.test(name))
    throw fail("invalid_name", `\`${String(name)}\` is not a valid plugin name`);
  return name;
}

function builtIns(): PluginState[] {
  const out: PluginState[] = [];
  const names = [...Object.keys(MOCK_MANIFESTS), "keybindings", "theme"];
  for (const name of new Set(names)) {
    const manifest = MOCK_MANIFESTS[name];
    const permissions = access({ ...manifest?.permissions });
    const own = name === "plugins" ? COMMANDS : [];
    out.push({
      name,
      version: "0.1.0",
      running_version: "0.1.0",
      description:
        manifest?.description ??
        (name === "keybindings" ? "Keyboard shortcuts" : "Colour themes"),
      tier: "native",
      core: MOCK_CORE_PLUGINS.includes(name),
      permissions,
      granted: permissions,
      new_permissions: EMPTY_ACCESS,
      enabled: true,
      status: "loaded",
      signed: false,
      commands: own.map(([id, title]) => ({ id, title })),
      settings: (manifest?.settings ?? []).map((s) => s.id),
      contexts: manifest?.settingsContexts ?? ["app"],
      update_pending: false,
    });
  }
  return out;
}

function samples(now: number): PluginState[] {
  const day = 86_400;
  const base = {
    tier: "wasm" as const,
    core: false,
    signed: false,
    contexts: ["app"],
    enabled: true,
    update_pending: false,
    new_permissions: EMPTY_ACCESS,
  };
  const wordCount = access({ buffer: ["read:active"] });
  const plot = access({ filesystem: ["write:project"] });
  const fetcher = access({
    network: ["api.crossref.org"],
    filesystem: ["read:project"],
  });
  return [
    {
      ...base,
      name: "word-count",
      version: "0.1.0",
      running_version: "0.1.0",
      description: "Counts words, characters and lines in the open document.",
      permissions: wordCount,
      granted: wordCount,
      status: "loaded",
      source: "word-count.manangplugin",
      installed_at: now - 12 * day,
      commands: [{ id: "word-count.count", title: "Count words" }],
      settings: [],
    },
    {
      ...base,
      name: "plot-export",
      version: "0.3.0",
      running_version: "0.2.0",
      description: "Exports figures as publication-ready plots.",
      permissions: access({ ...plot, network: ["plots.example.org"] }),
      granted: plot,
      new_permissions: access({ network: ["plots.example.org"] }),
      status: "needs_approval",
      update_pending: true,
      publisher: "Example Labs",
      source: "plot-export.manangplugin",
      installed_at: now - 30 * day,
      commands: [{ id: "plot-export.run", title: "Export figure as plot" }],
      settings: [],
    },
    {
      ...base,
      name: "citation-fetch",
      version: "0.1.0",
      description: "Looks up references by DOI and adds them to the bibliography.",
      permissions: fetcher,
      granted: EMPTY_ACCESS,
      new_permissions: fetcher,
      status: "needs_approval",
      source: "citation-fetch",
      installed_at: now - day,
      commands: [{ id: "citation-fetch.lookup", title: "Look up a DOI" }],
      settings: [],
    },
    {
      ...base,
      name: "latex-lint",
      version: "0.4.1",
      description: "Style checks for LaTeX sources.",
      permissions: EMPTY_ACCESS,
      granted: EMPTY_ACCESS,
      status: "failed",
      error:
        "plugin `latex-lint` imports `manang:plugin/http@0.2.0` but its manifest grants no network access",
      source: "latex-lint",
      installed_at: now - 4 * day,
      commands: [{ id: "latex-lint.run", title: "Check the document" }],
      settings: [],
    },
    {
      ...base,
      name: "slide-themes",
      version: "0.1.0",
      description: "Extra themes for slide decks.",
      permissions: EMPTY_ACCESS,
      granted: EMPTY_ACCESS,
      enabled: false,
      disabled_by: "user",
      status: "disabled",
      source: "slide-themes.manangplugin",
      installed_at: now - 60 * day,
      commands: [{ id: "slide-themes.apply", title: "Apply a slide theme" }],
      settings: [],
    },
  ];
}

export function pluginsPlugin(): MockPluginFactory {
  return (emit): MockPlugin => {
    const now = Math.floor(Date.now() / 1000);
    const natives = builtIns();
    const installed = new Map(samples(now).map((p) => [p.name, p]));
    const nativeOff = new Set<string>();
    // What a disabled plugin was before, so enabling it again restores that.
    const before = new Map<string, PluginState["status"]>();

    const stateOf = (name: string): PluginState | undefined => {
      const native = natives.find((p) => p.name === name);
      if (native !== undefined) {
        const off = nativeOff.has(name);
        return off
          ? {
              ...omit(native, "running_version"),
              enabled: false,
              status: "disabled",
              disabled_by: "user",
            }
          : native;
      }
      return installed.get(name);
    };
    const need = (name: string): PluginState => {
      const state = stateOf(name);
      if (state === undefined)
        throw fail("not_installed", `no plugin named \`${name}\` is installed`);
      return state;
    };
    const changed = (name: string) => emit("plugins.state-changed", { name });

    // ---- the sample registry ----
    const gate = (): void => {
      switch (registryMode()) {
        case "off":
          throw fail(
            "registry_off",
            "the plugin registry is switched off (Settings, Plugins, Browse the plugin registry)",
          );
        case "unreachable":
          throw fail(
            "registry_unreachable",
            "could not reach registry.manang.invalid: connection refused",
          );
        case "signature":
          throw fail(
            "registry_signature",
            "the plugin list could not be verified; nothing was installed",
          );
        default:
      }
    };
    const freshness = (): Freshness =>
      registryMode() === "offline"
        ? {
            state: "offline",
            reason: "could not reach registry.manang.invalid: connection refused",
            fetched_at: now - 2 * 86_400,
            generated: "2026-10-17T08:00:00Z",
          }
        : {
            state: "fresh",
            reason: null,
            fetched_at: now,
            generated: "2026-10-19T08:00:00Z",
          };
    const catalogue = () => (registryMode() === "empty" ? [] : MOCK_REGISTRY);
    const versionOf = (id: string): string | null => {
      const s = installed.get(id);
      if (s === undefined) return null;
      return s.update_pending ? s.version : (s.running_version ?? s.version);
    };
    const updatesNow = (): UpdateInfo[] =>
      catalogue().flatMap((p) => {
        const have = versionOf(p.id);
        const latest = latestOf(p);
        const state = installed.get(p.id);
        if (
          have === null ||
          state === undefined ||
          latest === undefined ||
          latest.yanked ||
          !isNewer(p.latest, have)
        )
          return [];
        const added = accessDifference(latest.permissions, state.granted);
        return [
          {
            id: p.id,
            name: p.name,
            installed: have,
            latest: p.latest,
            size: latest.size,
            changelog: latest.changelog,
            compatible: latest.compatible,
            permissions_grew: !accessIsEmpty(added),
            new_permissions: added,
          },
        ];
      });
    const put = (state: PluginState): PluginState => {
      installed.set(state.name, state);
      return state;
    };

    return {
      name: "plugins",
      commands: COMMANDS,
      handlers: {
        "plugins.list": (): PluginList => ({
          plugins: [
            ...natives.map((p) => stateOf(p.name) as PluginState),
            ...installed.values(),
          ].sort((a, b) => a.name.localeCompare(b.name)),
          warnings: [],
        }),

        "plugins.inspect": (args): PluginInspect => {
          const path = args["path"];
          const known =
            typeof path === "string" ? MOCK_PLUGIN_SOURCES[path] : undefined;
          if (known === undefined)
            throw fail(
              "invalid_path",
              `\`${String(path)}\` is not a plugin folder or .manangplugin archive`,
            );
          const current = installed.get(known.name);
          return {
            ...known,
            installed_version: current?.running_version ?? current?.version ?? null,
          };
        },

        "plugins.install": (args): PluginState => {
          const path = args["path"];
          const known =
            typeof path === "string" ? MOCK_PLUGIN_SOURCES[path] : undefined;
          if (known === undefined)
            throw fail(
              "invalid_path",
              `\`${String(path)}\` is not a plugin folder or .manangplugin archive`,
            );
          const current = installed.get(known.name);
          if (current !== undefined && current.version === known.version)
            throw fail(
              "already_installed",
              `\`${known.name}\` ${known.version} is already installed; install a newer version or uninstall it first`,
            );
          const state = put({
            name: known.name,
            version: known.version,
            description: known.description,
            tier: "wasm",
            core: false,
            permissions: known.permissions,
            granted: EMPTY_ACCESS,
            new_permissions: known.permissions,
            enabled: true,
            status: "needs_approval",
            source: String(path).split("/").pop() ?? String(path),
            ...(known.publisher === null ? {} : { publisher: known.publisher }),
            signed: false,
            installed_at: Math.floor(Date.now() / 1000),
            commands: known.commands,
            settings: [],
            contexts: ["app"],
            update_pending: false,
          });
          emit("plugins.needs-approval", { name: state.name, version: state.version });
          changed(state.name);
          return state;
        },

        "plugins.approve": (args): PluginState => {
          const name = nameArg(args);
          const state = need(name);
          const granted = args["permissions"] as PluginAccess | undefined;
          if (state.tier === "native" || state.status !== "needs_approval")
            throw fail(
              "nothing_to_approve",
              `\`${name}\` has nothing waiting for approval`,
            );
          if (granted === undefined || !accessWithin(granted, state.permissions))
            throw fail(
              "permission_mismatch",
              "cannot grant access the plugin does not ask for",
            );
          const next = put({
            ...state,
            version: state.version,
            running_version: state.version,
            granted,
            new_permissions: EMPTY_ACCESS,
            status: state.enabled ? "loaded" : "disabled",
            update_pending: false,
          });
          emit("plugins.loaded", { name });
          changed(name);
          return next;
        },

        "plugins.enable": (args): PluginState => setEnabled(nameArg(args), true),
        "plugins.disable": (args): PluginState => setEnabled(nameArg(args), false),

        "plugins.reload": (args): PluginState => {
          const name = nameArg(args);
          const state = need(name);
          if (state.tier === "native")
            throw fail("built_in", `\`${name}\` is built in and cannot be uninstalled`);
          if (state.status === "failed" && state.error !== undefined)
            emit("plugins.failed", { name, error: state.error });
          else emit("plugins.reloaded", { name });
          changed(name);
          return state;
        },

        "plugins.uninstall": (args) => {
          const name = nameArg(args);
          if (MOCK_CORE_PLUGINS.includes(name))
            throw fail(
              "core_plugin",
              `\`${name}\` is part of the core of Manang and cannot be disabled or removed`,
            );
          if (natives.some((p) => p.name === name))
            throw fail("built_in", `\`${name}\` is built in and cannot be uninstalled`);
          need(name);
          installed.delete(name);
          emit("plugins.unloaded", { name });
          changed(name);
          return {
            name,
            trashed_to: `/home/author/.config/manang/plugins-trash/${name}`,
          };
        },

        "plugins.permissions": (args): PluginPermissionReport => {
          const state = need(nameArg(args));
          return {
            requested: state.permissions,
            granted: state.granted,
            new_permissions: state.new_permissions,
          };
        },

        "plugins.use-project": () => ({ changed: [] }),

        "plugins.configure": (args) => ({ ...(args["values"] as object) }),

        "plugins.registry-status": (): RegistryStatus => ({
          enabled: registryMode() !== "off",
          browse_setting: registryMode() !== "off",
          configured: true,
          reason: null,
          url: "https://registry.manang.invalid",
          host: "registry.manang.invalid",
          custom_url: false,
          fetched_at: now,
          plugin_count: catalogue().length,
          updates: updatesNow().length,
        }),

        "plugins.browse": (): BrowseResult => {
          gate();
          const list = catalogue();
          const tags = new Map<string, number>();
          for (const p of list)
            for (const t of p.tags) tags.set(t, (tags.get(t) ?? 0) + 1);
          return {
            freshness: freshness(),
            total: list.length,
            plugins: list.map((p) => toBrowseEntry(p, versionOf(p.id), isNewer)),
            tags: [...tags.entries()]
              .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
              .map(([tag, count]) => ({ tag, count })),
            updates: updatesNow().length,
          };
        },

        "plugins.detail": (args) => {
          gate();
          const id = String(args["id"]);
          const p = catalogue().find((c) => c.id === id);
          if (p === undefined)
            throw fail(
              "registry_unknown_plugin",
              `no plugin named \`${id}\` is in the registry`,
            );
          return toDetail(p, toBrowseEntry(p, versionOf(p.id), isNewer), freshness());
        },

        "plugins.check-updates": (): UpdateCheck => {
          if (registryMode() === "off") return { enabled: false, updates: [] };
          gate();
          const updates = updatesNow();
          emit("plugins.updates", { count: updates.length });
          return { enabled: true, freshness: freshness(), updates };
        },

        "plugins.cancel-install": () => ({}),

        "plugins.contributions": (): Contributions => ({
          format_adapters: [],
          editors: [],
          toolbars: [],
        }),

        "plugins.install-from-registry": (args): PluginState => {
          if (args["consent"] !== true)
            throw fail(
              "consent_required",
              "installing from the plugin registry needs your consent",
            );
          gate();
          const id = String(args["id"]);
          const p = catalogue().find((c) => c.id === id);
          if (p === undefined)
            throw fail(
              "registry_unknown_plugin",
              `no plugin named \`${id}\` is in the registry`,
            );
          const wanted =
            typeof args["version"] === "string" ? args["version"] : p.latest;
          const v = p.versions.find((x) => x.version === wanted);
          if (v === undefined)
            throw fail(
              "registry_unknown_version",
              `\`${id}\` has no version ${wanted} in the registry`,
            );
          if (v.yanked)
            throw fail(
              "registry_withdrawn",
              `\`${id}\` ${wanted} was withdrawn by the registry and cannot be installed`,
            );
          const progress = (phase: string, received: number) =>
            emit("plugins.install-progress", { id, phase, received, total: v.size });
          progress("downloading", 0);
          progress("downloading", Math.floor(v.size / 2));
          progress("verifying", v.size);
          progress("installing", v.size);
          const current = installed.get(id);
          const state = put({
            ...(current ?? {
              tier: "wasm" as const,
              core: false,
              signed: false,
              contexts: ["app"],
              enabled: true,
              settings: [],
              installed_at: Math.floor(Date.now() / 1000),
            }),
            name: id,
            version: wanted,
            running_version: wanted,
            description: p.description,
            permissions: v.permissions,
            granted: v.permissions,
            new_permissions: EMPTY_ACCESS,
            status: current?.enabled === false ? "disabled" : "loaded",
            publisher: p.author,
            source: "registry",
            origin: {
              registry: "https://registry.manang.invalid",
              id,
              version: wanted,
            },
            commands: current?.commands ?? [
              { id: `${id}.run`, title: `Run ${p.name}` },
            ],
            update_pending: false,
          } as PluginState);
          progress("done", v.size);
          emit("plugins.loaded", { name: id });
          changed(id);
          emit("plugins.updates", { count: updatesNow().length });
          return state;
        },
      },
    };

    function setEnabled(name: string, on: boolean): PluginState {
      if (MOCK_CORE_PLUGINS.includes(name))
        throw fail(
          "core_plugin",
          `\`${name}\` is part of the core of Manang and cannot be disabled or removed`,
        );
      const state = need(name);
      if (state.tier === "native") {
        if (on) nativeOff.delete(name);
        else nativeOff.add(name);
        changed(name);
        return need(name);
      }
      if (!on) before.set(name, state.status);
      const restored = before.get(name) ?? "loaded";
      const next = put(
        on
          ? {
              ...omit(state, "disabled_by"),
              enabled: true,
              status: restored === "disabled" ? "loaded" : restored,
            }
          : { ...state, enabled: false, disabled_by: "user", status: "disabled" },
      );
      emit(on ? "plugins.loaded" : "plugins.unloaded", { name });
      changed(name);
      return next;
    }
  };
}
