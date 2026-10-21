# Write a Journal Paper in Typst or LaTeX

Start a paper from Manang's Article template in Typst or LaTeX, add citations, figures and equations, export a PDF and save named versions before you submit.

Manang has two Article templates that give you the skeleton of a paper: a title block, an abstract, four numbered sections, one equation, one citation and a bibliography. They are generic articles. They are not the style of any journal or publisher, and journal class files are not included, so the formatting rules of the journal you submit to stay your job. This page walks through writing a paper on top of either template and ends with a checklist for the day you submit.

## Which template should I use: Typst or LaTeX?

Both are listed on the Home screen under **Templates**: **Article (Typst)** and **Article (LaTeX)** (see [Templates](templates.md)). They contain the same sample paper in two languages.

- **Article (Typst)** compiles with the compiler built into Manang. There is nothing else to install and it works offline. See [Using Manang as a Local Typst Editor](typst-editor.md).
- **Article (LaTeX)** needs a TeX distribution on your computer or, on Linux only, the optional built-in engine. See [Using Manang as an Offline LaTeX Editor](offline-latex-editor.md).

If the journal gives you a LaTeX class or a LaTeX template, use the LaTeX one. If the journal only gives you formatting instructions, either works. [Move from LaTeX to Typst](move-from-latex-to-typst.md) covers the differences between the two languages.

## How do I create the project?

1. On the Home screen click **Templates**, then **Use template** on **Article (Typst)** or **Article (LaTeX)**. (Or choose **File > New project** and pick the template in the **Template** list.)
2. Enter a **Project name**, choose a folder under **Create in**, and click **Create project**.
3. Manang copies `main.typ` (or `main.tex`), `references.bib` and a `manang.toml` into a new folder and opens the main file with the live preview.

The project name is written into the document as its title (in LaTeX, `\title{...}`; in Typst, both the PDF title and the heading at the top of the page). Change it in the source at any time; the project name and the title are not linked afterwards.

## What does the template contain?

Every part below is placeholder text that you replace.

| Part | Typst (`main.typ`) | LaTeX (`main.tex`) |
| :--- | :--- | :--- |
| Page | `#set page(paper: "a4", ...)`, 11 pt text | `\documentclass[11pt,a4paper]{article}` |
| Title | Centred block at the top | `\title{...}` and `\maketitle` |
| Authors | The line `Author Name` and a two-line affiliation | `\author{Author Name\\Department...\\\texttt{author@example.edu}}` |
| Date | None | `\date{\today}` |
| Abstract | A smaller indented block starting with `*Abstract*` | The `abstract` environment |
| Sections | `= Introduction`, `= Methodology`, `= Results`, `= Conclusion`, numbered `1.1` | `\section{...}` with the same four names |
| Equation | `$ Delta S >= integral_0^t (delta Q)/T $` | An `equation` environment with the same relation |
| Bibliography | `#bibliography("references.bib")` | `\bibliographystyle{plain}` and `\bibliography{references}` |

The sample entry `knuth1984` in `references.bib` is cited once in the introduction. The template does not set a page number style, a running header or a document language.

### How do I add several authors?

In Typst, the author lines are plain text inside the centred block. For two columns of authors you can use a grid ([Typst grid reference](https://typst.app/docs/reference/layout/grid/)):

```typst
#grid(
  columns: (1fr, 1fr),
  gutter: 1em,
  [Ada Example \ Example University],
  [Grace Sample \ Sample Institute],
)
```

To record the authors in the PDF properties, add `author` to the first line, for example `#set document(title: "My paper", author: ("Ada Example", "Grace Sample"))` ([Typst document reference](https://typst.app/docs/reference/model/document/)). In LaTeX, separate authors with `\and` inside `\author{...}`.

## How do I cite sources from a .bib file?

1. Put your entries in `references.bib` in the project folder. Manang reads `.bib` files in the project automatically.
2. Open the **References** panel in the Activity bar, find an entry, place the editor cursor where the citation goes, and click **Insert**. Manang writes `@key` in Typst or `\cite{key}` in LaTeX.
3. Or type the citation yourself: `@knuth1984` in Typst, `\cite{knuth1984}` in LaTeX.
4. The bibliography appears where the template puts it, at the end. In Typst only cited works are listed; Typst's `bibliography` function lists every work only if you pass `full: true` ([Typst bibliography reference](https://typst.app/docs/reference/model/bibliography/)).

Typst's `bibliography` uses the `ieee` style unless you pass `style:`, for example `#bibliography("references.bib", style: "apa")` ([Typst bibliography reference](https://typst.app/docs/reference/model/bibliography/)). The LaTeX template uses the `plain` BibTeX style; change the name in `\bibliographystyle{...}`. If your journal supplies a `.bst` file, copy it into the project folder and use its name. For `biblatex` and `biber` read the notes in [Using Manang as an Offline LaTeX Editor](offline-latex-editor.md#which-bibliography-tools-work). More on the panel, citation styles and the optional online **Verify** button: [References and Bibliographies](references.md).

## How do I add figures, tables and equations?

Draw a figure in the figure studio or export one from your analysis tool, and save it in the project, for example in `figures/`. [Draw Figures for Papers and Theses](draw-figures-for-papers.md) shows how to draw one and copy the include line.

**Typst.** Put the image in a `figure` with a caption and a label, then refer to it with `@`:

```typst
#figure(
  image("figures/setup.svg", width: 80%),
  caption: [Experimental setup.],
) <fig-setup>

As @fig-setup shows, ...
```

Typst supports PNG, JPG, GIF, SVG, PDF and WebP images ([Typst image reference](https://typst.app/docs/reference/visualize/image/)). Figures and tables are numbered separately ([Typst figure reference](https://typst.app/docs/reference/model/figure/)). To number equations, add `#set math.equation(numbering: "(1)")` near the top, and put a label after an equation to refer to it ([Typst equation reference](https://typst.app/docs/reference/math/equation/)). The template's equation is not numbered.

**LaTeX.** The template loads only `inputenc`, `fontenc` and `amsmath`/`amssymb`. To include images, add `\usepackage{graphicx}` to the preamble ([CTAN: graphicx](https://ctan.org/pkg/graphicx)), then:

```latex
\begin{figure}[ht]
  \centering
  \includegraphics[width=0.8\linewidth]{figures/setup.pdf}
  \caption{Experimental setup.}\label{fig:setup}
\end{figure}
```

Refer to it with `Figure~\ref{fig:setup}`. The template's `equation` environment is numbered; add `\label{eq:entropy}` inside it and refer to it with `\eqref{eq:entropy}`. Which image formats work depends on your LaTeX engine, so check the Problems tab after adding one.

## Can I use my journal's LaTeX class in Manang?

Often yes, because Manang runs TeX on your files in your project folder and does not fix the document class. This is what the code does (`crates/manang-latex/src/system.rs`, `crates/manang-latex/src/worker.rs`): the system TeX runs with the project folder as its working directory, and the built-in engine is given the project folder as its file root, so a `.cls`, `.sty` or `.bst` file in the project folder is found.

1. Copy the journal's class and style files into the project folder, next to `main.tex`.
2. In `main.tex`, replace the `\documentclass` line with the one the journal tells you to use, and follow its instructions for title, authors and abstract.
3. If your main file is not called `main.tex`, right-click it in the Files panel and choose **Set as main file**. See [Supported File Types](file-types.md#7-the-manangtoml-project-file).
4. If the class needs `xelatex` or `lualatex` instead of `pdflatex`, choose it in Settings under **Plugins > LaTeX > Engine**. It applies to your own TeX installation; the built-in engine always uses XeTeX.
5. Compile with `Ctrl+Shift+B` (`Cmd+Shift+B`) and read the **Problems** and **Output** tabs.

Limits: shell escape is off, so classes or packages that call external programs will not work; a class that only works with `pdflatex` or `lualatex` will not build on the built-in engine; and Manang does not check that your output meets the journal's rules. A journal that requires its own LaTeX class means a LaTeX project, because Manang's Typst template is a generic article.

## How do I export the PDF?

Press `Ctrl+E` (`Cmd+E`), click **Export PDF** in the preview header, or choose **File > Export PDF**, then choose where to save. Manang compiles the document and writes the PDF. If the build has errors it refuses and points you to the **Problems** panel. Details: [Preview and PDF](preview-and-pdf.md#5-export-pdf).

Typst compiles with the fonts bundled with Manang and does not scan the fonts installed on your computer in this release, so a journal that requires a specific font may not be possible in Typst here.

## How do I keep drafts and reviewer rounds?

Use versions. Press `Ctrl+Alt+S` (`Cmd+Option+S`) and give the snapshot a name such as *Submitted v1* or *After reviewer 2*. The Versions view lists them, shows line-by-line changes and restores a single file or the whole project without losing history. See [Version History and Restore](version-history.md#1-save-a-version).

## What should I check before submitting?

1. The **Problems** tab is empty and the exported PDF opens in the built-in viewer with every citation and figure resolved (no `?` or `[?]`).
2. All placeholder text is gone: `Author Name`, the affiliation, the sample abstract, the sample `knuth1984` citation and its bibliography entry if you no longer cite it.
3. The format matches the journal's author instructions: page size, font, margins, reference style, length and file type. The template's A4, 11 pt, `plain` or `ieee` defaults are not a journal's.
4. Figures have captions, readable text at print size, and any licence you need to show.
5. The PDF title and author fields are what you want (`#set document(...)` in Typst).
6. You saved a named version of exactly what you submitted.

## Sources

- [Typst document reference](https://typst.app/docs/reference/model/document/): `title` and `author` go into the PDF properties.
- [Typst bibliography reference](https://typst.app/docs/reference/model/bibliography/): `.bib` sources, `@key` citations, default `ieee` style, `full` parameter.
- [Typst figure reference](https://typst.app/docs/reference/model/figure/): captions, labels, separate numbering per kind.
- [Typst image reference](https://typst.app/docs/reference/visualize/image/): supported image formats.
- [Typst equation reference](https://typst.app/docs/reference/math/equation/): equation numbering and labels.
- [Typst grid reference](https://typst.app/docs/reference/layout/grid/): columns and gutter.
- [CTAN: graphicx](https://ctan.org/pkg/graphicx): the package for `\includegraphics` options.

Last checked: October 2026.
