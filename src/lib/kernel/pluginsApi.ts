/**
 * Typed wrappers over the plugin manager's `plugins.*` commands
 * (`crates/manang-plugins`). Argument names and result shapes are the
 * manifest's; the Rust side is the source of truth.
 */

import type { Kernel } from "./client";

/** Where a plugin stands: running, switched off, broken, or waiting for consent. */
export type PluginStatus = "loaded" | "disabled" | "failed" | "needs_approval";

/** A set of permissions, as a manifest declares and the user approves them. */
export interface PluginAccess {
  readonly filesystem: readonly string[];
  readonly network: readonly string[];
  readonly buffer: readonly string[];
}

export interface PluginCommandRef {
  readonly id: string;
  readonly title: string;
}

/** One plugin, built in or installed (`plugins.list`). */
export interface PluginState {
  readonly name: string;
  /** The version on disk; the waiting one while an update needs approval. */
  readonly version: string;
  /** The version that is running, when one is. */
  readonly running_version?: string;
  readonly description: string;
  readonly tier: "native" | "wasm";
  /** Required by the app: cannot be disabled or removed. */
  readonly core: boolean;
  /** What it asks for. */
  readonly permissions: PluginAccess;
  /** What the user approved. */
  readonly granted: PluginAccess;
  /** Asked for but not yet approved; what an update adds. */
  readonly new_permissions: PluginAccess;
  readonly enabled: boolean;
  readonly disabled_by?: "user" | "project";
  readonly status: PluginStatus;
  readonly error?: string;
  readonly dir?: string;
  readonly source?: string;
  /** Declared by the plugin, unverified while `signed` is false. */
  readonly publisher?: string;
  readonly signed: boolean;
  /** Seconds since the Unix epoch. */
  readonly installed_at?: number;
  readonly commands: readonly PluginCommandRef[];
  readonly settings: readonly string[];
  readonly contexts: readonly string[];
  /** A newer version waits beside the running one. */
  readonly update_pending: boolean;
}

export interface PluginList {
  readonly plugins: readonly PluginState[];
  /** Startup problems worth showing (a damaged registry). */
  readonly warnings: readonly string[];
}

/** What a plugin archive or folder is, read without installing it. */
export interface PluginInspect {
  readonly name: string;
  readonly version: string;
  readonly description: string;
  readonly tier: "native" | "wasm";
  readonly publisher: string | null;
  readonly signed: boolean;
  readonly kind: "archive" | "folder";
  readonly permissions: PluginAccess;
  /** Beyond what the user already approved for this plugin (all, if new). */
  readonly new_permissions: PluginAccess;
  readonly commands: readonly PluginCommandRef[];
  /** Set when installing would update this version. */
  readonly installed_version: string | null;
}

export interface PluginPermissionReport {
  readonly requested: PluginAccess;
  readonly granted: PluginAccess;
  readonly new_permissions: PluginAccess;
}

export function pluginsApi(kernel: Kernel) {
  const named = <T>(command: string, name: string) => kernel.run<T>(command, { name });
  return {
    list: () => kernel.run<PluginList>("plugins.list", {}),
    /** Read a folder or `.manangplugin` archive; installs nothing. */
    inspect: (path: string) => kernel.run<PluginInspect>("plugins.inspect", { path }),
    /** Validate and copy in; the plugin then waits for `approve`. */
    install: (path: string) => kernel.run<PluginState>("plugins.install", { path }),
    approve: (name: string, permissions: PluginAccess) =>
      kernel.run<PluginState>("plugins.approve", { name, permissions }),
    enable: (name: string) => named<PluginState>("plugins.enable", name),
    disable: (name: string) => named<PluginState>("plugins.disable", name),
    reload: (name: string) => named<PluginState>("plugins.reload", name),
    uninstall: (name: string) =>
      named<{ name: string; trashed_to: string | null }>("plugins.uninstall", name),
    permissions: (name: string) =>
      named<PluginPermissionReport>("plugins.permissions", name),
    /** Apply the open project's `.manang/plugins.toml` (null clears it). */
    useProject: (path: string | null) =>
      kernel.run<{ changed: readonly string[] }>("plugins.use-project", { path }),
  };
}

export type PluginsApi = ReturnType<typeof pluginsApi>;
