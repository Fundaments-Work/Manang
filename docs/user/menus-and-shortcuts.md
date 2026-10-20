# Menus and Shortcuts

Look up every menu in Manang and its default keyboard shortcut, learn the Alt-key mnemonics, and find the keys used inside the figure studio and the PDF viewer.

## 1. Access the menu bar

The menu bar is located in the window title bar on Linux, Windows, and in the web browser. On macOS, these commands appear in the system application menu bar at the top of your screen.

1. **Mouse navigation:** Click any menu title to open its dropdown list. Hover over adjacent menus to switch between them.
2. **Mnemonic shortcuts (`Alt+letter`):** Press `Alt` together with the underlined mnemonic letter:
   - `Alt+F`: Open the **File** menu
   - `Alt+E`: Open the **Edit** menu
   - `Alt+V`: Open the **View** menu
   - `Alt+G`: Open the **Go** menu
   - `Alt+B`: Open the **Build** menu
   - `Alt+H`: Open the **Help** menu
3. **Keyboard navigation (`F10`):**
   - Press `F10` to move keyboard focus directly to the menu bar.
   - Press `Left Arrow` or `Right Arrow` to move between menus.
   - Press `Down Arrow` or `Up Arrow` to highlight items in an open menu.
   - Press `Enter` or `Space` to run the highlighted command.
   - Press `Escape` to close the menu and return focus to your document.
4. **Compact window mode:** When the window width is narrower than 900 pixels, the six individual menus collapse into a single **Menu** button. Clicking this button or pressing `F10` opens a menu listing all six categories, with submenus opening to the side.

---

## 2. Complete menu reference

Below is every menu in Manang with its items, groups, and default shortcuts.

### File menu (`Alt+F`)

| Item | Command ID | Default shortcut | Notes |
| :--- | :--- | :--- | :--- |
| **New project** | `home.new-project` | — | Opens the Home view with the New Project dialog. |
| **New file** | `file.new` | `Ctrl+N` (`Cmd+N`) | Creates a new file in the current project directory. |
| **New figure** | `file.new-figure` | — | Creates a new vector figure (`.figure`). |
| **New diagram** | `file.new-diagram` | — | Creates a new text diagram (Mermaid, Pikchr, ASCII). |
| *— Separator —* | | | |
| **Open folder…** | `project.open-folder` | — | Opens a project folder using the system file picker. |
| **Switch project…** | `project.switch` | `Ctrl+O` (`Cmd+O`) | Opens the recent project switcher palette. |
| *— Separator —* | | | |
| **Save** | `file.save` | `Ctrl+S` (`Cmd+S`) | Saves modifications in the active editor tab. |
| **Save all** | `file.save-all` | `Ctrl+Shift+S` (`Cmd+Shift+S`) | Saves all modified documents across the project. |
| **Save a version** | `file.save-version` | `Ctrl+Alt+S` (`Cmd+Option+S`) | Saves a named version of the whole project. See [Version History and Restore](version-history.md). |
| *— Separator —* | | | |
| **Share…** | `share.open` | `Ctrl+Alt+L` (`Cmd+Option+L`) | Opens the Share popover at the end of the tab strip: edit together live, share versions, invite link. |
| **Share this file live…**, **Join a live session…**, **Copy live invite**, **Leave live session** | `live.*` | — | The same live actions, straight from the menu. See [Edit together live](edit-together-live.md). |
| *— Separator —* | | | |
| **Export PDF** | `doc.export-pdf` | `Ctrl+E` (`Cmd+E`) | Compiles and exports a PDF from Typst or LaTeX. |
| *— Separator —* | | | |
| **Close project** | `project.close` | — | Closes the active project and returns to Home. |
| *— Separator —* | | | |
| **Settings** | `view.settings` | `Ctrl+,` (`Cmd+,`) | Opens settings scoped to the active file or document. |
| **All settings** | `settings.all` | — | Opens the full application and project settings view. |
| **Document settings** | `settings.document` | — | Opens compiler and preview settings for active file. |
| *— Separator —* | | | |
| **Quit** | `app.quit` | `Ctrl+Q` (`Cmd+Q`) | Closes Manang (desktop application only). |

### Edit menu (`Alt+E`)

| Item | Command ID | Default shortcut | Notes |
| :--- | :--- | :--- | :--- |
| **Undo** | `edit.undo` | `Ctrl+Z` (`Cmd+Z`) | Reverts the last text or canvas modification. |
| **Redo** | `edit.redo` | `Ctrl+Shift+Z` (`Cmd+Shift+Z` or `Ctrl+Y`) | Re-applies the previously undone action. |
| *— Separator —* | | | |
| **Cut** | `edit.cut` | `Ctrl+X` (`Cmd+X`) | Cuts selected text or figure elements to clipboard. |
| **Copy** | `edit.copy` | `Ctrl+C` (`Cmd+C`) | Copies selected text or figure elements to clipboard. |
| **Paste** | `edit.paste` | `Ctrl+V` (`Cmd+V`) | Pastes clipboard content into document or canvas. |
| *— Separator —* | | | |
| **Select all** | `edit.select-all` | `Ctrl+A` (`Cmd+A`) | Selects all text or all elements on the active canvas. |
| *— Separator —* | | | |
| **Find** | `edit.find` | `Ctrl+F` (`Cmd+F`) | Opens the in-document or in-PDF find bar. |
| **Replace** | `edit.replace` | — | Opens search and replace in the active editor. |
| **Search in project** | `view.search` | `Ctrl+Shift+F` (`Cmd+Shift+F`) | Opens the Search sidebar panel for project-wide text search. See [Search and Replace](search-and-replace.md). |
| *— Separator —* | | | |
| **Rewrite selection**, **Shorten selection**, **Translate selection**, **Explain selection** | `agent.rewrite`, `agent.shorten`, `agent.translate`, `agent.explain` | — | Open the AI assistant and send that command for the selected text. See [AI Assistant](ai-assistant.md). |
| *— Separator —* | | | |
| **Format document** | `doc.format` | `Shift+Alt+F` (`Shift+Option+F`) | Tidies whitespace and indentation in the active file. |
| **Copy include line** | `doc.copy-include` | — | Copies inclusion markup for active figure or diagram. |
| **Convert to Typst** | `doc.convert-typst` | — | Converts the active Markdown document into a Typst file. |

### View menu (`Alt+V`)

| Item | Command ID | Default shortcut | Notes |
| :--- | :--- | :--- | :--- |
| **Command palette** | `palette.commands` | `Ctrl+K` (`Cmd+K`) or `Ctrl+Shift+P` | Opens the searchable command launcher. |
| *— Separator —* | | | |
| **Files** | `view.files` | `Ctrl+Shift+E` (`Cmd+Shift+E`) | Shows the project file tree panel in the sidebar. |
| **Search** | `view.search` | `Ctrl+Shift+F` (`Cmd+Shift+F`) | Shows the text search and replace panel in the sidebar. |
| **Source control** | `view.source-control` | — | Shows Git status, staging, branches, and diffs. |
| **Versions** | `view.versions` | — | Shows the version timeline in the Source control panel. |
| **Outline** | `view.outline` | — | Shows document headings and hierarchical structure. |
| **Figures** | `view.figures` | — | Shows vector figures in the project. |
| **References** | `view.references` | — | Shows bibliography entries and citation tools. |
| **Plugins** | `view.plugins` | `Ctrl+Shift+X` (`Cmd+Shift+X`) | Shows installed plugins and permissions. |
| **Agent** | `agent.toggle` | `Ctrl+Shift+A` (`Cmd+Shift+A`) | Opens or closes the AI agent chat column on the right. See [AI Assistant](ai-assistant.md). |
| *— Separator —* | | | |
| **Toggle side panel** | `view.toggle-side` | `Ctrl+B` (`Cmd+B`) | Expands or collapses the active sidebar panel. |
| **Toggle panel** | `view.toggle-panel` | `Ctrl+J` (`Cmd+J`) | Expands or collapses the bottom panel (Problems/Output). |
| *— Separator —* | | | |
| **Editor and preview** | `view.preview-split` | — | Sets split layout (editor on left, preview on right). |
| **Editor only** | `view.preview-editor` | — | Hides the preview pane to maximize editor space. |
| **Preview only** | `view.preview-preview` | — | Maximizes the preview pane across the workspace. |
| **Cycle preview layout** | `view.cycle-preview` | `Ctrl+Shift+V` (`Cmd+Shift+V`) | Cycles through Split, Editor only, and Preview only. |
| **Visual editor** | `view.visual` | `Ctrl+Alt+V` (`Cmd+Option+V`) | Switches a Markdown or Typst file between Source and Visual. See [The Visual editor](visual-editor.md). |
| *— Separator —* | | | |
| **Zen mode** | `view.zen` | `Ctrl+\` (`Cmd+\`) | Toggles distraction-free reading column. |
| **Switch to light/dark theme** | `view.theme` | — | Switches between Dark and Light color themes. |
| **Slides** | `doc.slides` | — | Toggles between text and slides view for Typst. See [Slides and Presentations](slides.md). |
| *— Separator (when figure is active) —* | | | |
| **Zoom figure to fit** | `figure.zoom-fit` | `Shift+1` | Zooms the figure canvas to fit all elements. |
| **Figure: Insert Mermaid** | `figure.insert-mermaid` | — | Opens dialog to insert a Mermaid diagram as shapes. |

### Go menu (`Alt+G`)

| Item | Command ID | Default shortcut | Notes |
| :--- | :--- | :--- | :--- |
| **Go to file** | `goto.file` | `Ctrl+P` (`Cmd+P`) | Quick-opens files across the project with fuzzy matching. |
| **Home** | `home.show` | `Ctrl+Shift+H` (`Cmd+Shift+H`) | Opens the Home view project manager. |
| *— Separator —* | | | |
| **Next tab** | `tab.next` | `Ctrl+Tab` or `Ctrl+PageDown` | Cycles forward through open editor tabs. |
| **Previous tab** | `tab.previous` | `Ctrl+Shift+Tab` or `Ctrl+PageUp` | Cycles backward through open editor tabs. |
| *— Separator —* | | | |
| **Close tab** | `tab.close` | `Ctrl+W` (`Cmd+W`) | Closes the currently active editor tab. |
| **Close other tabs** | `tab.close-others` | — | Closes all open tabs except the active one. |
| *— Separator —* | | | |
| **Show in preview** | `doc.show-in-preview` | `Ctrl+Alt+J` (`Cmd+Option+J`) | Performs forward SyncTeX jump to matching preview position. |

*Tab jumping shortcuts:* Press `Ctrl+1` through `Ctrl+8` (`Cmd+1`–`Cmd+8`) to switch directly to tabs 1 through 8. Press `Ctrl+9` (`Cmd+9`) to jump to the last open tab.

### Build menu (`Alt+B`)

| Item | Command ID | Default shortcut | Notes |
| :--- | :--- | :--- | :--- |
| **Compile** | `doc.compile` | `Ctrl+Shift+B` (`Cmd+Shift+B`) | Triggers an immediate recompile of the document. |
| **Export PDF** | `doc.export-pdf` | `Ctrl+E` (`Cmd+E`) | Exports a publication-ready PDF to disk. |
| *— Separator —* | | | |
| **Present slides** | `doc.present` | `F5` | Enters full-screen presentation mode for Typst slides. |
| *— Separator —* | | | |
| **Problems** | `view.problems` | `Ctrl+Shift+M` (`Cmd+Shift+M`) | Opens the Problems panel in the bottom dock. |
| **Output** | `view.output` | — | Opens the compiler and build log output panel. |

### Help menu (`Alt+H`)

| Item | Command ID | Default shortcut | Notes |
| :--- | :--- | :--- | :--- |
| **User guide** | `help.user-guide` | — | Opens this user guide inside Manang, with search. |
| **Keyboard shortcuts** | `help.shortcuts` | `Ctrl+/` (`Cmd+/`) | Opens the searchable shortcuts cheat sheet overlay. |
| **Change keyboard shortcuts** | `help.keybindings` | — | Opens Settings directly on the Keyboard shortcuts tab. |
| *— Separator —* | | | |
| **Report a problem** | `help.report-problem` | — | Opens a dialog where you describe the problem and read a diagnostics report to copy. Manang sends nothing itself. |
| **Send feedback** | `help.send-feedback` | — | The same dialog, for suggestions and ideas. |
| **Open logs** | `help.open-logs` | — | Reveals the application logs folder in your system file manager. |
| *— Separator —* | | | |
| **Toggle developer tools** | `dev.inspect` | `F12` or `Ctrl+Shift+I` | Opens the web inspector (development builds only). |
| *— Separator —* | | | |
| **About Manang** | `help.about` | — | Displays version, commit, channel, system, what is new, and licenses. |

---

## 3. Surface-specific shortcuts

In addition to menu commands, specific editing surfaces handle specialized keys directly.

### Vector figure studio

When editing a figure (`.figure` or legacy `.figure.json`) on the canvas:

| Action | Shortcut | Description |
| :--- | :--- | :--- |
| **Select tool** | `V` (or `1`) | Select, move, and transform shapes on the canvas. |
| **Hand tool** | `H` (or `2`) | Pan across the canvas without selecting shapes. |
| **Rectangle tool** | `R` (or `3`) | Draw rectangle shapes and boxes. |
| **Diamond tool** | `D` (or `4`) | Draw decision diamonds for flowcharts. |
| **Ellipse tool** | `O` (or `5`) | Draw circles and ellipses. |
| **Arrow tool** | `A` (or `6`) | Draw directional arrows that bind to shape anchors. |
| **Line tool** | `L` (or `7`) | Draw straight connector lines without arrowheads. |
| **Draw tool** | `P` or `B` (or `8`) | Freehand ink drawing with tablet pressure sensitivity. |
| **Text tool** | `T` (or `9`) | Insert editable text labels on the canvas. |
| **Frame tool** | `F` | Draw containers that group and organize nested shapes. |
| **Eraser tool** | `E` | Click or drag across shapes to erase them. |
| **Laser tool** | `K` | Temporary laser pointer trail for live presentations. |
| **Formula tool** | `M` | Insert mathematical formula (Typst or LaTeX notation). |
| **Nudge selection** | `Arrow keys` | Nudges selected shapes by 1 point (`Shift+Arrow` for 10 points). |
| **Move without snap** | Hold `Alt` | Drag shapes freely without snapping to anchor points. |
| **Duplicate** | `Ctrl+D` (`Cmd+D`) | Clones selected elements with a slight offset. |
| **Group / Ungroup** | `Ctrl+G` / `Ctrl+Shift+G` | Groups selection into a single unit or ungroups. |
| **Lock / Unlock** | `Ctrl+Shift+L` (`Cmd+Shift+L`) | Locks elements to prevent accidental moves. |
| **Z-Order Front / Back** | `Ctrl+Shift+]` / `Ctrl+Shift+[` | Moves selected shapes to very top or bottom. |
| **Z-Order Step** | `Ctrl+]` / `Ctrl+[` | Moves selected shapes forward or backward one step. |
| **Zoom in / Zoom out** | `Ctrl+=` / `Ctrl+-` | Zooms the canvas view in or out. |
| **Zoom 100%** | `Ctrl+0` | Resets canvas zoom to 100% original scale. |
| **Zoom to fit** | `Shift+1` | Adjusts zoom and pan to fit all shapes on screen. |
| **Zoom to selection** | `Shift+2` | Centers and zooms to the selected shapes. |
| **Pan canvas** | `Space` + drag | Hold spacebar and drag with left mouse button (or middle-click drag). |
| **Edit label** | `Enter` or double-click | Opens inline text editor on the selected shape. |
| **Delete selection** | `Delete` or `Backspace` | Removes selected shapes from the figure scene. |

### PDF preview viewer

When previewing a compiled document or viewing a `.pdf` file:

| Action | Control / Shortcut | Description |
| :--- | :--- | :--- |
| **Select & copy text** | Click & drag, then `Ctrl+C` | Copies selected text as plain text with clean spacing. |
| **Find in PDF** | `Ctrl+F` (`Cmd+F`) | Opens the PDF find bar with live match counts. |
| **Jump to source** | Double-click or `Ctrl+click` | Jumps to corresponding line in the source editor. |
| **Select word** | `Alt+double-click` | Selects a single word without triggering a source jump. |
| **Zoom in / out** | `Ctrl+=` / `Ctrl+-` | Steps the document zoom level up or down. |
| **Fit page width** | `Ctrl+0` (`Cmd+0`) | Fits the document page width to the preview pane. |
| **Pointer zoom** | `Ctrl+wheel` (or pinch) | Anchors continuous zoom directly at the mouse cursor. |
| **Zoom presets** | Zoom dropdown | Presets: Fit page width, 50%, 75%, 100%, 125%, 150%, 200%. |
| **Page box** | Input field in header | Type a page number and press `Enter` to jump directly. |
| **Previous / Next page** | Arrow buttons in header | Steps backward or forward one page. |
| **Document outline** | Outline button in header | Toggles collapsible PDF heading outline. |
| **Dark pages** | Invert button in header | Inverts page colors for high-contrast reading at night. |
| **Follow links** | Click link | Navigates internal cross-references or opens external URLs. |
