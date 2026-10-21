# Draw Figures for Papers and Theses

Draw diagrams in Manang's figure studio or write them as Mermaid, Pikchr or ASCII text, then include the saved SVG or PDF in a Typst, LaTeX or Markdown paper.

This page is a workflow guide for people who need figures in a paper or thesis: which tool to pick, how to make a figure that prints well, and how to include it so the document compiles. The tool-by-tool reference, with every shortcut and inspector section, is in [Figures and Diagrams](figures-and-diagrams.md). Everything below runs locally; nothing is uploaded.

## Which figure tool should I use?

| You want | Use | Saved as |
| --- | --- | --- |
| A block diagram, apparatus sketch or annotated schematic where layout is by hand | Figure studio | `figures/name.figure` and `figures/name.svg` |
| A flowchart, sequence diagram, class diagram, state machine or Gantt chart written as text | Mermaid file (`.mmd`) | SVG beside the source |
| A box-and-arrow diagram with exact placement written as text | Pikchr file (`.pikchr`) | SVG beside the source |
| A quick sketch drawn with characters | ASCII file (`.bob`) | SVG beside the source |
| A flowchart you want to start from text and then adjust by hand | Mermaid button in the figure studio | Editable shapes and bound arrows |

Text diagrams suit figures that change often or are reviewed in version control, because the source is a few lines of text. The studio suits figures where you decide where everything sits. Plots and charts from your data are not drawn here; export them from your analysis tool as SVG, PDF or PNG and add them to the project.

## How do I draw a figure in the studio?

1. Choose **File > New figure**, or click the **Figures** icon in the Activity bar and the **+** button. Enter a name such as `setup`.
2. Manang creates `figures/setup.figure` and opens it on a canvas with a toolbar, a layers panel and an inspector.
3. Draw boxes with Rectangle (`R`), Ellipse (`O`) and Diamond (`D`). Add labels with Text (`T`), or double-click a shape to type its label.
4. Connect shapes with Arrow (`A`) or Line (`L`): drag from one shape to another.
5. Group related shapes inside a labelled Frame (`F`).
6. Press `Ctrl+S` (`Cmd+S`). Manang writes `figures/setup.svg` next to the source.

### What are bound arrows, and why do they matter for papers?

A bound arrow is attached to anchor points (top, right, bottom, left) of the shapes it connects. When you move or resize a shape, the arrow follows. For a paper this means you can rearrange a block diagram late in the writing process without redrawing every connection. In the inspector's **Stroke** section choose the route (*straight*, *elbow* or *curved*) and the heads (*none*, *arrow*, *triangle*, *dot* or *bar*) at each end. Hold `Alt` (`Option`) while dragging to move a shape without snapping.

### What are frames for?

A frame is a labelled container that groups and organises nested elements on the canvas. Use one to keep a subsystem together or to mark a panel such as "(a)". The layers panel lists every element in stacking order with lock and visibility toggles; a hidden layer is not drawn and not exported.

### How do I put an equation in a figure?

Choose the Formula tool (`M`). In the inspector, set the language to Typst or LaTeX and write the formula. It becomes a vector node: Typst math is rendered through Typst and LaTeX math through a built-in renderer, and the result is drawn as outlines with no font dependency, so the figure looks the same on every computer.

### How do I make a figure that prints well?

- **Use the palette tokens.** The colour row offers *ink*, *muted*, *accent* and seven hues (*orange*, *sky*, *green*, *yellow*, *blue*, *vermillion*, *purple*) taken from a colour-blind-safe set. Saved files use the print values of these tokens, and a hue used as a fill is drawn as a light tint, so labels stay readable.
- **Choose a clean or hand-drawn style.** The **Effects** section offers *clean*, *sketchy* and *rough*. Use *clean* for a publication figure.
- **Set the canvas size.** With nothing selected, the inspector shows **Canvas** size and background. Choose a background only if your document needs one; None keeps exports transparent.
- **Keep text readable at the final size.** Figures are scaled when included (see below). Check the preview at the width the document will use.

## How do I write a text diagram?

1. Choose **File > New diagram**, pick Mermaid flowchart, Mermaid sequence, Pikchr or ASCII, name it, and click **Create**. The file goes in `figures/`.
2. Edit the source. The preview shows the rendered SVG, and a syntax error is reported with its line and column in the **Problems** panel.
3. Press `Ctrl+S` (`Cmd+S`). Manang writes `<name>.svg` beside the source.

Mermaid sources use extensions `.mmd` and `.mermaid`, Pikchr uses `.pikchr`, and ASCII diagrams use `.bob`. In a Markdown document you can also write a fenced block with the language `mermaid`, `pikchr`, `bob` or `svgbob` and the preview draws it inline. The same source and theme always produce the same SVG bytes, which keeps version-control diffs small.

## What formats does Manang export?

- **SVG** for every figure and diagram, written when you save. The studio's inspector also has **Export SVG**, which downloads the figure as an SVG file.
- **PDF** beside the SVG when the project is LaTeX, or when it contains a `.tex` file. Manang converts the SVG to a one-page PDF itself, so LaTeX needs no Inkscape.

Manang does not export TikZ or CeTZ code from the figure studio or text diagrams, and the studio has no PNG export. If you need TikZ or CeTZ, write it by hand in the document; the Typst package route is described in [Using Manang as a Local Typst Editor](typst-editor.md#how-do-typst-packages-work-in-manang).

## How do I include a figure in my document?

Open the figure or diagram and click **Copy embed line** in the studio toolbar (or choose **Edit > Copy include line** for a diagram file), then paste into the document. The line depends on the project's language.

### Typst

```typst
#figure(
  image("figures/setup.svg", width: 80%),
  caption: [The measurement setup.],
) <fig-setup>
```

Refer to it with `@fig-setup`. The `figure` function takes a caption and a trailing label, and Typst's `image` accepts `svg`, `png`, `jpg`, `gif`, `pdf` and `webp` ([Typst figure reference](https://typst.app/docs/reference/model/figure/), [Typst image reference](https://typst.app/docs/reference/visualize/image/)). A path without a leading `/` is relative to the file that contains it, and a path with a leading `/` starts at the project root ([Typst path reference](https://typst.app/docs/reference/foundations/path/)). From a chapter in `chapters/`, write `/figures/setup.svg`; see [Write a Thesis in Typst](write-a-thesis-in-typst.md).

### LaTeX

```latex
\begin{figure}[htbp]
  \centering
  \includegraphics[width=0.8\linewidth]{figures/setup.pdf}
  \caption{The measurement setup.}
  \label{fig:setup}
\end{figure}
```

Manang points at the PDF it wrote next to the SVG. The optional width argument comes from the `graphicx` package, so load it in the preamble with `\usepackage{graphicx}` ([graphicx on CTAN](https://ctan.org/pkg/graphicx)). Refer to the figure with `\ref{fig:setup}`. The path is written from the project root, which is right when your main `.tex` file is in the project root.

### Markdown

```markdown
![The measurement setup](figures/setup.svg)
```

## How do I keep figures under version control?

A `.figure` file is human-readable JSON with a fixed key order and rounded numbers, so a one-shape edit is a small diff. The `.svg` and `.pdf` beside it are generated and deterministic: the same figure gives the same bytes. Save a version after a figure is final (`Ctrl+Alt+S`, `Cmd+Option+S`); [Version History and Restore](version-history.md) shows how to compare and restore.

## A short checklist before you submit

1. Every figure has a caption and a label, and the text refers to it.
2. The SVG or PDF is current: save the source again after any change.
3. Colours work in greyscale or for readers with colour-vision deficiency; do not rely on hue alone.
4. The preview shows labels legible at the final width.
5. The PDF export opens in the built-in viewer with every figure present; see [Preview and PDF](preview-and-pdf.md#5-export-pdf).

## Sources

- [Typst figure reference](https://typst.app/docs/reference/model/figure/)
- [Typst image reference](https://typst.app/docs/reference/visualize/image/)
- [Typst path reference](https://typst.app/docs/reference/foundations/path/)
- [graphicx on CTAN](https://ctan.org/pkg/graphicx)
- Manang source: `crates/manang-figure`, `crates/manang-diagram` and `crates/manang-project/src/export.rs`.

Last checked: October 2026.
