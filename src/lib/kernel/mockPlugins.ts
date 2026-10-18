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

import { EMPTY_ACCESS, accessWithin } from "$lib/workbench/pluginManager";

import type { MockPlugin, MockPluginFactory } from "./mock";
import { MOCK_MANIFESTS } from "./mockManifests";
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
];

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
