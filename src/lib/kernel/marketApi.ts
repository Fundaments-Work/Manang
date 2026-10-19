/**
 * Typed wrappers over the plugin marketplace commands of `crates/manang-plugins`
 * (`plugins.browse`, `plugins.detail`, `plugins.check-updates`,
 * `plugins.install-from-registry`, `plugins.registry-status`). The Rust side
 * (`marketplace.rs`) is the source of truth for names and shapes.
 */

import type { Kernel } from "./client";
import type { PluginState } from "./pluginsApi";

/** What a plugin may touch, as a manifest and the registry index list it. */
export interface RegistryAccess {
  readonly filesystem: readonly string[];
  readonly network: readonly string[];
  readonly buffer: readonly string[];
}

export type BrowseSort = "name" | "newest" | "downloads";

/**
 * How current the list is. `fresh`: just downloaded; `unchanged`: the registry
 * confirmed the cached list; `cached`: served from disk without asking;
 * `offline`: the registry did not answer, this is the last verified list.
 */
export interface Freshness {
  readonly state: "fresh" | "unchanged" | "cached" | "offline";
  readonly reason: string | null;
  /** Unix seconds of the last successful contact. */
  readonly fetched_at: number;
  readonly generated: string;
}

/** One registry plugin in the Browse list. */
export interface BrowseEntry {
  readonly id: string;
  readonly name: string;
  readonly author: string;
  readonly description: string;
  readonly tags: readonly string[];
  readonly license: string;
  readonly latest: string;
  /** ISO timestamp of the latest version. */
  readonly published: string | null;
  readonly size: number | null;
  readonly downloads: number | null;
  /** The latest version runs on this Manang (`min_app`). */
  readonly compatible: boolean;
  readonly installed: {
    readonly version: string;
    readonly update_available: boolean;
  } | null;
}

export interface BrowseResult {
  readonly freshness: Freshness;
  readonly total: number;
  readonly plugins: readonly BrowseEntry[];
  readonly tags: readonly { readonly tag: string; readonly count: number }[];
  readonly updates: number;
}

export interface RegistryVersion {
  readonly version: string;
  readonly published: string;
  readonly size: number;
  readonly min_app: string | null;
  readonly changelog: string;
  readonly permissions: RegistryAccess;
  readonly compatible: boolean;
  /** Withdrawn by the registry: not offered to install or update. */
  readonly yanked: boolean;
}

/** One registry plugin's page (`plugins.detail`). */
export interface PluginDetail {
  readonly freshness: Freshness;
  readonly plugin: BrowseEntry;
  readonly repo: string;
  readonly homepage: string | null;
  readonly tier: string;
  /** What the latest version asks for. */
  readonly permissions: RegistryAccess | null;
  /** In the registry's order (newest first); the page sorts by version itself. */
  readonly versions: readonly RegistryVersion[];
  /** Markdown, untrusted: render it through the sanitising renderer. */
  readonly readme: string | null;
  readonly readme_error: string | null;
}

/** A newer version of an installed plugin. */
export interface UpdateInfo {
  readonly id: string;
  readonly name: string;
  readonly installed: string;
  readonly latest: string;
  readonly size: number;
  readonly changelog: string;
  readonly compatible: boolean;
  /** The update asks for access that was not approved before. */
  readonly permissions_grew: boolean;
  readonly new_permissions: RegistryAccess;
}

export interface UpdateCheck {
  /** False when browsing is turned off: nothing was contacted. */
  readonly enabled: boolean;
  readonly freshness?: Freshness;
  readonly updates: readonly UpdateInfo[];
}

export interface RegistryStatus {
  readonly enabled: boolean;
  readonly browse_setting: boolean;
  readonly configured: boolean;
  readonly reason: string | null;
  readonly url: string;
  readonly host: string | null;
  readonly custom_url: boolean;
  readonly fetched_at: number | null;
  readonly plugin_count: number | null;
  readonly updates: number;
}

/** What a plugin contributes beyond commands (`plugins.contributions`). */
export interface Contributions {
  readonly format_adapters: readonly {
    readonly plugin: string;
    readonly id: string;
    readonly title: string;
    readonly extensions: readonly string[];
  }[];
  readonly editors: readonly {
    readonly plugin: string;
    readonly id: string;
    readonly kind: string;
    readonly title: string;
  }[];
  readonly toolbars: readonly {
    readonly plugin: string;
    /** The editor the toolbar belongs to. */
    readonly editor: string;
    readonly items: readonly { readonly command: string }[];
  }[];
}

/** Progress of a registry install (`plugins.install-progress`). */
export interface InstallProgress {
  readonly id: string;
  readonly phase: "downloading" | "verifying" | "installing" | "done";
  readonly received: number;
  readonly total: number;
}

export function marketApi(kernel: Kernel) {
  return {
    status: () => kernel.run<RegistryStatus>("plugins.registry-status", {}),
    browse: (refresh: boolean) =>
      kernel.run<BrowseResult>("plugins.browse", { refresh }),
    detail: (id: string) => kernel.run<PluginDetail>("plugins.detail", { id }),
    checkUpdates: (background: boolean) =>
      kernel.run<UpdateCheck>("plugins.check-updates", { background }),
    /** Download, verify and install. The caller must have shown the access and got consent. */
    install: (id: string, version?: string) =>
      kernel.run<PluginState>("plugins.install-from-registry", {
        id,
        ...(version === undefined ? {} : { version }),
        consent: true,
      }),
    cancelInstall: () =>
      kernel.run<Record<string, never>>("plugins.cancel-install", {}),
    contributions: () => kernel.run<Contributions>("plugins.contributions", {}),
  };
}

export type MarketApi = ReturnType<typeof marketApi>;
