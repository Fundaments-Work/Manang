# Preview and PDF

Preview your document beside the editor, jump between source and page with SyncTeX, use the PDF viewer, export a PDF and set up a LaTeX engine.

## 1. Control preview layouts

The preview pane shows the rendered document output (pages for Typst and LaTeX, and a formatted preview for Markdown). Its header holds the page, zoom and find controls for paged documents, a refresh button, an **Export PDF** button, and the layout buttons.

1. Select your preferred preview layout:
   - **Split layout:** Displays the source editor on the left and the preview on the right.
   - **Editor only (Hide preview):** Hides the preview pane so the editor occupies the full workspace width.
   - **Preview only (Maximize preview):** Maximizes the preview pane across the full editor area.
2. To cycle between these three layouts:
   - Press `Ctrl+Shift+V` (`Cmd+Shift+V`).
   - Or choose **View > Cycle preview layout** from the menu bar.
   - Or pick one layout directly from the **View** menu: **Editor and preview**, **Editor only** or **Preview only**.
   - Or use the buttons in the preview header: **Maximise preview** (it becomes **Restore split**) and **Close preview**. After you close the preview, a **Show preview** button appears at the right of the tab strip to bring it back.
   - In a window narrower than 900 pixels the preview takes the whole editor area instead of sitting beside it.
3. To adjust the split width:
   - Click and drag the vertical divider line between the editor and preview panes.

## 2. Use the built-in PDF viewer

When compiling documents or opening standalone `.pdf` files, Manang uses a fast, continuous PDF viewer:

1. **Select and copy text:**
   - Click and drag over any text in the preview pane to select it.
   - Press `Ctrl+C` (`Cmd+C`) to copy the selected text to your clipboard. Manang normalizes whitespace and line breaks cleanly.
2. **Find text (`Ctrl+F`):**
   - Press `Ctrl+F` (`Cmd+F`) while focused in the PDF viewer to open the find bar (or click the magnifying glass icon in the preview header).
   - Type your search query. The viewer highlights all matches and displays the current match index and total match count (e.g. `1 of 14`).
   - Press `Enter` or click the down arrow to jump to the next match; press `Shift+Enter` or click the up arrow to move to the previous match.
   - Use the **Match case** button to make the search case-sensitive.
   - Press `Escape` to close the find bar.
3. **Follow links:**
   - Click internal citation references or section links to scroll directly to the target destination.
   - Click external web links to open them safely in your system browser.
4. **Navigate with the document outline:**
   - If the document contains a PDF table of contents, click the **Outline** button in the preview header.
   - An outline drawer opens displaying all document sections and subsections. Click any heading to jump directly to that page.
5. **Zoom controls:**
   - **Zoom in / out:** Press `Ctrl+=` (`Cmd+=`) to zoom in, and `Ctrl+-` (`Cmd+-`) to zoom out (or click the `+` and `−` buttons in the preview header).
   - **Fit page width:** Press `Ctrl+0` (`Cmd+0`) to adjust zoom so pages fit the width of your preview pane.
   - **Pointer zoom:** Hold `Ctrl` (or `Cmd`) and scroll your mouse wheel, or pinch on a trackpad. Zoom anchors dynamically at your pointer position.
   - **Zoom presets:** Click the percentage indicator in the preview header to choose from presets: *Fit page width*, *50%*, *75%*, *100%*, *125%*, *150%*, or *200%*.
6. **Dark pages for night reading:**
   - Click the **Dark pages** (invert) button in the preview header.
   - Manang smoothly inverts the rendered page colors while preserving diagram contrast, reducing eye strain in dark environments.
7. **Page box navigation:**
   - The preview header displays your current position and total pages (`<page> / <total>`).
   - Click the page number input, type a page number, and press `Enter` to jump directly to that page.
   - Click the previous and next arrow buttons to step through pages.

## 3. Bidirectional SyncTeX navigation

SyncTeX lets you jump seamlessly between lines in your source code and positions in the rendered preview.

### Jump from preview to source (Backward sync)
1. Double-click anywhere on a paragraph, heading, list item, or equation on the rendered page (or hold `Ctrl`/`Cmd` and click).
2. The editor immediately focuses and places your caret on the exact source line that generated that content.
3. *Tip:* To select a word in the PDF without triggering a jump to source, hold `Alt` (or `Option`) and double-click the word.

### Jump from source to preview (Forward sync)
1. Place your cursor anywhere in the text editor.
2. Press `Ctrl+Alt+J` (`Cmd+Option+J`).
   - Or choose **Go > Show in preview** from the menu bar.
   - Or open the command palette (`Ctrl+K` (`Cmd+K`)), type `Show in preview`, and press `Enter`.
3. The preview pane scrolls to the corresponding page and highlights the matching position.

## 4. Live recompile

Manang recompiles your document incrementally in the background as you write.

1. When you type in a Typst, LaTeX, or Markdown file, Manang waits for a short pause before triggering a build.
2. If compilation succeeds, the preview updates immediately without losing your scroll position.
3. If an error occurs, the preview keeps displaying the last successful build, and diagnostics appear in the **Problems** tab of the bottom panel (`Ctrl+Shift+M` (`Cmd+Shift+M`)).
4. To trigger a manual rebuild at any time:
   - Press `Ctrl+Shift+B` (`Cmd+Shift+B`).
   - Or choose **Build > Compile** from the menu bar.
5. To configure auto-compilation:
   - Open Settings (`Ctrl+,` (`Cmd+,`)).
   - Toggle **Auto compile** (`preview.auto-compile`), which recompiles while you type. With it off, press `Ctrl+Shift+B` (`Cmd+Shift+B`) or the refresh button in the preview header to compile.
   - Adjust the delay in milliseconds under **Debounce ms** (`preview.debounce-ms`, default is `300` ms).

## 5. Export PDF

You can export standalone, publication-ready PDF documents from Typst and LaTeX files.

1. Open the Typst or LaTeX document you want to publish.
2. Press `Ctrl+E` (`Cmd+E`).
   - Or click **Export PDF** in the preview header.
   - Or choose **File > Export PDF** (or **Build > Export PDF**) from the menu bar.
   - Or open the command palette (`Ctrl+K` (`Cmd+K`)), type `Export PDF`, and press `Enter`.
3. A native system save dialog appears.
4. Select the folder location, enter the file name, and click **Save**.
5. Manang compiles the document and writes the PDF file directly to disk.
6. *Note for Markdown files:* Markdown files generate HTML previews. To export a paginated PDF from Markdown, run `Convert to Typst` (`Edit > Convert to Typst`) to produce a `.typ` document, then export the PDF.

## 6. Install TeX Live for LaTeX

Typst compiles using Manang's built-in Rust engine and requires no external tools. Compiling LaTeX documents requires a TeX distribution installed on your system.

### Install TeX Live on Linux
1. **Ubuntu / Debian:**
   ```bash
   sudo apt install texlive-latex-extra texlive-fonts-recommended latexmk
   ```
   For a complete distribution including all CTAN packages, install `texlive-full`:
   ```bash
   sudo apt install texlive-full
   ```
2. **Fedora:**
   ```bash
   sudo dnf install texlive-scheme-medium latexmk
   ```
3. **Arch Linux:**
   ```bash
   sudo pacman -S texlive-meta
   ```

### Install MacTeX on macOS
1. If you use Homebrew, install the terminal distribution:
   ```bash
   brew install --cask mactex-no-gui
   ```
2. Alternatively, download the full MacTeX graphical installer from [tug.org/mactex](https://tug.org/mactex/).

### Install TeX Live or MiKTeX on Windows
1. Download the TeX Live installer from [tug.org/texlive](https://tug.org/texlive/).
2. Alternatively, download MiKTeX from [miktex.org](https://miktex.org/).
3. Run the installer and ensure the option to add TeX binaries to your system `PATH` is enabled.

### No TeX installed? Use the built-in engine
If Manang finds no TeX on your computer, the preview, the **Problems** tab and the **Output** tab say "No TeX installation found" and offer **Download the built-in engine (about 48 MB)**. It is optional, and nothing is downloaded until you agree. In this beta the built-in engine is available on Linux (x86-64 and arm64); on Windows and macOS, install TeX Live, MiKTeX or MacTeX instead.


1. Press the button. A window tells you exactly what will happen: the server the engine comes from, its size, its SHA-256 checksum, and the folder it will be saved in.
2. Press **Download**. A progress line shows the download; **Cancel** stops it and leaves nothing behind. If the connection drops, press **Try again**: it continues where it stopped.
3. Manang checks the file against the checksum it ships with. If it does not match, nothing is installed. It also starts the engine once to make sure it runs on your computer.
4. When it says the engine is ready, your document compiles at once.

Things to know about the built-in engine:
- **Your own TeX comes first.** In the default setting (**Automatic**), Manang uses the TeX Live or MiKTeX it finds and only uses the built-in engine when there is none.
- **It is XeTeX (Tectonic).** It handles most articles, theses and Beamer slides. It does not run `pdflatex`-only packages or LuaLaTeX, and it has no `biber` (BibTeX works). If a document needs those, install TeX Live.
- **Packages come from the internet, once.** The first time a document uses a LaTeX package, the engine fetches it from `relay.fullyjustified.net` (the Tectonic project's package server) and keeps it on your computer. The first compile of a new document can therefore take a minute or more; later compiles need no network. To work fully offline, turn off **Fetch LaTeX packages on demand** in Settings under **LaTeX**; packages you never used then show up as "file not found".
- **Where it lives.** The engine is in Manang's data folder, under `engines/tectonic-<version>/` (Linux: `~/.local/share/work.fundaments.manang/`; macOS: `~/Library/Application Support/work.fundaments.manang/`; Windows: `%APPDATA%\work.fundaments.manang\`). Downloaded packages are in Manang's cache folder under `latex/`.
- **Remove it any time:** Settings > **LaTeX** > **Remove the built-in engine**. Your documents are never touched.

### Configure LaTeX settings in Manang
1. Open Settings (`Ctrl+,` (`Cmd+,`)).
2. Under **Plugins**, open the **LaTeX** section:
   - **LaTeX engine:** **Automatic** uses your TeX installation and falls back to the built-in engine. **System TeX** uses only your installation. **Built-in engine** uses only the downloaded one.
   - **Engine:** Choose `pdflatex`, `xelatex`, or `lualatex` for your own TeX installation (the built-in engine always uses XeTeX).
   - **Built-in engine** (below the settings): shows whether it is installed and which version, lets you download or remove it, and says which engine the last compile used. The Compile button's tooltip also names it (for example "xelatex, built-in engine").
