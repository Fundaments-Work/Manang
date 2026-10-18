/**
 * Typed client for the Rust plugin kernel.
 *
 * The shell is a thin presentation layer: it never implements a feature, it
 * dispatches a command id to the kernel and renders the result. Structured data
 * crosses IPC as JSON; large artefacts (PDF, SVG) come back as a raw
 * `Uint8Array`, never base64.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { reportError } from "./log";

/** A command contributed by a plugin manifest, as listed for the palette. */
export interface KernelCommand {
  readonly id: string;
  readonly title: string;
  readonly plugin: string;
  /** Menu placements from the manifest's `[[contributes.menus]]`. */
  readonly menus?: readonly KernelMenuPlacement[] | undefined;
}

/** Where a plugin's manifest places one of its commands in the menu bar. */
export interface KernelMenuPlacement {
  readonly bar: "file" | "edit" | "view" | "go" | "build" | "help";
  readonly group: string;
  readonly order: number;
}

/** Access a plugin's manifest declares. */
export interface PluginPermissions {
  readonly filesystem?: readonly string[] | undefined;
  readonly network?: readonly string[] | undefined;
  readonly buffer?: readonly string[] | undefined;
}

/** One registered plugin, summarised from its manifest (`kernel_plugins`). */
export interface PluginInfo {
  readonly name: string;
  readonly version: string;
  readonly description: string;
  readonly tier: "native" | "wasm";
  readonly permissions: PluginPermissions;
  /** Number of commands it contributes. */
  readonly commands: number;
  /** Ids of the settings it declares in its manifest. */
  readonly settings: readonly string[];
  /** `[plugin] settings_contexts`: where its Settings section belongs. Absent means `["app"]`. */
  readonly contexts?: readonly string[];
}

/**
 * An event published on the kernel bus and forwarded by the shell as the
 * `kernel://event` Tauri event, e.g. `project.changed`.
 */
export interface KernelEvent {
  readonly topic: string;
  readonly payload: unknown;
}

export type Unlisten = () => void;

/** Failure reported by the kernel. */
export interface KernelError {
  readonly message: string;
}

export function isKernelError(value: unknown): value is KernelError {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { message?: unknown }).message === "string"
  );
}

/** Human-readable text of anything a kernel call may reject with. */
export function errorMessage(error: unknown): string {
  if (isKernelError(error)) return error.message;
  return error instanceof Error ? error.message : String(error);
}

export type Args = Record<string, unknown>;

/**
 * The surface every kernel backend implements: the Tauri IPC client below and
 * the in-memory mock used in the browser. Callers depend on this, never on a
 * concrete backend.
 */
export interface Kernel {
  commands(): Promise<KernelCommand[]>;
  /** Manifest summaries; optional because older shells and test kernels lack it. */
  plugins?(): Promise<PluginInfo[]>;
  run<T = unknown>(command: string, args?: Args): Promise<T>;
  runBinary(command: string, args?: Args): Promise<Uint8Array>;
  /**
   * Subscribe to kernel events. Resolves once the subscription is live and
   * yields the function that cancels it. Never rejects: a backend without an
   * event channel simply never calls the handler.
   */
  onEvent(handler: (event: KernelEvent) => void): Promise<Unlisten>;
}

/**
 * Every IPC call goes through here, so a failure is logged with the command
 * that failed even when the caller catches it and carries on.
 */
async function call<T>(ipc: string, label: string, args?: Args): Promise<T> {
  try {
    return await (args === undefined ? invoke<T>(ipc) : invoke<T>(ipc, args));
  } catch (error) {
    reportError(`${label} failed`, error);
    throw error;
  }
}

export const kernel: Kernel = {
  commands(): Promise<KernelCommand[]> {
    return call<KernelCommand[]>("kernel_commands", "kernel_commands");
  },

  plugins(): Promise<PluginInfo[]> {
    return call<PluginInfo[]>("kernel_plugins", "kernel_plugins");
  },

  /** Run a command that returns structured data. */
  run<T = unknown>(command: string, args?: Args): Promise<T> {
    return call<T>("kernel_invoke", command, { command, args });
  },

  /** Run a command that returns a binary artefact. */
  async runBinary(command: string, args?: Args): Promise<Uint8Array> {
    const bytes = await call<ArrayBuffer>("kernel_invoke_binary", command, {
      command,
      args,
    });
    return new Uint8Array(bytes);
  },

  async onEvent(handler: (event: KernelEvent) => void): Promise<Unlisten> {
    try {
      return await listen<KernelEvent>("kernel://event", (message) =>
        handler(message.payload),
      );
    } catch (error) {
      reportError("kernel events are unavailable", error);
      return () => undefined;
    }
  },
};
