/**
 * The default keyboard shortcuts, by command id.
 *
 * The keybindings registry (`crates/manang-settings`, `CORE_DEFAULTS`) is the
 * source of truth in the desktop app: it merges these defaults with the
 * person's `keybindings.toml` and reports conflicts. This copy serves the
 * browser build (the in-memory settings plugin) and the moment before the
 * registry answers; `defaults.test.ts` reads the Rust table and fails if the
 * two differ.
 *
 * Keys are in the registry's canonical form: `Mod` is Ctrl on Windows and
 * Linux and Cmd on macOS.
 */

export interface DefaultBinding {
  readonly command: string;
  readonly key: string;
}

const TABS: readonly DefaultBinding[] = Array.from({ length: 9 }, (_, i) => ({
  command: `tab.go-${i + 1}`,
  key: `Mod+${i + 1}`,
}));

export const DEFAULT_BINDINGS: readonly DefaultBinding[] = [
  { command: "palette.commands", key: "Mod+K" },
  { command: "palette.commands", key: "Mod+Shift+P" },
  { command: "goto.file", key: "Mod+P" },
  { command: "project.switch", key: "Mod+O" },
  { command: "home.show", key: "Mod+Shift+H" },
  { command: "view.files", key: "Mod+Shift+E" },
  { command: "view.search", key: "Mod+Shift+F" },
  { command: "view.plugins", key: "Mod+Shift+X" },
  { command: "agent.toggle", key: "Mod+Shift+A" },
  { command: "view.settings", key: "Mod+," },
  { command: "view.toggle-side", key: "Mod+B" },
  { command: "view.toggle-panel", key: "Mod+J" },
  { command: "view.problems", key: "Mod+Shift+M" },
  { command: "view.cycle-preview", key: "Mod+Shift+V" },
  { command: "view.zen", key: "Mod+\\" },
  { command: "tab.next", key: "Mod+PageDown" },
  { command: "tab.next", key: "Mod+Tab" },
  { command: "tab.previous", key: "Mod+PageUp" },
  { command: "tab.previous", key: "Mod+Shift+Tab" },
  { command: "tab.close", key: "Mod+W" },
  ...TABS,
  { command: "file.new", key: "Mod+N" },
  { command: "file.save", key: "Mod+S" },
  { command: "file.save-all", key: "Mod+Shift+S" },
  { command: "file.save-version", key: "Mod+Alt+S" },
  { command: "doc.compile", key: "Mod+Shift+B" },
  { command: "doc.export-pdf", key: "Mod+E" },
  { command: "doc.format", key: "Alt+Shift+F" },
  { command: "doc.show-in-preview", key: "Mod+Alt+J" },
  { command: "share.open", key: "Mod+Alt+L" },
  { command: "doc.present", key: "F5" },
  { command: "help.shortcuts", key: "Mod+/" },
  { command: "dev.inspect", key: "F12" },
  { command: "dev.inspect", key: "Mod+Shift+I" },
  { command: "app.quit", key: "Mod+Q" },
];
