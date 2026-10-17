/**
 * Kernel selection.
 *
 * Inside the Tauri shell the real IPC client talks to the Rust plugin kernel.
 * In a plain browser (`pnpm dev`, Playwright, vitest) there is no
 * `__TAURI_INTERNALS__`, so an in-memory kernel with the same command shapes
 * (editor, project and typst plugins) stands in. Everything else imports
 * `kernel` from here and never learns which it got.
 */

import { kernel as tauriKernel, type Kernel } from "./client";
import { createDevKernel } from "./dev";

export type {
  Args,
  Kernel,
  KernelCommand,
  KernelError,
  KernelEvent,
  Unlisten,
} from "./client";
export { errorMessage, isKernelError } from "./client";

export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const kernel: Kernel = inTauri() ? tauriKernel : createDevKernel();
