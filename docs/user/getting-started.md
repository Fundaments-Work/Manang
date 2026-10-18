# Getting Started

You can run Manang from source, create and organize authoring projects on the Home screen, and navigate the main window layout.

## 1. Run Manang from source

Until pre-built installers are published, run Manang from source with the development toolchain.

1. Ensure your system meets the requirements:
   - **Rust:** Version 1.85 or later (MSRV 1.95).
   - **Node.js:** Version 22 or later.
   - **pnpm:** Version 10 or later.
   - **Linux system libraries:** If running Linux, install the WebKitGTK and GTK development packages:
     - On Ubuntu/Debian: `sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libglib2.0-dev`
     - On Fedora: `sudo dnf install webkit2gtk4.1-devel gtk3-devel libsoup3-devel glib2-devel`
     - On Arch Linux: `sudo pacman -S webkit2gtk-4.1 gtk3 libsoup3 glib2`
2. Open a terminal in the Manang repository root.
3. Install frontend dependencies:
   ```bash
   pnpm install
   ```
4. Start the desktop application:
   ```bash
   pnpm app:dev
   ```
   This compiles the native Rust backend and opens the desktop window with hot reloading enabled.
5. If you only want to test the web frontend in your browser without desktop bindings, run:
   ```bash
   pnpm dev
   ```
   Then open `http://127.0.0.1:1420` in your browser.

## 2. Use the Home view

The Home view lets you manage all your projects in one place. When you start Manang without an open project, Home opens automatically.

1. To open Home from inside a project:
   - Click the Manang mark icon in the top-left corner of the window.
   - Or press `Ctrl+Shift+H` (`Cmd+Shift+H`).
   - Or press `Ctrl+K` (`Cmd+K`), type `Go to Home`, and press `Enter`.
2. Browse your projects using the sections in the left navigation:
   - **Projects:** Shows all registered projects. Toggle between grid and list views with the view buttons at the top right.
   - **Recent:** Lists projects ordered by when you last opened them.
   - **Pinned:** Displays projects you have pinned for quick access.
   - **Archive:** Stores older projects you have hidden from the main list.
   - **Templates:** Provides starter documents for Typst, LaTeX, and Markdown. An expanded template library is coming soon.
   - Restoring removed projects with Home trash is coming soon.
3. Right-click any project card in Home to open, rename, edit tags, pin, archive, or remove it from the list.

## 3. Create a project

1. Open the Home screen.
2. Click **New project** (or run `New project` from the command palette `Ctrl+K` (`Cmd+K`)).
3. Type a project name in the dialog.
4. Select your document authoring engine:
   - **Typst:** Fast, modern markup language with instant sub-millisecond preview. Recommended for new documents.
   - **LaTeX:** Traditional TeX authoring. Best for journal submission templates and existing `.tex` files.
   - **Markdown:** Lightweight plain-text markup for notes and simple reports.
5. Choose the folder where Manang will create the project directory.
6. Click **Create**. Manang creates the folder, writes a starter document and bibliography file, and opens the project in the editor.

## 4. Open an existing project

1. Open the Home screen.
2. Click **Open folder**.
3. Use the system file picker to select a folder on your computer.
4. If the folder already contains a `manang.toml` file or scientific documents (`.typ`, `.tex`, `.md`), Manang detects the engine and opens the project.
5. To close the project and return to Home, press `Ctrl+K` (`Cmd+K`), type `Close project`, and press `Enter`.

## 5. Main window layout

The Manang workbench is divided into six functional areas:

1. **Top bar:**
   - Functions as the window drag handle and title bar.
   - Contains the Home mark button on the left, breadcrumbs showing the current project and active document, and window controls (minimize, maximize, close).
   - Press `/` to focus the search field when you are on Home or in Settings.
2. **Activity bar:**
   - The vertical icon rail on the far left.
   - Click any icon to open its sidebar panel, or click the active icon to collapse the sidebar:
     - **Files** (`Ctrl+Shift+E` (`Cmd+Shift+E`)): Project file tree.
     - **Search** (`Ctrl+Shift+F` (`Cmd+Shift+F`)): Find and replace text across files.
     - **Source control:** Git staging, commits, branches, and diffs.
     - **Outline:** Document headings and sections.
     - **Figures:** Vector figures in the project.
     - **References:** Bibliography entries and citations.
     - **Settings** (`Ctrl+,` (`Cmd+,`)): Application and project configuration.
3. **Sidebar:**
   - Displays the panel selected in the Activity bar.
   - Press `Ctrl+B` (`Cmd+B`) to toggle the sidebar open or closed.
4. **Editor area:**
   - The central workspace with tabbed documents.
   - Displays the CodeMirror 6 text editor for source files, the vector figure canvas for `.figure.json` files, and viewers for images and PDFs.
   - Switch between open tabs using `Ctrl+Tab` (`Cmd+Tab`) or `Ctrl+PageDown` / `Ctrl+PageUp`.
   - Close the active tab with `Ctrl+W` (`Cmd+W`).
5. **Preview pane:**
   - Appears to the right of the text editor when editing Typst, LaTeX, or Markdown documents.
   - Automatically recompiles and updates as you type.
   - Cycle preview layouts with `Ctrl+Shift+V` (`Cmd+Shift+V`).
6. **Bottom dock (Bottom panel):**
   - Contains two tabs:
     - **Problems** (`Ctrl+Shift+M` (`Cmd+Shift+M`)): Lists compiler errors, syntax warnings, diagram failures, and spelling suggestions. Click any problem to jump to that line in your document.
     - **Output:** Shows raw build and compiler logs.
   - Press `Ctrl+J` (`Cmd+J`) to toggle the bottom panel open or closed.
