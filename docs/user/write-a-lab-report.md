# Write a Lab Report in LaTeX

Write a lab report from Manang's Lab Report (LaTeX) template: objective, theory, procedure, tables, figures, error analysis, references and PDF export.

The **Lab Report (LaTeX)** template is a short article-class document with six sections laid out the way many lab courses ask for a report: objective, theoretical background, apparatus and procedure, data and results, discussion and error analysis, and conclusion. It is a generic structure. Your course or lab may require different headings, a cover page or a different citation style, so follow your instructor's handout first and use this page for the mechanics.

## What do I need to compile it?

LaTeX compiles in Manang in one of two ways. Which one you have decides what you can do offline.

- **A TeX distribution on your computer** (TeX Live, MiKTeX or MacTeX): works on Windows, macOS and Linux. Manang finds it on your `PATH`.
- **The optional built-in engine** (XeTeX through Tectonic): about 48 MB, downloaded once after you agree, on Linux (x86-64 and arm64) only. On Windows and macOS install a TeX distribution instead.

If neither is present, the preview says "No TeX installation found" and offers the download. Setup steps, install commands and the offline table are in [Using Manang as an Offline LaTeX Editor](offline-latex-editor.md) and [Preview and PDF](preview-and-pdf.md#6-install-tex-live-for-latex). With the built-in engine, the first compile fetches the LaTeX packages the document uses from the internet and keeps them; later compiles of the same document need no network.

## How do I create the project?

1. On the Home screen click **Templates**, then **Use template** on **Lab Report (LaTeX)**. (Or choose **File > New project** and pick it from the **Template** list.)
2. Enter a **Project name**, such as `thermal-lab-1`. It becomes the `\title{...}`.
3. Choose a folder under **Create in** and click **Create project**.
4. Manang creates `main.tex`, `references.bib` and `manang.toml`, opens `main.tex` and compiles it. See [Templates](templates.md).

## What is in the template?

`main.tex` starts with `\documentclass[11pt,a4paper]{article}` and loads `inputenc`, `fontenc`, `amsmath` and `amssymb`. After `\maketitle` it has these sections:

| Section | What it holds in the template |
| :--- | :--- |
| Objective | One paragraph on the purpose, with a `\cite{knuth1984}` citation |
| Theoretical Background | A sentence and an `equation` environment |
| Apparatus and Procedure | An `enumerate` list of three steps |
| Data and Results | A paragraph; no table or figure yet |
| Discussion and Error Analysis | A paragraph on systematic and random uncertainties |
| Conclusion | One paragraph |

The title block uses `\author{Lab Partner 1 \and Lab Partner 2\\Department of Physics}` and `\date{\today}`. All the body text is sample text about thermal dissipation: replace every paragraph, the two partner names and the department. The bibliography ends the file with `\bibliographystyle{plain}` and `\bibliography{references}`, and `references.bib` holds the single sample entry `knuth1984`.

## How do I write each section?

1. **Objective.** State what you measured and why in a few sentences. Cite the source of the method with `\cite{key}`.
2. **Theoretical background.** Put the governing equations here (see the next section).
3. **Apparatus and procedure.** Use the `enumerate` list for steps, one `\item` each. For a list of equipment use `itemize`. Write quantities with a non-breaking space before the unit, as the template does with `1~kHz`.
4. **Data and results.** Add tables and figures (below) and state uncertainties and confidence levels next to the values.
5. **Discussion and error analysis.** Separate systematic and random uncertainty, and compare your result with the prediction.
6. **Conclusion.** Answer the objective in a few sentences.

## How do I add equations?

The template's `equation` environment numbers an equation automatically. Give it a label and refer to it by label so numbers stay correct when you add equations later:

```latex
\begin{equation}
\Delta S \ge \int_0^t \frac{\delta Q}{T}
\label{eq:entropy}
\end{equation}

By Eq.~\eqref{eq:entropy}, ...
```

For several aligned lines use `align` from `amsmath`, which the template already loads ([CTAN: amsmath](https://ctan.org/pkg/amsmath)). Inline math goes between single dollar signs, as in `$95\%$` in the template.

## How do I add tables?

The template loads no table packages, and a plain `tabular` works without any. For cleaner rules and number alignment, add these lines to the preamble, after `\usepackage{amsmath,amssymb}`:

```latex
\usepackage{booktabs}
\usepackage{siunitx}
```

`booktabs` provides `\toprule`, `\midrule` and `\bottomrule` ([CTAN: booktabs](https://ctan.org/pkg/booktabs)). `siunitx` formats numbers and SI units consistently and aligns numbers in table columns with its `S` column type ([CTAN: siunitx](https://ctan.org/pkg/siunitx)). A captioned table with a label:

```latex
\begin{table}[ht]
  \centering
  \caption{Sensor readings.}\label{tab:readings}
  \begin{tabular}{lSS}
    \toprule
    Trial & {Temperature (\si{\kelvin})} & {Heat (\si{\joule})} \\
    \midrule
    1 & 300.1 & 2.31 \\
    2 & 300.4 & 2.29 \\
    \bottomrule
  \end{tabular}
\end{table}

See Table~\ref{tab:readings}.
```

This table, with `\label` on the equation and the `\ref` and `\eqref` calls, compiled with `latexmk` and `pdflatex` in a desktop TeX Live installation when this page was written; Manang runs the same tools with a system TeX. If a package is not installed, the compile reports a missing file: install it through your TeX distribution's package manager (MiKTeX can install missing packages on demand). With the built-in engine on Linux, missing packages are fetched on first use unless you turned off **Fetch LaTeX packages on demand** in Settings under **LaTeX**.

## How do I add figures?

Put your plot or diagram in the project, for example `figures/trial1.pdf`. You can draw a diagram of the apparatus in Manang's figure studio, which saves SVG and, in LaTeX projects, PDF; see [Draw Figures for Papers and Theses](draw-figures-for-papers.md). Add `\usepackage{graphicx}` to the preamble ([CTAN: graphicx](https://ctan.org/pkg/graphicx)), then:

```latex
\begin{figure}[ht]
  \centering
  \includegraphics[width=0.8\linewidth]{figures/trial1.pdf}
  \caption{Sensor trajectory for trial 1.}\label{fig:trial1}
\end{figure}
```

Refer to it as `Figure~\ref{fig:trial1}`. The cross-references settle after the second compile, which latexmk (or Manang's own repeat passes without latexmk, up to three) handles for you. If you see `??` in the preview, compile once more with `Ctrl+Shift+B` (`Cmd+Shift+B`).

## How do I cite sources?

1. Add entries to `references.bib`. Manang reads `.bib` files in the project.
2. Open the **References** panel in the Activity bar, put the cursor in the text, and click **Insert** on an entry to write `\cite{key}`. Or type the command yourself.
3. The `plain` BibTeX style numbers references in alphabetical order. To change the style, change the name in `\bibliographystyle{...}`; see [References and Bibliographies](references.md#2-choose-a-citation-style).
4. Only cited entries are printed. BibTeX works on both compile routes; `biblatex` with `biber` needs your own TeX installation. See [Which bibliography tools work?](offline-latex-editor.md#which-bibliography-tools-work).

## How do I fix compile errors?

Open the **Problems** tab (`Ctrl+Shift+M` / `Cmd+Shift+M`) and click an error to jump to its line; the **Output** tab holds the raw TeX log. For a forgotten `\usepackage` or a missing `\end{document}`, the **Fix** button can apply the repair locally and you can undo it. See [Fixing Errors](fixing-errors.md). While there are errors the preview keeps the last good PDF.

## How do I export the PDF?

Press `Ctrl+E` (`Cmd+E`), click **Export PDF** in the preview header, or choose **File > Export PDF**, then pick a folder and a file name. Manang builds the document first and refuses to export while the **Problems** tab shows errors. Details: [Preview and PDF](preview-and-pdf.md#5-export-pdf). Before you hand the report in, save a named version with `Ctrl+Alt+S` (`Cmd+Option+S`) so you can return to exactly what you submitted; see [Version History and Restore](version-history.md).

## A checklist before you hand it in

1. Every sample paragraph, the partner names, the department and the sample citation are replaced.
2. The **Problems** tab is empty and the PDF shows no `??` or `[?]`.
3. Every table and figure has a caption, units and a reference in the text.
4. Values carry uncertainties and the same number of significant figures throughout.
5. The headings and citation style match your instructor's handout.

## Sources

- [CTAN: amsmath](https://ctan.org/pkg/amsmath): the AMS mathematics package, part of the LaTeX required distribution.
- [CTAN: booktabs](https://ctan.org/pkg/booktabs): publication-quality tables.
- [CTAN: siunitx](https://ctan.org/pkg/siunitx): numbers and SI units, and table alignment.
- [CTAN: graphicx](https://ctan.org/pkg/graphicx): key-value options for `\includegraphics`.

Last checked: October 2026.
