# Using Manang as an Offline LaTeX Editor

Compile LaTeX in Manang with a TeX distribution on your computer or the optional built-in engine, and see what works with no internet and what needs it.

Manang edits LaTeX files in a project folder on your disk and shows the compiled PDF beside the source. It does not include a TeX distribution. It compiles LaTeX in one of two ways, and which one you use decides what needs the internet.

## How does LaTeX compile in Manang?

1. **A TeX distribution on your computer.** Manang finds TeX Live, MiKTeX or MacTeX on your `PATH` and runs it. This works on Windows, macOS and Linux.
2. **The optional built-in engine.** If Manang finds no TeX, it offers to download an engine. In this beta it exists only for Linux on x86-64 and arm64; builds for Windows and macOS are marked planned in the engine manifest (`engines/manifest.json`) and not available yet.

In Settings, under **Plugins > LaTeX**, the **LaTeX engine** setting chooses between them:

- **Automatic** (the default) uses your TeX installation and falls back to the built-in engine only when there is none.
- **System TeX** uses only your installation.
- **Built-in engine** uses only the downloaded engine.

The Compile button's tooltip and the **Built-in engine** section of the settings say which engine the last compile used.

## Which TeX distribution should I install?

TeX Live is a comprehensive TeX system that supports GNU/Linux, macOS and Windows and provides the TeX, pdfTeX, XeTeX and LuaTeX engines ([CTAN: texlive](https://ctan.org/pkg/texlive)). MacTeX is TeX Live with Mac-specific software, a download of more than 5 GB ([CTAN: mactex](https://ctan.org/pkg/mactex)). MiKTeX is an implementation of TeX/LaTeX for Windows, Mac and Linux whose package manager installs missing components from the internet when required ([MiKTeX](https://miktex.org/about)).

Install commands for each system are in [Preview and PDF](preview-and-pdf.md#6-install-tex-live-for-latex). On Windows, keep the option that adds the TeX programs to your `PATH` enabled; Manang finds TeX through `PATH`. Under **Plugins > LaTeX**, the **Engine** setting picks `pdflatex`, `xelatex` or `lualatex` for your own TeX.

How Manang runs your TeX (from `crates/manang-latex/src/system.rs`):

- It uses `latexmk` when it is installed. Without `latexmk`, it runs the engine, then `bibtex` if the document uses BibTeX, then reruns the engine up to three passes in total while references are still changing.
- Shell escape is off (`-no-shell-escape`), so packages that need it, such as ones that call external programs, do not work.
- Build files go to a build folder that Manang owns, not into your project folder.

## What is the built-in engine?

The built-in engine is Tectonic, version 0.17.0 in this release (`engines/manifest.json`). Tectonic describes itself as a modernized, complete, self-contained TeX/LaTeX engine, powered by XeTeX and TeXLive ([Tectonic](https://tectonic-typesetting.github.io/book/latest/introduction/index.html)). It automatically downloads support files, so you do not need a full LaTeX system to start; when a document uses a new LaTeX package, it pulls down the files it needs ([Tectonic home page](https://tectonic-typesetting.github.io/en-US/)).

What this means in Manang:

- **It is optional and downloaded once.** Nothing is downloaded until you press **Download the built-in engine (about 48 MB)** and confirm in a window that names the server, size, SHA-256 checksum and install folder. Manang checks the file against a checksum shipped in the app and starts the engine once to confirm it runs. If the connection drops, **Try again** continues where it stopped.
- **It fetches packages on first use.** The first time a document uses a LaTeX package, the engine fetches it from `relay.fullyjustified.net` and keeps it on your computer. Later compiles need no network, so the first compile of a new document can take a minute or more.
- **It is XeTeX.** It handles most articles, theses and Beamer slides, but it does not run packages that only work with `pdflatex` or LuaLaTeX. The **Engine** setting does not apply to it.
- **It is a separate process.** Manang never links it into the app.

You can remove it any time with **Remove the built-in engine** under **Settings > LaTeX**. Your documents are not touched.

### Install the built-in engine

1. Open a LaTeX project on Linux with no TeX installed. The preview says "No TeX installation found".
2. Click **Download the built-in engine (about 48 MB)**.
3. Read the window, then click **Download**. **Cancel** stops it and leaves nothing behind.
4. When it says the engine is ready, the document compiles at once.

## How do I set the main file and compile?

1. The project's main file is `main.tex` unless `manang.toml` says otherwise. To change it, right-click the root `.tex` file in the Files panel and choose **Set as main file**. Manang saves it as `main` in `manang.toml` and compiles that file whichever tab is open. See [Supported File Types](file-types.md#7-the-manangtoml-project-file).
2. Edit your document. With **Auto compile** on, Manang compiles after a short pause (**Debounce ms**, 300 by default). LaTeX compiles the saved file.
3. To compile at once, press `Ctrl+Shift+B` (`Cmd+Shift+B`) or choose **Build > Compile**.
4. The PDF appears in the preview. If an error occurs, the preview keeps the last successful build.

If you are bringing a project over, [Coming from Overleaf](coming-from-overleaf.md#bring-a-latex-project-over) shows the `manang.toml` it needs.

## How do I jump between the source and the PDF?

SyncTeX records how positions in the PDF map to lines in the source ([SyncTeX](https://github.com/jlaurens/synctex)). Manang asks the engine for it on every compile and reads the result itself.

- **PDF to source:** double-click a paragraph, heading, list item or equation in the preview (or hold `Ctrl`/`Cmd` and click).
- **Source to PDF:** press `Ctrl+Alt+J` (`Cmd+Option+J`), or choose **Go > Show in preview**.

See [Preview and PDF](preview-and-pdf.md#3-bidirectional-synctex-navigation).

## Which bibliography tools work?

- **BibTeX** works with both routes. With your own TeX, Manang runs `bibtex` when the document asks for it. The built-in engine runs BibTeX too. Use `\bibliographystyle{...}` and `\bibliography{...}`.
- **biblatex** needs a backend. biblatex works with BibTeX, bibtex8 or Biber and recommends Biber ([CTAN: biblatex](https://ctan.org/pkg/biblatex)). The built-in engine has no `biber`, so a document set up for `biblatex` with `biber` needs your own TeX installation that includes it. With your own TeX, Manang runs `bibtex` itself and leaves other steps to `latexmk`; whether a given `biblatex`/`biber` document builds in one click depends on your `latexmk` setup, and this page does not promise it.
- The **References** panel reads `.bib` and `.biblatex` files and inserts `\cite{key}`. See [References and Bibliographies](references.md).

## How do I see and fix errors?

1. Open the **Problems** tab (`Ctrl+Shift+M` / `Cmd+Shift+M`) for errors and warnings from the TeX log, including BibTeX warnings. Click one to jump to its line.
2. Open the **Output** tab for the raw build log.
3. For small mistakes such as a missing `\usepackage`, a missing TikZ library or a missing `\end{document}`, press **Fix**. It is applied locally with no network and can be undone. See [Fixing Errors](fixing-errors.md).
4. If the message says "No TeX installation found", install a distribution or download the built-in engine. See [Troubleshooting](troubleshooting.md).

## How do I export a PDF?

Press `Ctrl+E` (`Cmd+E`), click **Export PDF** in the preview header, or choose **File > Export PDF**, then choose where to save it. See [Preview and PDF](preview-and-pdf.md#5-export-pdf).

## Does Manang work with no internet at all?

| Task | System TeX | Built-in engine (Linux) |
| :--- | :--- | :--- |
| Create a project from a LaTeX template | Works offline | Works offline |
| Edit, spell check, preview layout | Works offline | Works offline |
| Compile | Works offline, if every package you use is already installed | Works offline once the engine and the packages you use are on your computer |
| SyncTeX, Problems, Fix, PDF export | Works offline | Works offline |
| Get the engine | Not applicable | Needs the internet once |
| A package you have never used | Not in your distribution: install it first. MiKTeX installs missing components from the internet if required | Needs the internet on first use |

Notes:

- Templates are bundled in the app. The LaTeX templates still need a TeX installation or the built-in engine to compile; see [Templates](templates.md).
- To stay fully offline with the built-in engine, turn off **Fetch LaTeX packages on demand** under **Settings > LaTeX**. Packages you have never used then show as not found. The setting is passed to the engine as "refuse to touch the network; the bundle must already be cached" (`crates/manang-latex/src/engine.rs`), so compile at least once online before you switch it off.
- To prepare a system TeX for offline work, install a large scheme in advance, for example `texlive-full` on Debian and Ubuntu, listed in [Preview and PDF](preview-and-pdf.md#6-install-tex-live-for-latex).
- Optional features that use the network, none of which is needed to compile: the **Verify** button in the References panel, live co-editing, and getting updates from a Git address.

For Typst, which needs no TeX and no download, see [Using Manang as a Local Typst Editor](typst-editor.md).

## Sources

Claims about Manang come from its code (`crates/manang-latex/`, `engines/manifest.json`) and its guide pages, named where used. Claims about other projects come from their own pages:

- [https://tectonic-typesetting.github.io/book/latest/introduction/index.html](https://tectonic-typesetting.github.io/book/latest/introduction/index.html): Tectonic is a self-contained TeX/LaTeX engine powered by XeTeX and TeXLive.
- [https://tectonic-typesetting.github.io/en-US/](https://tectonic-typesetting.github.io/en-US/): Tectonic downloads support files, and pulls down files for new packages.
- [https://ctan.org/pkg/texlive](https://ctan.org/pkg/texlive): what TeX Live is, platforms and engines.
- [https://ctan.org/pkg/mactex](https://ctan.org/pkg/mactex): MacTeX is a TeX distribution for macOS, based on TeX Live, over 5 GB.
- [https://miktex.org/about](https://miktex.org/about): MiKTeX platforms and on-demand installation of missing components from the internet.
- [https://github.com/jlaurens/synctex](https://github.com/jlaurens/synctex): SyncTeX links source and PDF positions.
- [https://ctan.org/pkg/biblatex](https://ctan.org/pkg/biblatex): biblatex backends BibTeX, bibtex8 and Biber.

Last checked: October 2026.
