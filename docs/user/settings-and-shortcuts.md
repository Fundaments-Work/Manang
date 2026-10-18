# Settings and Shortcuts

You can configure editor options, manage global versus project-scoped settings, review keyboard shortcuts, and assign custom keybindings.

## 1. Open the Settings page

1. Open Settings using any of these methods:
   - Press `Ctrl+,` (`Cmd+,`).
   - Click the **Settings** gear icon at the bottom of the Activity bar.
   - Choose **File > Settings** (`view.settings`) from the menu bar.
   - Press `Ctrl+K` (`Cmd+K`), type `Open settings`, and press `Enter`.
2. The Settings page offers three tabs:
   - **Settings:** Core application options and plugin configurations.
   - **Keyboard shortcuts:** Searchable table of keybindings with custom shortcut recording.
   - **About:** Software version, git commit hash, and open-source licenses.
3. To return to your documents, click any side panel icon in the Activity bar or press `Escape`.

## 2. Understand contextual settings and scoping

Settings in Manang adapt to your active context and support layered configuration:

### Contextual settings
1. When you open Settings while editing a document tab (for example, `main.typ` or `paper.tex`), Manang opens **contextual settings** scoped to that specific file (for example, `Settings for main.typ · Typst`).
2. Only sections relevant to the active document type are displayed (such as Editor, Preview, and the active compiler engine).
3. A scope banner appears across the top showing the current origin with a **Show all settings** button. Click this button to expand the view to all settings categories.
4. You can also open the full settings list directly at any time by choosing **File > All settings** from the menu bar or running `Open all settings` from the command palette.
5. To jump straight to compiler and formatting settings for the active file, choose **File > Document settings**.

### User versus Project scope
Locate the scope toggle at the top of the Settings view:
- **User scope:** Saved globally in your user profile directory (`~/.config/manang/settings.json` on Linux, `~/Library/Application Support/manang/settings.json` on macOS, `%APPDATA%\manang\settings.json` on Windows). These settings apply to every project you open.
- **This project:** Saved in the project's `.manang/settings.json` file. These settings apply only to the active project and override user settings. When no project is open (such as on the Home screen), the **This project** toggle is disabled.

Recommended scoping guidelines:
- Use **User scope** for personal preferences like dark/light theme, UI scale, editor font size, and autosave behavior.
- Use **Project scope** for publication requirements like citation style (`apa` vs `numeric`), LaTeX compiler backend, and document format line width.

## 3. Configure application options

Type in the search field at the top of Settings to filter options by name, description, or setting ID.

1. **Appearance:**
   - **Theme (`appearance.theme`):** Switch between `dark`, `light`, and `system` (follows operating system appearance).
   - **UI scale (`appearance.ui-scale`):** Scale the user interface between 75% and 200%.
   - **Reduced motion (`appearance.reduced-motion`):** Disable sliding animations and use instant transitions.
2. **Editor:**
   - **Font size (`editor.font-size`):** Editor text size in pixels (default is 14 px).
   - **Line height (`editor.line-height`):** Vertical line spacing (default is 1.6).
   - **Tab size (`editor.tab-size`):** Indentation width in spaces (default is 2).
   - **Word wrap (`editor.word-wrap`):** Wrap long lines at the editor boundary.
   - **Line numbers (`editor.line-numbers`):** Toggle gutter line numbers.
   - **Vim mode (`editor.vim-mode`):** Enable modal Vim keybindings.
3. **Preview & Compile:**
   - **Auto-compile (`preview.auto-compile`):** Recompile preview documents automatically while typing.
   - **Debounce delay (`preview.debounce-ms`):** Milliseconds to wait after the last keystroke before compiling (default is 300 ms).
   - **Default engine (`compile.engine`):** Default compiler for new projects (`typst`, `latex`, or `markdown`).
4. **Files:**
   - **Autosave (`files.autosave`):** Automatically save documents `after-delay`, `on-focus-loss`, or keep `off`.
   - **Trash on delete (`files.trash-on-delete`):** Send deleted files to the system trash instead of permanently removing them.
5. **Resetting settings:**
   - Any setting modified from its default value shows a modified indicator dot. Click the reset icon button beside the field to restore its default value.
6. Click **Open settings file** to open the underlying JSON configuration file in your system editor.

## 4. Browse keyboard shortcuts

1. Press `Ctrl+/` (`Cmd+/`) from anywhere in Manang to open the **Keyboard shortcuts cheat sheet** overlay.
2. Shortcuts are categorized into:
   - **General:** Command palette (`Ctrl+K` (`Cmd+K`)), Quick open file (`Ctrl+P` (`Cmd+P`)), Shortcuts cheat sheet (`Ctrl+/` (`Cmd+/`)).
   - **Document & Files:** New file (`Ctrl+N` (`Cmd+N`)), Save (`Ctrl+S` (`Cmd+S`)), Save all (`Ctrl+Shift+S` (`Cmd+Shift+S`)), Export PDF (`Ctrl+E` (`Cmd+E`)), Format document (`Shift+Alt+F` (`Shift+Option+F`)).
   - **Views & Panels:** Files (`Ctrl+Shift+E` (`Cmd+Shift+E`)), Search (`Ctrl+Shift+F` (`Cmd+Shift+F`)), Toggle sidebar (`Ctrl+B` (`Cmd+B`)), Toggle bottom panel (`Ctrl+J` (`Cmd+J`)), Problems (`Ctrl+Shift+M` (`Cmd+Shift+M`)), Cycle preview (`Ctrl+Shift+V` (`Cmd+Shift+V`)), Zen mode (`Ctrl+\` (`Cmd+\`)).
   - **Tabs & Navigation:** Next tab (`Ctrl+Tab` or `Ctrl+PageDown`), Previous tab (`Ctrl+Shift+Tab` or `Ctrl+PageUp`), Close tab (`Ctrl+W` (`Cmd+W`)).
3. Type in the filter field to search for a specific keybinding.
4. Press `Escape` or click outside the modal to dismiss the cheat sheet.
5. For a complete list of all menu items and shortcuts derived from the command registry, see [Menus and shortcuts](menus-and-shortcuts.md).

## 5. Customize keybindings

You can change existing shortcuts or assign key combinations to unbound commands.

1. Open Settings (`Ctrl+,` (`Cmd+,`)) and select the **Keyboard shortcuts** tab.
2. Search for the command you want to rebind.
3. Click the shortcut box or edit button on the command row.
4. A recording prompt asks you to press your desired key combination.
5. Press the keys on your keyboard (for example, `Ctrl+Shift+U` or `Cmd+Shift+U`).
6. If the shortcut is already in use by another command, Manang displays an inline conflict warning.
7. Modified shortcuts take effect **immediately** across the entire application—no restart or reload is required.
8. To revert a customized keybinding back to the default shortcut, click the **Reset** button on the row.
