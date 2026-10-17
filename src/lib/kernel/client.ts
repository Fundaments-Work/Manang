/**
 * Typed client for the Rust plugin kernel.
 *
 * The shell is a thin presentation layer: it never implements a feature, it
 * dispatches a command id to the kernel and renders the result. Structured data
 * crosses IPC as JSON; large artefacts (PDF, SVG) come back as a raw
 * `Uint8Array`, never base64.
 */

import { invoke } from "@tauri-apps/api/core";

/** A command contributed by a plugin manifest, as listed for the palette. */
export interface KernelCommand {
  readonly id: string;
  readonly title: string;
  readonly plugin: string;
}

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

export type Args = Record<string, unknown>;

/**
 * The surface every kernel backend implements: the Tauri IPC client below and
 * the in-memory mock used in the browser. Callers depend on this, never on a
 * concrete backend.
 */
export interface Kernel {
  commands(): Promise<KernelCommand[]>;
  run<T = unknown>(command: string, args?: Args): Promise<T>;
  runBinary(command: string, args?: Args): Promise<Uint8Array>;
}

export const kernel: Kernel = {
  commands(): Promise<KernelCommand[]> {
    return invoke<KernelCommand[]>("kernel_commands");
  },

  /** Run a command that returns structured data. */
  run<T = unknown>(command: string, args?: Args): Promise<T> {
    return invoke<T>("kernel_invoke", { command, args });
  },

  /** Run a command that returns a binary artefact. */
  async runBinary(command: string, args?: Args): Promise<Uint8Array> {
    const bytes = await invoke<ArrayBuffer>("kernel_invoke_binary", {
      command,
      args,
    });
    return new Uint8Array(bytes);
  },
};
