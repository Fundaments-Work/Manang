# Using Manang as a Local Typst Editor

Write, preview and export Typst documents on your own computer with Manang, which compiles Typst inside the app and needs no network connection for any of it.

Typst is a markup-based typesetting system that is designed to be as powerful as LaTeX but easier to learn ([Typst repository](https://github.com/typst/typst)). Manang edits Typst files in a project folder on your disk and compiles them with a Typst compiler built into the app. There is nothing else to install, and no account.

## Does Manang need an internet connection for Typst?

No. Writing, the live preview, error checking, the one-click fixes, slides and PDF export all run on your computer with the built-in compiler. Specifically:

- **Templates** are bundled in the app. Creating a project from one needs no network, and none of the Typst templates imports a `@preview` package (checked in the `templates/` folder of the source).
- **Fonts:** the Typst compiler uses the fonts bundled with Manang. In this release it does not scan the fonts installed on your system (the compiler's default settings, checked in the code).
- **Packages:** Manang never downloads Typst packages. See [How do Typst packages work in Manang?](#how-do-typst-packages-work-in-manang)

Things in Manang that do use the network are optional and separate from Typst: the **Verify** button in the References panel (see [References and Bibliographies](references.md#5-verify-references-online)), live co-editing ([Edit Together Live](edit-together-live.md)) and getting updates from a shared Git address ([Share and Get Updates](share-and-get-updates.md)).

## How do I create a Typst project?

1. On the Home screen, click **Templates** in the sidebar, or click **New project**.
2. Filter the gallery with the **Typst** chip and click **Use template** on a card. The Typst templates are **Article (Typst)**, **Blank Typst**, **Letter (Typst)**, **Poster (Typst)**, **Slides (Typst)**, **Thesis Chapter (Typst)** and the onboarding project **Welcome to Manang**. See [Templates](templates.md).
3. Enter a **Project name**, check **Language** is **Typst**, choose a folder under **Create in**, and click **Create project**.
4. Manang writes the files and a `manang.toml` project file, and opens `main.typ` with the live preview beside it.

If you already have a Typst folder, add a `manang.toml` with `engine = "typst"` and open the folder with **File > Open folder…**; see [Coming from Overleaf](coming-from-overleaf.md#bring-a-typst-project-over) for the exact file.

## How do I write and see the result?

1. Type in the editor. After a short pause (300 ms by default) Manang recompiles and the preview updates without losing your place. You can turn this off with **Auto compile** in Settings; then press `Ctrl+Shift+B` (`Cmd+Shift+B`) or choose **Build > Compile**.
2. If an error occurs, the preview keeps showing the last successful build and the **Problems** tab lists the errors. Click one to jump to its line.
3. Change the layout with `Ctrl+Shift+V` (`Cmd+Shift+V`): editor and preview, editor only, or preview only.

More on the preview, zoom and search is in [Preview and PDF](preview-and-pdf.md). Headings, equations and snippets are in [Writing](writing.md#typst-typ). If you prefer to see headings and formulas formatted while you type, see [The Visual Editor](visual-editor.md).

## How do I jump between the source and the preview?

- **Preview to source:** double-click a paragraph, heading, list item or equation in the preview (or hold `Ctrl`/`Cmd` and click). The caret lands on the source line that produced it.
- **Source to preview:** put the caret in the editor and press `Ctrl+Alt+J` (`Cmd+Option+J`), or choose **Go > Show in preview**. The preview scrolls to that position and highlights it.

The details are in [Preview and PDF](preview-and-pdf.md#3-bidirectional-synctex-navigation).

## How do I fix errors?

Errors appear in the **Problems** tab and, with a squiggle, in the editor. For a few small mistakes Manang offers a **Fix** button next to **Fix with AI**: a Typst name that belongs to a known `@preview` package (`cetz`, `fletcher`, `touying`, `physica` and others) gets an `#import` added, a one-letter slip in a keyword or known function is corrected, and an unclosed `(`, `[`, `{`, `$`, string or raw block gets its closer. Nothing is applied until you press the button, and one `Ctrl+Z` undoes it. Fixes are computed on your computer and never use the network. The full list is in [Fixing Errors](fixing-errors.md).

Adding an `#import` does not download the package. If the package is not on your computer, the compile still fails; see the next section.

## How do Typst packages work in Manang?

In Typst, a package is imported with a namespace, a name and a version, for example `#import "@preview/example:0.1.0": add` ([Typst scripting reference](https://typst.app/docs/reference/scripting/)). The Typst command-line compiler downloads `@preview` packages on demand and keeps them in a cache; importing a cached package does not use the network, and packages you place in the data directory under `typst/packages` take precedence over the cache ([typst/packages](https://github.com/typst/packages)).

Manang's compiler uses the same two folders but only reads them. This is what the code does (`crates/manang-typst/src/world.rs`: "Packages are never downloaded"):

- It looks for `<namespace>/<name>/<version>` in the Typst data folder (`~/.local/share/typst/packages` on Linux, `~/Library/Application Support/typst/packages` on macOS, `%APPDATA%\typst\packages` on Windows) and then in the Typst cache folder (`~/.cache/typst/packages` on Linux, `~/Library/Caches/typst/packages` on macOS, `%LOCALAPPDATA%\typst\packages` on Windows).
- If the package is not in either folder, the import fails and the error appears in **Problems**.

To use a package such as `@preview/cetz`, put it in one of those folders first. One way is to run the `typst` command-line tool once while online on a document that imports it; it fills the cache, and Manang then finds the package and works offline. You can also copy a package folder into the data folder yourself, for example under `local/` and import it as `@local/name:version`. This workflow is described by the Typst project, not by Manang; Manang has no package manager screen.

## How do I write math?

Put math between dollar signs. Inline math has no spaces next to the dollar signs, `$E = m c^2$`; display math has whitespace after the opening and before the closing sign, `$ sum_(i=1)^n x_i $`, and is centered on its own line ([Typst equation reference](https://typst.app/docs/reference/math/equation/)). In the editor, type a symbol name inside a formula, such as `alpha` or `integral`, and pick it from the completion list (`Ctrl+Space` opens it). See [Writing](writing.md#5-insert-snippets-and-math-symbols).

## How do I add figures?

1. Put an image in the project, or draw one in the figure studio or write a text diagram; see [Figures and Diagrams](figures-and-diagrams.md).
2. With the figure or diagram file open, choose **Edit > Copy include line** and paste the result into your document.
3. The copied Typst looks like this, and `@fig-circuit` refers to it in your text:

   ```typst
   #figure(
     image("figures/circuit.svg"),
     caption: [Circuit schematic],
   ) <fig-circuit>
   ```

The `#figure` function takes the image as its body and a `caption`; a label after it can be cited with `@` ([Typst figure reference](https://typst.app/docs/reference/model/figure/)).

## How do I add a bibliography?

1. Put a `.bib` file in the project. Manang indexes it in the **References** panel; click **Reload bibliography files** there after changing it outside the app.
2. Click **Insert** on a reference card to put `@key` at the caret.
3. Add the bibliography to the document:

   ```typst
   #bibliography("references.bib", style: "apa")
   ```

Typst reads BibLaTeX `.bib` files and its own Hayagriva YAML format, can cite with `@key`, and shows only the works you cited ([Typst bibliography reference](https://typst.app/docs/reference/model/bibliography/)). The References panel lists `.bib`, `.biblatex`, `.json` and `.csl` files; it does not list Hayagriva YAML. See [References and Bibliographies](references.md).

## How do I export a PDF?

1. Press `Ctrl+E` (`Cmd+E`), click **Export PDF** in the preview header, or choose **File > Export PDF**.
2. Choose a folder and file name in the save dialog and click **Save**.

Manang compiles the document and writes the PDF to disk. No network is involved.

## How do I make slides?

Create a project from **Slides (Typst)**. In a Typst document every heading starts a slide in the **Slides** view; click **Slides** at the right of the tab strip, and press `F5` to present full window. The Slides view is an outline of your headings, not a rendering of your page design, and the PDF you export is still the compiled document. See [Slides and Presentations](slides.md).

## Where can I read more?

- [Getting Started](getting-started.md) for the window layout.
- [Install Manang](install.md) if you have not installed it yet.
- [Using Manang as an Offline LaTeX Editor](offline-latex-editor.md) for LaTeX projects.
- [Coming from Overleaf](coming-from-overleaf.md) for moving an existing project over.

## Sources

Claims about Manang come from its own code and guide pages, named where they are used. Claims about Typst come from the Typst project:

- [https://github.com/typst/typst](https://github.com/typst/typst): what Typst is, and that its compiler runs locally.
- [https://github.com/typst/packages](https://github.com/typst/packages): `@preview` packages are downloaded on demand by the Typst compiler, cache and data folder locations, precedence, and that a cached package needs no network.
- [https://typst.app/docs/reference/scripting/](https://typst.app/docs/reference/scripting/): the package import syntax.
- [https://typst.app/docs/reference/math/equation/](https://typst.app/docs/reference/math/equation/): inline and display math.
- [https://typst.app/docs/reference/model/figure/](https://typst.app/docs/reference/model/figure/): figures, captions and labels.
- [https://typst.app/docs/reference/model/bibliography/](https://typst.app/docs/reference/model/bibliography/): supported bibliography formats and citing.

Last checked: October 2026.
