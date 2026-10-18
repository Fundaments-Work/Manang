# Plugins

You can view installed plugins, understand security sandboxing and permissions, install third-party extensions, and interpret plugin status indicator lines.

## 1. What plugins are

In Manang, features are modular plugins that contribute commands, settings, hooks, and tools to the application.

1. To view and manage installed plugins:
   - Press `Ctrl+Shift+X` (`Cmd+Shift+X`) or click the **Plugins** icon in the Activity bar to open the Plugins sidebar view.
   - Or open Settings (`Ctrl+,` (`Cmd+,`)) and scroll down to the Plugins section.
2. The Plugins sidebar lets you filter by category:
   - **All:** View all registered plugins.
   - **Installed:** Inspect third-party plugins installed in your configuration directory.
   - **Built-in:** View native core plugins.
   - **Attention:** Highlights any plugin reporting load errors or requiring configuration.
3. Click any plugin entry to view its version, execution tier, description, contributed commands, and requested permissions. Use the toggle switch to enable or disable individual sandboxed plugins.

## 2. Built-in versus Sandboxed plugins

Manang distinguishes between core first-party plugins and third-party extensions:

1. **Built-in (Native) plugins:**
   - First-party plugins compiled directly into the application in Rust (such as Typst, LaTeX, Markdown, Citations, Diagram, Figure, Writing, Git, Search, and Settings).
   - They provide maximum performance, sub-millisecond execution, and zero-copy binary IPC.
   - Built-in plugins cannot be uninstalled.
2. **Sandboxed (WebAssembly component) plugins:**
   - Third-party extensions compiled to WebAssembly components (`wasm32-unknown-unknown`).
   - Run inside isolated Wasmtime component-model sandboxes.
   - Sandboxed plugins have **no ambient access** to your operating system, files, network, or clocks. They can only interact with host services that their manifest explicitly requests and that you approve.
   - Every command and hook invocation runs in a fresh, isolated instance; a crash in a third-party plugin cannot crash Manang or corrupt your documents.

## 3. Install a plugin

Until the visual plugin store is released, install plugins by placing their files in your user configuration directory.

1. Locate your platform's plugins configuration directory:
   - **Linux:** `~/.config/manang/plugins/`
   - **macOS:** `~/Library/Application Support/manang/plugins/`
   - **Windows:** `%APPDATA%\manang\plugins\` (usually `C:\Users\<Username>\AppData\Roaming\manang\plugins\`)
2. Create a folder named exactly after the plugin (for example, `word-count`).
3. Place two files inside the directory:
   - `manifest.toml`: The plugin manifest defining its name, version, requested permissions, and contributed commands.
   - `plugin.wasm`: The compiled WebAssembly component.
4. Manang detects new plugins at launch and automatically hot-reloads them when files in the directory are modified.

## 4. Understand plugin permissions

Sandboxed plugins must declare every capability they require in their `manifest.toml`. If a plugin tries to perform an ungranted operation, the sandbox halts the call.

1. **Buffer permissions (`buffer`):**
   - `read:active`: Allows the plugin to read the text of your currently open document tab.
   - `write:active`: Allows the plugin to modify or replace text in the active tab (e.g. for code formatting or snippets).
2. **Filesystem permissions (`filesystem`):**
   - `read:<dir>`: Grants read access to files inside `<project>/<dir>`.
   - `write:<dir>`: Grants read and write access inside `<project>/<dir>`.
   - Permissions are strictly confined to the project directory; directory traversal (`..`) is blocked.
3. **Network permissions (`network`):**
   - Must specify exact hostnames (for example: `["api.crossref.org", "export.arxiv.org"]`).
   - Wildcards, arbitrary IP addresses, and unencrypted HTTP connections are rejected.
4. Review permissions: In Settings (`Ctrl+,` (`Cmd+,`)), each plugin lists its granted access in plain English (e.g., *"Reads and changes project files"*, *"Connects to api.crossref.org"*).

## 5. What the indicator lines mean

Plugin listings and status cards use tonal color indicator lines to reflect operational health:

1. **Green indicator line:**
   - **Loaded and active.**
   - The plugin's WebAssembly component compiled cleanly, its manifest is valid, and all requested host interfaces are linked and ready.
2. **Amber indicator line:**
   - **Warning or restricted.**
   - The plugin is functional but operating with limitations (for example: a newly installed version requested additional permissions awaiting user consent, or an optional dependency is missing).
3. **Red indicator line:**
   - **Failed to load or compile.**
   - The plugin could not be started. Common causes include a malformed `manifest.toml`, an ungranted import (the WebAssembly component imports a host interface not declared under `[permissions]`), or an execution crash.
   - Check the **Problems** tab (`Ctrl+Shift+M` (`Cmd+Shift+M`)) or the application log file for the exact failure message.
