# Settings and Shortcuts

Open the Settings page, choose between user and project settings, tune the editor, preview and files options, and change or reset keyboard shortcuts.

## 1. Open the Settings page

1. Open Settings using any of these methods:
   - Press `Ctrl+,` (`Cmd+,`).
   - Click the **Settings** gear icon at the bottom of the Activity bar.
   - Choose **File > Settings** (`view.settings`) from the menu bar.
   - Press `Ctrl+K` (`Cmd+K`), type `Open settings`, and press `Enter`.
2. The Settings page has a list of sections on the left, in three views:
   - **Settings:** The application's options (**Appearance**, **Editor**, **Preview**, **Compile** and **Files**), **Collaboration**, and **Plugins**, where each plugin has its own section (for example **Writing**, **LaTeX**, **Citations**, **Search**, **Heal** and **Agent**).
   - **Keyboard shortcuts:** Searchable table of keybindings with custom shortcut recording.
   - **About:** Software version, commit, channel, system, what is new, and open-source licenses.
3. The search field in the top bar filters settings and plugins by name or description.
4. To return to your documents, click the close button at the top right of the page or press `Escape`.

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
- **User:** Saved globally in `settings.toml` in Manang's configuration folder (`~/.config/work.fundaments.manang/` on Linux, `~/Library/Application Support/work.fundaments.manang/` on macOS, `%APPDATA%\work.fundaments.manang\` on Windows). These settings apply to every project you open.
- **This project:** Saved in the project's `.manang/settings.toml` file, so they apply to everyone who opens the project. These settings apply only to that project and override user settings. When no project is open (such as on the Home screen), the toggle is not shown.

Recommended scoping guidelines:
- Use **User scope** for personal preferences like dark/light theme, UI scale, editor font size, and autosave behavior.
- Use **Project scope** for publication requirements like citation style (`apa` vs `numeric`), LaTeX compiler backend, and document format line width.

## 3. Configure application options

Type in the search field at the top of Settings to filter options by name, description, or setting ID.

1. **Appearance:**
   - **Theme (`appearance.theme`):** Switch between `dark` (the default), `light`, and `system` (follows operating system appearance). **View > Switch to light theme** (or dark) toggles it from the menu.
   - **UI scale (`appearance.ui-scale`):** Scale the user interface between 75% and 200%.
   - **Reduced motion (`appearance.reduced-motion`):** `system` (default) follows the operating system; `on` replaces transitions with instant changes; `off` keeps them.
   - **Custom theme (`appearance.custom-theme`):** The id of a theme from the `themes` folder of Manang's configuration folder, used instead of the built-in one. Empty for none.
   - **Coach lines (`appearance.coach-lines`):** Show short onboarding hints on the editor and preview.
   - **Reset layout:** A button that puts the side panel, bottom panel and preview split back to their default sizes.
2. **Editor:**
   - **Font size (`editor.font-size`):** Editor text size in pixels (default is 14 px).
   - **Line height (`editor.line-height`):** Vertical line spacing (default is 1.6).
   - **Tab size (`editor.tab-size`):** Indentation width in spaces (default is 2).
   - **Word wrap (`editor.word-wrap`):** Wrap long lines at the editor boundary.
   - **Line numbers (`editor.line-numbers`):** Toggle gutter line numbers.
   - **Vim mode (`editor.vim-mode`):** Enable modal Vim keybindings.
   - **Visual default (`editor.visual-default`):** File types (`markdown`, `typst`) that open in the Visual editor. See [The Visual Editor](visual-editor.md).
3. **Preview & Compile:**
   - **Auto-compile (`preview.auto-compile`):** Recompile preview documents automatically while typing.
   - **Debounce ms (`preview.debounce-ms`):** Milliseconds to wait after the last keystroke before compiling (default is 300 ms).
   - **Double click source (`preview.double-click-source`):** Double-click in the PDF goes to the source; `Alt`+double-click then selects a word.
   - **Dark pages (`preview.dark-pages`):** Show PDF pages inverted for reading in the dark. Export is unaffected.
   - **Engine (`compile.engine`):** Default compile engine for new projects (`typst`, `latex`, or `markdown`).
4. **Files:**
   - **Autosave (`files.autosave`):** Automatically save documents `after-delay`, `on-focus-loss`, or keep `off`.
   - **Trash on delete (`files.trash-on-delete`):** Listed in Settings, but in this version deleting a file always moves it to the project's `.manang/trash/` folder, whatever this setting says. See [The Trash](trash.md).
5. **Resetting settings:**
   - Any setting modified from its default value shows a modified indicator dot. Click the reset icon button beside the field to restore its default value.
6. Click **User settings file** (or **Project settings file**) at the bottom of the page to open the underlying TOML file. A few settings say they apply after a restart, and then a **Restart now** button appears.
7. Plugins add their own settings in their own sections. The ones you are most likely to need are described in [Writing](writing.md), [Preview and PDF](preview-and-pdf.md), [References and Bibliographies](references.md), [Search and Replace](search-and-replace.md), [Fixing Errors](fixing-errors.md), [AI Assistant](ai-assistant.md) and [Edit Together Live](edit-together-live.md).

## 4. Browse keyboard shortcuts

1. Press `Ctrl+/` (`Cmd+/`) from anywhere in Manang to open the **Keyboard shortcuts cheat sheet** overlay.
2. Shortcuts are categorized into groups such as the following. The overlay always shows the current keys, including any you changed:
   - **General:** Command palette (`Ctrl+K` (`Cmd+K`) or `Ctrl+Shift+P`), Quick open file (`Ctrl+P` (`Cmd+P`)), Shortcuts cheat sheet (`Ctrl+/` (`Cmd+/`)).
   - **Document & Files:** New file (`Ctrl+N` (`Cmd+N`)), Save (`Ctrl+S` (`Cmd+S`)), Save all (`Ctrl+Shift+S` (`Cmd+Shift+S`)), Export PDF (`Ctrl+E` (`Cmd+E`)), Format document (`Shift+Alt+F` (`Shift+Option+F`)).
   - **Views & Panels:** Files (`Ctrl+Shift+E` (`Cmd+Shift+E`)), Search (`Ctrl+Shift+F` (`Cmd+Shift+F`)), Toggle sidebar (`Ctrl+B` (`Cmd+B`)), Toggle bottom panel (`Ctrl+J` (`Cmd+J`)), Problems (`Ctrl+Shift+M` (`Cmd+Shift+M`)), Cycle preview (`Ctrl+Shift+V` (`Cmd+Shift+V`)), Zen mode (`Ctrl+\` (`Cmd+\`)).
   - **Tabs & Navigation:** Next tab (`Ctrl+Tab` or `Ctrl+PageDown`), Previous tab (`Ctrl+Shift+Tab` or `Ctrl+PageUp`), Close tab (`Ctrl+W` (`Cmd+W`)).
3. Type in the filter field to search for a specific keybinding. The same list is under **Help > Keyboard shortcuts**.
4. Press `Escape` or click outside the modal to dismiss the cheat sheet.
5. For a complete list of all menu items and shortcuts derived from the command registry, see [Menus and shortcuts](menus-and-shortcuts.md).

## 5. Customize keybindings

You can change existing shortcuts or assign key combinations to unbound commands.

1. Open Settings (`Ctrl+,` (`Cmd+,`)) and select the **Keyboard shortcuts** tab.
2. Search for the command you want to rebind.
3. Click the **Change** button on the command's row.
4. Press the key combination you want, for example `Ctrl+Shift+U` (`Cmd+Shift+U`). The row shows what it recorded: "Enter to apply, Esc to cancel".
5. Press `Enter` to apply it, or `Escape` to cancel.
6. If the shortcut is already in use by another command, Manang displays an inline conflict warning.
7. Modified shortcuts take effect immediately across the entire application; no restart is required. A row you changed is marked **Changed**.
8. To revert a customized keybinding back to the default shortcut, click the **Reset** button on the row.
9. Your changes are stored in `keybindings.toml` in Manang's configuration folder. The **Keybindings file** button at the top of the Keyboard shortcuts view opens it, and **Help > Change keyboard shortcuts** opens this view directly.
