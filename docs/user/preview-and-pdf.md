# Preview and PDF

You can preview compiled documents beside your code, switch layout modes, jump bidirectionally between source text and rendered output with SyncTeX, export publication-grade PDFs, and configure your LaTeX TeX environment.

## 1. Control preview layouts

The preview pane shows the rendered document output (SVG pages for Typst, PDF rendering for LaTeX, and formatted HTML for Markdown).

1. Select your preferred preview layout:
   - **Split layout:** Displays the source editor on the left and the preview on the right.
   - **Editor only (Hide preview):** Hides the preview pane so the editor occupies the full workspace width.
   - **Preview only (Maximize preview):** Maximizes the preview pane across the full editor area.
2. To cycle between these three layouts:
   - Press `Ctrl+Shift+V` (`Cmd+Shift+V`).
   - Or click the preview layout button located on the right side of the tab bar.
   - Or open the command palette (`Ctrl+K` (`Cmd+K`)) and run `Cycle preview layout`.
3. To adjust the split width:
   - Click and drag the vertical divider line between the editor and preview panes.
4. Additional tools for the PDF preview viewer are coming soon.

## 2. Live recompile

Manang recompiles your document incrementally in the background as you write.

1. When you type in a Typst, LaTeX, or Markdown file, Manang waits for a short pause before triggering a build.
2. If compilation succeeds, the preview updates immediately without losing your scroll position.
3. If an error occurs, the preview keeps displaying the last successful build, and diagnostics appear in the **Problems** tab of the bottom panel (`Ctrl+Shift+M` (`Cmd+Shift+M`)).
4. To trigger a manual rebuild at any time:
   - Open the command palette (`Ctrl+K` (`Cmd+K`)), type `Compile now`, and press `Enter`.
5. To configure auto-compilation:
   - Open Settings (`Ctrl+,` (`Cmd+,`)).
   - Toggle **Recompile automatically while typing** (`preview.auto-compile`).
   - Adjust the delay in milliseconds under **Compile debounce** (`preview.debounce-ms`, default is `300` ms).

## 3. Bidirectional SyncTeX navigation

SyncTeX lets you jump seamlessly between lines in your source code and positions in the rendered preview.

### Jump from preview to source (Backward sync)
1. Move your mouse over the preview pane.
2. Click any paragraph, heading, list item, or equation on the rendered page.
3. The editor immediately focuses and places your caret on the exact source line that generated that content.

### Jump from source to preview (Forward sync)
1. Place your cursor anywhere in the text editor.
2. Press `Ctrl+Alt+J` (`Cmd+Alt+J`).
   - Or open the command palette (`Ctrl+K` (`Cmd+K`)), type `Show in preview`, and press `Enter`.
3. The preview pane scrolls to the corresponding page and highlights the matching position.

## 4. Export PDF

You can export standalone, publication-ready PDF documents from Typst and LaTeX files.

1. Open the Typst or LaTeX document you want to publish.
2. Press `Ctrl+E` (`Cmd+E`).
   - Or open the command palette (`Ctrl+K` (`Cmd+K`)), type `Export PDF`, and press `Enter`.
3. A native system save dialog appears.
4. Select the folder location, enter the file name, and click **Save**.
5. Manang compiles the document and writes the PDF file directly to disk.
6. *Note for Markdown files:* Markdown files generate HTML previews. To export a paginated PDF from Markdown, run `Convert to Typst` (`Ctrl+K` (`Cmd+K`)) to produce a `.typ` document, then export the PDF.

## 5. Install TeX Live for LaTeX

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

### Configure LaTeX settings in Manang
1. Open Settings (`Ctrl+,` (`Cmd+,`)).
2. Scroll to the **LaTeX** section:
   - **Backend:** Keep set to `auto` to use the system TeX installation.
   - **Engine:** Choose `pdflatex`, `xelatex`, or `lualatex` according to your document requirements.
   - **Use latexmk:** Keep enabled (default `true`) so bibliographies and cross-references resolve automatically over multiple compiler passes.
   - **Compile timeout:** Adjust the maximum build time in seconds (default `60` seconds) to prevent runaway macros from hanging the process.
3. An optional built-in Tectonic worker engine can also be compiled locally using `cargo build -p manang-latex --features engine --bin manang-latex-worker`.
