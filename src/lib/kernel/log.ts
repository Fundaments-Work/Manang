/**
 * One place for the shell's diagnostics, so a failure is never silent.
 *
 * `reportError` prints to the devtools console. In the desktop app every
 * `console.error` and `console.warn` (ours, Svelte's, a library's), every
 * uncaught error and every unhandled rejection is also copied to the Rust log,
 * which prints in the terminal running `pnpm app:dev` and is kept in the app's
 * log file. Messages from before the log plugin has loaded are queued.
 */

/** Whether this is the desktop app (Tauri) rather than a browser. */
export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

type Level = "error" | "warn";
type Sink = (level: Level, text: string) => void;

let sink: Sink | null = null;
const queued: [Level, string][] = [];
let installed = false;

/** Text for anything thrown: an Error's stack, a kernel `{ message }`, JSON. */
export function describe(value: unknown): string {
  if (value instanceof Error) return value.stack ?? `${value.name}: ${value.message}`;
  if (typeof value === "string") return value;
  if (
    typeof value === "object" &&
    value !== null &&
    typeof (value as { message?: unknown }).message === "string"
  )
    return (value as { message: string }).message;
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function forward(level: Level, text: string): void {
  if (sink !== null) sink(level, text);
  else if (installed) queued.push([level, text]);
}

/** Log a failure the caller handles (it shows a notice, falls back, ...). */
export function reportError(where: string, error: unknown): void {
  // The text first: WebKit's console shows a bare object as "Object".
  console.error(`[manang] ${where}: ${describe(error)}`, error);
}

/** Log something unexpected that is not a failure. */
export function reportWarning(where: string, detail: unknown): void {
  // The text first: WebKit's console shows a bare object as "Object".
  console.warn(`[manang] ${where}: ${describe(detail)}`, detail);
}

/**
 * Copy console errors and warnings, uncaught errors and unhandled rejections
 * to the Rust log. Call once, before mounting, so boot errors are caught too.
 * In a browser (`pnpm dev`) the devtools console already has everything.
 */
export function installErrorLog(): void {
  if (installed || !inTauri()) return;
  installed = true;

  const mirror =
    (level: Level, original: (...args: unknown[]) => void) =>
    (...args: unknown[]) => {
      original(...args);
      forward(level, args.map(describe).join(" "));
    };
  console.error = mirror("error", console.error.bind(console));
  console.warn = mirror("warn", console.warn.bind(console));
  window.addEventListener("error", (event) => {
    const at = event.filename
      ? ` (${event.filename}:${event.lineno}:${event.colno})`
      : "";
    forward("error", `uncaught: ${describe(event.error ?? event.message)}${at}`);
  });
  window.addEventListener("unhandledrejection", (event) => {
    forward("error", `unhandled rejection: ${describe(event.reason)}`);
  });

  void import("@tauri-apps/plugin-log")
    .then(({ error, warn }) => {
      // A failing log call must not report itself, or it would loop.
      sink = (level, text) => {
        void (level === "error" ? error : warn)(`[web] ${text}`).catch(() => undefined);
      };
      for (const [level, text] of queued.splice(0)) sink(level, text);
    })
    .catch((failure: unknown) => {
      // Devtools only from here on; the sink is set first so this cannot loop.
      queued.length = 0;
      sink = () => undefined;
      reportError("the log plugin could not load", failure);
    });
}

/**
 * Whether the web inspector exists: a development build of the desktop app.
 * Release builds have none (right-click, Inspect Element is gone too).
 */
export function canOpenDevtools(): boolean {
  return import.meta.env.DEV && inTauri();
}

/**
 * Open the web inspector. The `dev.inspect` command runs this; its keys (F12,
 * Ctrl+Shift+I) live in the keybindings registry like every other shortcut.
 */
export function openDevtools(): void {
  if (!canOpenDevtools()) return;
  void import("@tauri-apps/api/core")
    .then(({ invoke }) => invoke("open_devtools"))
    .catch((error: unknown) => reportError("the inspector could not open", error));
}
