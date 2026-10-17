/**
 * Native desktop integration: the folder picker, the Save dialog and the
 * shell's `save_binary_artifact` command.
 *
 * Only the Tauri build has these. In a browser every function says so (by
 * returning `null` or throwing) and callers fall back to what a browser can do:
 * a typed path, a download. The dialog plugin is imported lazily so the browser
 * bundle never loads it.
 */

import { inTauri } from "./index";

/** Whether native dialogs exist (the desktop app, not `pnpm dev`). */
export function hasNativeDialogs(): boolean {
  return inTauri();
}

/** Ask for a folder. Resolves to its path, or null when cancelled or unavailable. */
export async function pickFolder(title: string): Promise<string | null> {
  if (!inTauri()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const picked = await open({ directory: true, multiple: false, title });
  return typeof picked === "string" ? picked : null;
}

/** Ask where to save a file. Resolves to an absolute path, or null when cancelled. */
export async function pickSavePath(
  defaultName: string,
  filter: { readonly name: string; readonly extensions: readonly string[] },
): Promise<string | null> {
  if (!inTauri()) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const picked = await save({
    defaultPath: defaultName,
    filters: [{ name: filter.name, extensions: [...filter.extensions] }],
  });
  return typeof picked === "string" ? picked : null;
}

/**
 * Run a binary kernel command in the shell and write its bytes to `destPath`.
 * The bytes never cross IPC. Resolves to the number of bytes written.
 */
export async function saveBinaryArtifact(
  command: string,
  args: Record<string, unknown> | null,
  destPath: string,
): Promise<number> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<number>("save_binary_artifact", {
    command,
    args,
    destPath,
  });
}
