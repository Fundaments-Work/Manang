# Write a Thesis in Typst

Set up a multi-file Typst thesis in Manang with one file per chapter, a shared bibliography, figures, PDF export and named versions you can restore later.

Manang has no special thesis mode. A thesis is an ordinary Typst project: one main file that sets the page and heading rules, includes one file per chapter, and declares the bibliography once. This page builds that structure from the **Thesis Chapter (Typst)** template and shows how each Manang feature fits in. Writing, previewing and exporting run on your computer with no network connection; only the optional citation check goes online.

## What does Manang give you for a thesis?

- The built-in **Thesis Chapter (Typst)** template, an A4 chapter with numbered headings, a display equation, a citation and a bibliography.
- Multi-file projects: a chapter pulled in with `#include` previews through the main document, and **Export PDF** compiles the whole thesis.
- A References panel that reads your `.bib` file and inserts citations.
- A figure studio that saves SVG files for `image()`.
- Versions: named snapshots of the whole project, with a timeline, line-by-line comparison and restore.
- Crash recovery and safety copies before destructive actions.

## How do I start from the Thesis Chapter template?

1. On the Home screen, click **Templates** in the sidebar, then **Use template** on **Thesis Chapter (Typst)**. (Or choose **File > New project** and pick the template from the **Template** list.)
2. Enter a **Project name**, for example `thesis`, choose a folder under **Create in**, and click **Create project**.
3. Manang creates the project folder with three files: `main.typ`, `references.bib` and `manang.toml`, and opens `main.typ` with the live preview.

The template's `main.typ` is a complete single chapter. It starts with these rules:

```typst
#set document(title: "thesis")
#set page(paper: "a4", numbering: "1", margin: (x: 3cm, top: 3cm, bottom: 2.5cm))
#set text(size: 11pt)
#set heading(numbering: "1.1")
```

It then prints a "Chapter 1" label and the title, writes sections with `=` headings, includes a citation `@knuth1984`, and ends with `#bibliography("references.bib")`. The sample entry `knuth1984` is in `references.bib`. See [Templates](templates.md) for the other starters.

## How do I split the thesis into chapter files?

The template gives you one chapter in `main.typ`. To grow it into a thesis, keep `main.typ` as the thesis root and move the chapters into their own files.

1. In the Files panel, click the **New folder** icon, type `chapters` and press `Enter`.
2. Right-click the `chapters` folder, choose **New file**, type `01-introduction.typ` and press `Enter`. Repeat for each chapter, for example `02-background.typ`.
3. Open `main.typ`. Cut the chapter text, from the first `=` heading to the end of the last section, and paste it into `chapters/01-introduction.typ`. Leave the `#set` rules, the title and the `#bibliography` line in `main.typ`.
4. In `main.typ`, include each chapter where it should appear, in order:

   ```typst
   #include "chapters/01-introduction.typ"
   #pagebreak()

   #include "chapters/02-background.typ"
   ```

   Typst's `include` evaluates the file and inserts its content at that point ([Typst scripting reference](https://typst.app/docs/reference/scripting/)). `#pagebreak()` starts each chapter on a new page.
5. Start each chapter file with a first-level heading such as `= Introduction`. With `#set heading(numbering: "1.1")` in `main.typ`, chapters are numbered 1, 2, 3 and sections 1.1, 1.2.

A typical layout looks like this:

```text
thesis/
  manang.toml
  main.typ
  references.bib
  chapters/
    01-introduction.typ
    02-background.typ
  figures/
    setup.figure
    setup.svg
```

### How does Manang preview a single chapter?

A chapter on its own is a fragment, so Manang does not compile it alone. When the active file is a Typst file that the main document includes with a literal path, Manang compiles the main file instead and shows the whole thesis. The tooltip of the compile button in the preview header says which file it built, for example "via main.typ". To check what the project's main file is, or to change it:

1. Right-click the root document in the Files panel and choose **Set as main file**.
2. Manang saves the choice as `main` in `manang.toml`; the default for a Typst project is `main.typ`.

Two limits, both from how Manang finds chapters:

- Manang looks for `#include "path"` and `#import "path"` with a quoted path in the **main file's** text. A chapter that only another chapter includes is previewed on its own, which may fail or show a fragment. Include every chapter from `main.typ`.
- A path written with a string built in code, such as `#include chapter-name`, is not recognised; use a plain quoted path.

Typst resolves a path without a leading slash from the folder of the file that contains it, and a path with a leading `/` from the project root ([Typst path reference](https://typst.app/docs/reference/foundations/path/)). Inside `chapters/`, write `/figures/setup.svg` rather than `figures/setup.svg`.

## How do I add a bibliography and citations?

The bibliography is declared once, in `main.typ`, after the last chapter:

```typst
#bibliography("references.bib", style: "ieee")
```

Typst reads BibLaTeX `.bib` files and its own Hayagriva YAML, shows only the works you cite, and uses the `"ieee"` style by default; `style` also takes names such as `"apa"`, `"mla"` and `"chicago-author-date"`, or the path of a CSL file ([Typst bibliography reference](https://typst.app/docs/reference/model/bibliography/)).

1. Put your entries in `references.bib`, or add more `.bib` files to the project folder.
2. Click **References** in the Activity bar. Manang indexes the bibliography files; use the **Reload bibliography files** button after editing one outside the app.
3. Place the cursor in a chapter, find the entry with **Filter by author, title or key**, and click **Insert**. Manang writes `@key` for a Typst document.
4. The citation resolves to the list at the end. To check an entry against Crossref, OpenAlex or arXiv, click **Verify** on its card.

[References and Bibliographies](references.md) covers searching, styles and verification in full.

## How do I add figures?

1. Choose **File > New figure**, name it (for example `setup`), and draw in the figure studio. Manang creates `figures/setup.figure`.
2. Press `Ctrl+S` (`Cmd+S`). Saving writes `figures/setup.svg` beside the source.
3. Click **Copy embed line** in the figure toolbar and paste the result into a chapter:

   ```typst
   #figure(image("figures/setup.svg", width: 80%))
   ```

4. Add a caption and a label, and make the path absolute from the chapter folder:

   ```typst
   #figure(
     image("/figures/setup.svg", width: 80%),
     caption: [The measurement setup.],
   ) <fig-setup>
   ```

   Refer to it with `@fig-setup`. The figure function numbers images and tables with separate counters and takes `caption` and a trailing `<label>` ([Typst figure reference](https://typst.app/docs/reference/model/figure/)).

Existing SVG, PNG, JPG or PDF files work too; Typst's `image` supports `png`, `jpg`, `gif`, `svg`, `pdf` and `webp` ([Typst image reference](https://typst.app/docs/reference/visualize/image/)). See [Figures and Diagrams](figures-and-diagrams.md) and [Draw Figures for Papers and Theses](draw-figures-for-papers.md).

## How do I add a table of contents and a list of figures?

Add these to `main.typ` before the first `#include`:

```typst
#outline(title: [Contents], depth: 2)
#pagebreak()
#outline(title: [List of Figures], target: figure.where(kind: image))
```

`#outline()` lists headings by default; `depth` limits the levels, and `target: figure.where(kind: image)` lists figures ([Typst outline reference](https://typst.app/docs/reference/model/outline/)). Page numbering comes from `numbering: "1"` in `#set page(...)`, which the template already sets ([Typst page reference](https://typst.app/docs/reference/layout/page/)).

## How do I export the PDF?

1. Press `Ctrl+E` (`Cmd+E`), click **Export PDF** in the preview header, or choose **File > Export PDF**.
2. Choose a folder and file name in the save dialog and click **Save**.

Manang compiles the project's main document, so the PDF contains every included chapter, whichever tab is active. If the compile fails, fix the entries in the **Problems** tab first (`Ctrl+Shift+M`); see [Fixing Errors](fixing-errors.md) and [Preview and PDF](preview-and-pdf.md#5-export-pdf).

## How do I keep versions and safety copies of a thesis?

A thesis changes for months. Manang gives you three layers of protection, none of which needs a terminal.

### Named versions

1. Press `Ctrl+Alt+S` (`Cmd+Option+S`), or choose **File > Save a version**.
2. The first time, choose **Start keeping versions** and enter your name and email if asked. Versions use Git, so the `git` program must be installed on your computer.
3. Type a name such as *Chapter 3 draft to supervisor* and press `Enter`.

A version records every file in the project that your `.gitignore` does not ignore. Open **Source control**, then **Versions** to browse the timeline, click a version to see which files changed and line-by-line differences, and use **Restore this file** or **Restore whole project**. Restoring never deletes later versions. See [Version History and Restore](version-history.md).

Save a version before big edits (moving a chapter, replacing a figure style, a find-and-replace across files) and before sending a draft to anyone.

### Safety copies

- Before a restore replaces a file, Manang copies the old file into `.git/manang-trash/`.
- Files you move to trash, and files replaced by a project-wide replace, go to `.manang/trash/` in the project.
- Unsaved edits are snapshotted in the background, and a recovery dialog offers them after an unexpected quit.

[Crash Recovery and Safety Copies](crash-recovery.md) lists each case. These copies sit on the same disk as the thesis, so they do not replace an off-machine backup. Copy the project folder to external storage, or connect a shared Git address and press **Share**; see [Use Manang with Git](use-manang-with-git.md).

## A checklist before you submit

1. Every chapter is included from `main.typ`, and the **Problems** tab is empty.
2. Citations resolve and the bibliography style matches your institution's requirement.
3. Figures use paths that work from the chapter folder, and each has a caption and a label.
4. You saved a named version, for example *Submitted*, so you can return to exactly this state.
5. You exported the PDF and opened it in the built-in viewer to check page numbers and the outline.

## Sources

- [Typst scripting reference: modules and includes](https://typst.app/docs/reference/scripting/)
- [Typst path reference](https://typst.app/docs/reference/foundations/path/)
- [Typst bibliography reference](https://typst.app/docs/reference/model/bibliography/)
- [Typst figure reference](https://typst.app/docs/reference/model/figure/)
- [Typst image reference](https://typst.app/docs/reference/visualize/image/)
- [Typst outline reference](https://typst.app/docs/reference/model/outline/)
- [Typst page reference](https://typst.app/docs/reference/layout/page/)

Last checked: October 2026.
