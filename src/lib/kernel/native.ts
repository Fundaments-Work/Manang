/**
 * Native desktop integration: the folder picker, the Save dialog, the window
 * controls, opening links and files outside the app, relaunching, and the
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

/* -------------------------------------------------------------------------- */
/* The window: Manang draws its own title bar (`decorations: false`).          */
/* -------------------------------------------------------------------------- */

export type WindowAction = "minimize" | "toggle-maximize" | "close";

/** Whether the app draws its own window controls (the desktop app only). */
export function hasCustomTitleBar(): boolean {
  return inTauri();
}

/** Minimise, maximise/restore or close the app window. */
export async function windowAction(action: WindowAction): Promise<void> {
  if (!inTauri()) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const win = getCurrentWindow();
  if (action === "minimize") await win.minimize();
  else if (action === "toggle-maximize") await win.toggleMaximize();
  else await win.close();
}

/**
 * Follow the window's maximised state (the restore icon differs). Resolves to
 * the function that stops listening.
 */
export async function onMaximizedChange(
  handler: (maximized: boolean) => void,
): Promise<() => void> {
  if (!inTauri()) return () => undefined;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const win = getCurrentWindow();
  handler(await win.isMaximized());
  return win.onResized(() => {
    void win.isMaximized().then(handler);
  });
}

/** The operating system, for platform conventions such as where window controls go. */
export async function osPlatform(): Promise<"macos" | "windows" | "linux" | "other"> {
  if (!inTauri()) return "other";
  const { platform } = await import("@tauri-apps/plugin-os");
  const p = platform();
  return p === "macos" || p === "windows" || p === "linux" ? p : "other";
}

/* -------------------------------------------------------------------------- */
/* Opening things outside the app                                              */
/* -------------------------------------------------------------------------- */

/** Show a file in the system file manager. False when unavailable. */
export async function revealInFileManager(path: string): Promise<boolean> {
  if (!inTauri()) return false;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(path);
  return true;
}

/** Open a web link (a DOI, a licence) in the default browser. */
export async function openExternal(url: string): Promise<void> {
  if (!/^https?:\/\//i.test(url)) throw new Error(`Not a web link: ${url}`);
  if (!inTauri()) {
    window.open(url, "_blank", "noopener,noreferrer");
    return;
  }
  const { openUrl } = await import("@tauri-apps/plugin-opener");
  await openUrl(url);
}

/** Restart the app, for settings that apply after a restart. */
export async function relaunchApp(): Promise<boolean> {
  if (!inTauri()) {
    window.location.reload();
    return true;
  }
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
  return true;
}
