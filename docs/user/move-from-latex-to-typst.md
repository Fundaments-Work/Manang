# Move from LaTeX to Typst

A practical mapping of common LaTeX constructs to Typst, with steps for trying the move in Manang, which edits both languages and converts Markdown to Typst.

Moving a document from LaTeX to Typst is a rewrite of the markup, not a file conversion. This page shows what each common LaTeX construct becomes in Typst, backed by the official Typst documentation, and then what Manang does and does not do to help. Manang does not convert `.tex` files to Typst automatically; you translate the source yourself, section by section, with a live preview showing the result.

## What does Manang offer for the move?

Manang edits LaTeX, Typst and Markdown in one app, so you can keep your LaTeX project open while you build the Typst version.

- **Both languages, one app.** A project has one engine, set as `engine` in its `manang.toml`, and the engine decides how the project compiles. Typst compiles inside Manang with no extra install. LaTeX compiles with your TeX installation or, on Linux, the optional built-in engine; see [Preview and PDF](preview-and-pdf.md#6-install-tex-live-for-latex).
- **Live preview with source jumps.** Typst recompiles as you type, keeps the last good page when there is an error, and lists errors in the **Problems** tab. Double-click a paragraph in the preview to jump to its source.
- **Starter templates in both languages.** **Article (LaTeX)** and **Article (Typst)** are both academic article starters, so you can start the same kind of document in either language. See [Templates](templates.md).
- **The same bibliography file.** The References panel reads `.bib` files, and Typst reads BibLaTeX `.bib` files directly, so a bibliography often moves unchanged ([Typst bibliography reference](https://typst.app/docs/reference/model/bibliography/)).
- **The same figures.** The figure studio and text diagrams write an SVG for Typst and a PDF for LaTeX from one source. See [Figures and Diagrams](figures-and-diagrams.md).
- **Markdown to Typst conversion.** For Markdown files only; see the section below.

Manang has no import wizard for LaTeX, no `\usepackage` translation and no Typst package manager.

## How do I set up a side-by-side migration?

1. Keep your LaTeX project as it is. Open it with **File > Open folder…** if it is not already open.
2. Create a new project for the Typst version: on the Home screen choose **Templates**, then **Article (Typst)** or **Blank Typst**, and name it. A separate folder keeps both builds intact, because each project has a single engine.
3. Copy your `.bib` file and your figure files into the new folder.
4. Translate one section at a time, using the table below. Check the preview after each section.
5. Save a version when a chapter matches the LaTeX output (`Ctrl+Alt+S`, `Cmd+Option+S`); see [Version History and Restore](version-history.md).
6. Compare the two PDFs page by page, then export with `Ctrl+E` (`Cmd+E`).

## How do common LaTeX constructs map to Typst?

The Typst column follows the official guide for LaTeX users and the reference pages cited in Sources.

| Purpose | LaTeX | Typst |
| --- | --- | --- |
| Section heading | `\section{Introduction}` | `= Introduction` |
| Subsection | `\subsection{Setup}` | `== Setup` |
| Bold, italic | `\textbf{x}`, `\emph{x}` | `*x*`, `_x_` |
| Bulleted list | `itemize` with `\item` | `- item` |
| Numbered list | `enumerate` with `\item` | `+ item` |
| Label and reference | `\label{intro}`, `\ref{intro}` | `<intro>`, `@intro` |
| Citation | `\cite{key}` | `@key` or `#cite(<key>)` |
| Citation inside the sentence | `\citet{key}` (natbib), similar intent | `#cite(<key>, form: "prose")` |
| Inline math | `$x^2$` | `$x^2$` |
| Display math | `\[ ... \]`, `equation` | `$ x^2 $` (spaces inside the dollar signs) |
| Fraction | `\frac{a}{b}` | `a / b` |
| Sum with limits | `\sum_{k=1}^{n}` | `sum_(k=1)^n` |
| Matrix | `pmatrix` | `mat(1, 2; 3, 4)` |
| Cases | `cases` | `cases(1 "if" x <= 0, 2 "else")` |
| Upright text in math | `\text{area}` | `"area"` |
| Custom command | `\newcommand` | `#let name(arg) = ...` |
| Split into files | `\input{ch1}`, `\include{ch1}` | `#include "ch1.typ"` |
| Packages | `\usepackage{...}` | built-in features, or `#import "@preview/name:version"` |

Details behind the rows:

- **Headings.** One `=` per level. Numbering is a rule, not a command: `#set heading(numbering: "1.1")` ([heading reference](https://typst.app/docs/reference/model/heading/)).
- **Math.** Single letters stand for themselves; multi-letter names are read as variables or functions, so put words in quotes. Symbols have names such as `pi` and `RR`, and `&` marks alignment points in multi-line formulas ([math reference](https://typst.app/docs/reference/math/)). In Manang, type a symbol name and pick it from the completion list (`Ctrl+Space`).
- **Labels and references.** A label is written after the element it names; `@name` references it ([syntax reference](https://typst.app/docs/reference/syntax/)).
- **Functions.** A LaTeX command becomes a function call introduced by `#`, for example `\underline{text}` becomes `#underline[text]`, and `\newcommand` becomes `#let` ([guide for LaTeX users](https://typst.app/docs/guides/for-latex-users/)).

### How do figures translate?

LaTeX:

```latex
\begin{figure}[htbp]
  \centering
  \includegraphics[width=0.8\textwidth]{figures/setup.pdf}
  \caption{The measurement setup.}
  \label{fig:setup}
\end{figure}
```

Typst:

```typst
#figure(
  image("figures/setup.svg", width: 80%),
  caption: [The measurement setup.],
) <fig-setup>
```

Reference it with `@fig-setup`. The `figure` function takes the caption and a trailing label, and numbers images and tables separately ([figure reference](https://typst.app/docs/reference/model/figure/)). Typst's `image` accepts `png`, `jpg`, `gif`, `svg`, `pdf` and `webp` ([image reference](https://typst.app/docs/reference/visualize/image/)). With a figure from the studio, click **Copy embed line** and paste.

### How do tables translate?

LaTeX:

```latex
\begin{tabular}{lcr}
  Item & Qty & Price \\
  Pens & 3 & 4.50 \\
\end{tabular}
```

Typst:

```typst
#table(
  columns: 3,
  align: (left, center, right),
  table.header([Item], [Qty], [Price]),
  [Pens], [3], [4.50],
)
```

Cells are listed row by row, `columns` sets the number of columns, `align` takes one value per column, and `table.header` marks the header row ([table reference](https://typst.app/docs/reference/model/table/)).

### How do the preamble and document class translate?

A LaTeX preamble becomes `#set` rules at the top of your main Typst file. The **Article (Typst)** and **Thesis Chapter (Typst)** templates show the pattern: `#set page(paper: "a4", margin: ...)`, `#set text(size: 11pt)` and `#set heading(numbering: "1.1")`. A `set` rule applies to the rest of its scope, and a `show` rule redefines how an element looks, which is the closest match to `\renewcommand` ([guide for LaTeX users](https://typst.app/docs/guides/for-latex-users/)).

### How do bibliographies translate?

Replace `\bibliographystyle{...}` and `\bibliography{references}` with one line, placed where the list should appear:

```typst
#bibliography("references.bib", style: "apa")
```

Typst uses `"ieee"` unless you set `style`. It also accepts names such as `"mla"` and `"chicago-author-date"`, or a CSL file. Only works you cite appear in the list. A page or chapter goes in brackets after the key, as in `@key[p. 7]` ([cite reference](https://typst.app/docs/reference/model/cite/)). Manang's References panel inserts `@key` in Typst files; see [References and Bibliographies](references.md).

### What replaces LaTeX packages?

Several common packages are built into Typst: the guide for LaTeX users lists math mode for amsmath, `link` for hyperref, `list`, `enum` and `terms` for enumitem, `text(fill: ...)` for xcolor and `#set text(lang: ...)` for babel. Anything else comes from a package: `#import "@preview/example:0.1.0": add` names a namespace, a name and a version, and community packages are listed on [Typst Universe](https://typst.app/universe/).

In Manang, a package must already be on your computer. Typst's own command-line tool downloads packages on demand, but Manang's compiler only reads the Typst package folders and never downloads. When an import fails, the error appears in **Problems**; [Using Manang as a Local Typst Editor](typst-editor.md#how-do-typst-packages-work-in-manang) explains how to fill the folders.

## How do I convert Markdown to Typst in Manang?

If part of your content is in Markdown, Manang can convert it. LaTeX files cannot be converted this way.

1. Open the `.md` file.
2. Choose **Edit > Convert to Typst**, or run `Convert to Typst` from the command palette (`Ctrl+K`, `Cmd+K`).
3. Manang writes a `.typ` file with the same name in the same folder and opens it. If that file already exists, Manang asks before replacing it and copies the old one to the project trash first.
4. Check the status line. It says how many parts could not be converted exactly.

The converter handles headings, emphasis, strikethrough, lists, task lists, block quotes, code, links, images, tables, footnotes, rules and diagram fences (as figures, or kept as code blocks with a warning when the picture cannot be made). YAML front matter is not carried over. Raw HTML is left out with a warning. Math is copied as written between dollar signs, so LaTeX-style math inside Markdown still needs the Typst rewrite from the table above. The characters `#`, `$`, `@`, `[` and `]` in running text are escaped, so a citation such as `[@key]` is not converted; retype it as `@key`.

## What goes wrong most often?

- **Math with words.** `area` in math is read as a variable; write `"area"`.
- **Display math.** A formula with no space after the first `$` is inline.
- **Paths in chapters.** A path without a leading `/` is relative to the file that contains it; with `/` it starts at the project root ([path reference](https://typst.app/docs/reference/foundations/path/)).
- **Missing packages.** The import line is right but the package is not on this computer; see above.

## When should I stay with LaTeX?

Stay with LaTeX when:

- A publisher, journal or university requires LaTeX source or supplies a LaTeX class, and rewriting would only add risk.
- Your document depends on a package that has no Typst counterpart. CTAN lists thousands of packages ([CTAN](https://ctan.org/)), and Typst Universe is a separate, different collection.
- Co-authors work in LaTeX and share the source.
- The document is finished and stable, and nothing is gained by changing it.

Nothing forces an all-or-nothing choice: Manang keeps each project in its own engine, so new documents can be Typst while old ones stay LaTeX. For the LaTeX side see [Using Manang as an Offline LaTeX Editor](offline-latex-editor.md), and for a longer-form view of Typst see [Coming from Overleaf](coming-from-overleaf.md#is-typst-a-good-alternative-to-latex).

## Sources

- [Typst guide for LaTeX users](https://typst.app/docs/guides/for-latex-users/)
- [Typst heading reference](https://typst.app/docs/reference/model/heading/)
- [Typst math reference](https://typst.app/docs/reference/math/)
- [Typst syntax reference](https://typst.app/docs/reference/syntax/)
- [Typst figure reference](https://typst.app/docs/reference/model/figure/)
- [Typst image reference](https://typst.app/docs/reference/visualize/image/)
- [Typst table reference](https://typst.app/docs/reference/model/table/)
- [Typst bibliography reference](https://typst.app/docs/reference/model/bibliography/)
- [Typst cite reference](https://typst.app/docs/reference/model/cite/)
- [Typst path reference](https://typst.app/docs/reference/foundations/path/)
- [Typst scripting reference](https://typst.app/docs/reference/scripting/)
- [Typst Universe](https://typst.app/universe/)
- [CTAN](https://ctan.org/)

Last checked: October 2026.
