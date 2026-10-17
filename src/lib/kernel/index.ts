/**
 * Kernel selection.
 *
 * Inside the Tauri shell the real IPC client talks to the Rust plugin kernel.
 * In a plain browser (`pnpm dev`, Playwright, vitest) there is no
 * `__TAURI_INTERNALS__`, so an in-memory mock with the same contract stands in.
 * Everything else imports `kernel` from here and never learns which it got.
 */

import { kernel as tauriKernel, type Kernel } from "./client";
import { createMockKernel } from "./mock";

export type { Args, Kernel, KernelCommand, KernelError } from "./client";
export { isKernelError } from "./client";

export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const kernel: Kernel = inTauri() ? tauriKernel : createMockKernel();
