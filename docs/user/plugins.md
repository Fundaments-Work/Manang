# Plugins

You can browse and install plugins from the plugin registry, update and remove them, understand what each plugin is allowed to do, and read the plugin status lines.

## 1. What plugins are

In Manang, features are modular plugins that contribute commands, settings, hooks, and tools to the application.

1. To view and manage plugins:
   - Press `Ctrl+Shift+X` (`Cmd+Shift+X`) or click the **Plugins** icon in the Activity bar to open the Plugins sidebar view.
   - Or open Settings (`Ctrl+,` (`Cmd+,`)) and scroll down to the Plugins section.
2. The Plugins sidebar is a compact list with two tabs:
   - **Installed:** everything Manang runs as a plugin, built in and installed. The filter chips narrow it to **All**, **Installed** (third-party plugins), **Built in** (first-party plugins) or **Needs attention** (a plugin that failed to load or waits for your approval).
   - **Browse:** the plugins in the plugin registry (see section 3).
   - One search field at the top serves both tabs.
3. Click a plugin in either list to open its **plugin page** as a tab in the editor area (section 4). Use the small switch on an installed row to enable or disable it quickly.
4. When updates are available, the Plugins icon shows a number and the Installed tab says "N updates available" with **Update all** (section 5).

## 2. Built-in versus Sandboxed plugins

Manang distinguishes between core first-party plugins and third-party extensions:

1. **Built-in (Native) plugins:**
   - First-party plugins compiled directly into the application in Rust (such as Typst, LaTeX, Markdown, Citations, Diagram, Figure, Writing, Git, Search, and Settings).
   - They provide maximum performance, sub-millisecond execution, and zero-copy binary IPC.
   - Built-in plugins cannot be uninstalled; most can be switched off, a few core ones cannot.
2. **Sandboxed (WebAssembly component) plugins:**
   - Third-party extensions compiled to WebAssembly components (`wasm32-unknown-unknown`).
   - Run inside isolated Wasmtime component-model sandboxes.
   - Sandboxed plugins have **no ambient access** to your operating system, files, network, or clocks. They can only interact with host services that their manifest explicitly requests and that you approve.
   - Every command and hook invocation runs in a fresh, isolated instance; a crash in a third-party plugin cannot crash Manang or corrupt your documents.

## 3. Browse and install from the registry

The **Browse** tab lists the plugins of the plugin registry: a catalogue of reviewed, sandboxed plugins. Each row shows the name, author, latest version, a short description, tags, whether you have it, and how often it was downloaded.

1. Open **Plugins > Browse**. Search by name, author or tag (every word must match), pick a tag chip to narrow the list, and sort by **Name**, **Newest** or **Downloads**.
2. Click a row to read the plugin's page first (recommended), or press **Install** on the row.
3. Manang then shows the **consent window**: what the plugin asks to access, in plain language, taken from the registry's signed list. Nothing is downloaded until you press **Approve and install**; **Cancel** (or `Esc`) installs nothing.
4. Manang downloads the plugin and checks it twice before it is installed: its size and SHA-256 must match the registry's signed list, and the plugin's own manifest must ask for exactly the access you were shown. If either check fails, nothing is installed and you are told why.
5. The plugin is installed with exactly the access you approved and starts at once.

### What you may see instead of the list

- **Showing the list from 2026-10-17:** you are offline or the registry is not answering. Manang shows the last list it verified, with its date. Installing needs the network. Press **Retry** to ask again.
- **The plugin registry could not be reached:** nothing was cached yet. Check your connection and press **Retry**.
- **The plugin list could not be verified:** the list did not carry a valid signature from a key Manang trusts. Nothing was installed and the plugins you have are not affected. Try again later; if it persists, report it.
- **The plugin list needs a newer Manang:** the registry moved to a newer list format. Update Manang to browse.
- **The plugin registry is not available yet:** this build of Manang has no registry address or key. You can still install from a file (section 7).
- **Browsing plugins is turned off:** see section 9.
- **No plugin matches your search:** clear the search or the tag.

## 4. The plugin page

Clicking a plugin opens its page as a tab, like an extension's page in other editors. One tab per plugin: opening it again shows the same tab, and it closes like any tab. The page works for plugins you have installed, for registry plugins you have not installed yet, and for built-in plugins.

- **Header:** the icon (made from the plugin's initials), name, publisher, version, a one-line description and tags. On the right, the actions: **Install**, or **Update to x.y.z**, **Disable** / **Enable** and **Uninstall**. A built-in plugin shows "Built in: can be switched off but not removed". The one highlighted button is the main action.
- **Details:** the plugin's readme, rendered safely (links and raw HTML are filtered).
- **Features:** what an installed plugin adds: its commands (with ids and shortcuts), settings (with a button into Settings), agent tools, formats, editors and toolbars. A plugin you have not installed shows these once it is installed.
- **Permissions:** what the plugin is allowed to do, in plain language, and what each line means (section 8). For an installed plugin each line says Allowed or Not approved, and an update's new access is marked **New**.
- **Changelog:** the notes of every version, newest first.
- **Versions:** the installed version, the latest, the history with sizes, and **Install this version** for a newer one. A version the registry withdrew is marked **Withdrawn** and cannot be installed.
- **Facts** (beside the tabs, or under the header in a narrow window): version, publisher, repository (opens in your browser), licence, size, last updated, id, type (Sandboxed or Built in) and the oldest Manang version it needs.

## 5. Updates

- Manang looks for updates when you open the Plugins view and at most once a day in the background. It **never installs anything by itself**.
- An installed plugin with a newer version shows **Update to x.y.z** on its row and page. **Update all** on the Installed tab goes through them one by one, each with its own consent window.
- If the new version asks for access the old one did not have, the consent window marks it **New** and the running version keeps running until you approve. If you decline, nothing changes.
- A version the registry withdrew after you installed it keeps running, and its page says so. Withdrawing stops new installs and updates; it does not remove your copy. Consider updating or uninstalling it.

## 6. Disable and uninstall

1. **Disable** (the switch on a row, or the button on the page) unloads a plugin but keeps its files and your approvals. **Enable** brings it back.
2. **Uninstall** (on the plugin page) asks for confirmation. The plugin stops running and its folder moves to the plugins trash folder (`plugins-trash` next to the plugins folder); nothing is deleted. Its approved access is forgotten.

## 7. Install from a file or folder

You can also install a plugin without the registry, for example one you built yourself.

1. In the Plugins view press **Install** (a `.manangplugin` archive) or the folder icon (a plugin folder).
2. Manang reads the plugin, shows its access in the same consent window, and installs it only when you approve.
3. Or place the plugin folder in your user configuration directory by hand:
   - **Linux:** `~/.config/manang/plugins/`
   - **macOS:** `~/Library/Application Support/manang/plugins/`
   - **Windows:** `%APPDATA%\manang\plugins\`
   The folder is named exactly after the plugin and holds `manifest.toml` and `plugin.wasm`. Manang detects it at launch and hot-reloads it when the files change.

A plugin installed this way is not covered by the registry's checks. Install only what you trust.

## 8. Understand plugin permissions

Sandboxed plugins must declare every capability they require in their `manifest.toml`. If a plugin tries to perform an ungranted operation, the sandbox halts the call. The Permissions tab and the consent window list them in plain language:

| You see | It means |
| --- | --- |
| Read the open document | The plugin can see the text of the document you have open (`buffer: read:active`). |
| Read and change the open document | It can also edit that text. Undo works as usual (`buffer: write:active`). |
| Read files in your project | It can read any file in the folder you have open (`filesystem: read:project`). |
| Read and change files in your project | It can also create and overwrite files there. Use Git to undo (`filesystem: write:project`). |
| Read / Read and change its own cache or settings folder | Only a folder Manang keeps for this plugin, not your documents. |
| Connect to api.example.org | It can send and receive data with that one server, over HTTPS, and nowhere else (`network`). |

A plugin that lists nothing can only run its own commands. Filesystem access is confined to the project directory (`..` is blocked), network hosts are exact names (no wildcards or raw IP addresses), and anything Manang cannot describe is shown as written, never hidden.

## 9. The registry address, and turning Browse off

Two settings (**Settings > Plugins**) control the registry:

- **Browse the plugin registry** (`plugins.browse`, on by default). Turn it off and Manang never contacts the registry: the Browse tab says it is off, no update checks run, and installing from the registry is refused. Installing from a file keeps working. An organisation can also switch this off by policy.
- **Plugin registry address** (`plugins.registry-url`, empty by default). Empty uses the address built into Manang. A self-hosted or company registry goes here; it must start with `https://`, and its list must be signed by a key your build of Manang trusts (a different registry needs a Manang build that embeds its key).

Privacy: fetching the list is a plain request that sends no identifier, account or list of your plugins. Download counters are counts per plugin; no address or personal data is stored.

## 10. What the indicator lines mean

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
