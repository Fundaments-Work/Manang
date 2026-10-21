# Make a Conference Poster in Typst

Make a poster from Manang's Poster (Typst) template: set the A1 page, arrange three columns under a title banner, add figures and references, and export a PDF.

The **Poster (Typst)** template is a single Typst file that sets an A1 page, puts a title banner across the top and flows six numbered sections into three columns. It is a generic layout, not a template from any conference or institution, so check your event's poster instructions for the required size and orientation before you start. Typst compiles inside Manang with no network connection and nothing else to install.

## How do I start from the Poster template?

1. On the Home screen click **Templates**, then **Use template** on **Poster (Typst)**. (Or choose **File > New project** and pick it from the **Template** list.)
2. Enter a **Project name**, which becomes the poster title in the banner, choose a folder under **Create in**, and click **Create project**.
3. Manang creates `main.typ`, `references.bib` and `manang.toml`, and opens `main.typ` with the live preview. See [Templates](templates.md).

The preview shows one very large page. Press `Ctrl+0` (`Cmd+0`) to fit the page to the width of the preview pane, and use the zoom presets in the preview header to read details. See [Preview and PDF](preview-and-pdf.md).

## What does the template set up?

The first lines of `main.typ` decide the page:

```typst
#set page(paper: "a1", flipped: false, margin: (x: 2cm, y: 2cm))
#set text(size: 18pt)
```

- **Page size:** `paper: "a1"` is A1, 594 by 841 mm, and `flipped: false` keeps it portrait. Typst's `page` function accepts paper names from `"a0"` to `"a11"` and others, `flipped: true` switches to landscape, and `width` and `height` set an exact size instead ([Typst page reference](https://typst.app/docs/reference/layout/page/)).
- **Margins:** 2 cm left and right, 2 cm top and bottom. The `margin` parameter accepts one length or a dictionary with keys such as `x`, `y`, `top` and `left` ([Typst page reference](https://typst.app/docs/reference/layout/page/)).
- **Text size:** 18 pt for body text, because the page is large.

Then comes the header. It is a `block` with a light grey fill (`#f5f5f5`), 2 cm of inner padding and the full width, centred. Inside it are the title at 42 pt bold, the author line at 24 pt with superscript numbers (`Author Name#super[1], Co-Author Name#super[2]`) and the affiliations at 18 pt in grey. The title comes from the project name; everything else is placeholder text.

The body is a three-column block:

```typst
#columns(3, gutter: 2cm)[
  == 1. Introduction
  ...
  #colbreak()
  == 3. Methodology
  ...
  #colbreak()
  == 5. Discussion
  ...
]
```

The `columns` function splits its content into equally wide columns, with a gutter of `4%` by default; the template sets `2cm` ([Typst columns reference](https://typst.app/docs/reference/layout/columns/)). The template places two `#colbreak()` calls, which end a column on purpose ("explicitly continue in the next column", as the Typst docs put it). So column one holds sections 1 and 2, column two holds 3 and 4, and column three holds section 5 and the references. The section numbers are typed into the headings; the template does not turn on heading numbering. The columns do not balance their height for you.

The references come from `#bibliography("references.bib")` at the end of the third column, with the sample entry `knuth1984` cited in the introduction as `@knuth1984`.

## How do I change the layout?

Edit `main.typ`. The preview updates after a short pause (300 ms by default), and the **Problems** tab lists any error.

1. **Another paper size.** Change `paper: "a1"` to `"a0"`, `"a2"` or another name, or replace it with `width: 90cm, height: 120cm`. For landscape, set `flipped: true`.
2. **Another number of columns.** Change the `3` in `#columns(3, ...)` and add or remove `#colbreak()` lines so each column holds the sections you want.
3. **Put text where you want it with a grid.** If you would rather place each column's content separately than rely on breaks, use a grid with one cell per column ([Typst grid reference](https://typst.app/docs/reference/layout/grid/)):

   ```typst
   #grid(
     columns: (1fr, 1fr, 1fr),
     gutter: 2cm,
     [
       == 1. Introduction
       Text.
     ],
     [
       == 2. Methods
       Text.
     ],
     [
       == 3. Results
       Text.
     ],
   )
   ```

   A track sized `1fr` takes a share of the free width, so `(2fr, 1fr, 1fr)` makes the first column twice as wide as the others. A cell can span columns with `grid.cell(colspan: 2)[...]`.
4. **A figure.** Save an image in the project (for example `figures/results.svg`; see [Draw Figures for Papers and Theses](draw-figures-for-papers.md)) and add it where it belongs:

   ```typst
   #figure(
     image("figures/results.svg", width: 100%),
     caption: [Convergence under three drive schedules.],
   )
   ```

   Inside a column, `width: 100%` is the width of that column. Typst's `figure` can float to the top or bottom of the page with `placement`, and with `scope: "parent"` a figure can span all columns of a page that uses the page-level `columns` setting ([Typst figure reference](https://typst.app/docs/reference/model/figure/), [Typst place reference](https://typst.app/docs/reference/layout/place/)). The template's `columns(...)` block is a container, not the page setting, so for a figure that spans the full width, put it between the header and the `columns` block or in a grid cell that spans columns.
5. **Colours and sizes.** The header colours and text sizes are plain `fill:` and `size:` values; change them or remove the `fill` for a print without a grey banner.

## How do I change the references?

- The template cites with `@knuth1984` and lists references with `#bibliography("references.bib")`. Add entries to `references.bib`, then cite them with `@key` or insert them from the **References** panel. See [References and Bibliographies](references.md#4-insert-citations-into-a-document).
- Typst's `bibliography` uses the `ieee` style by default and lists only cited works ([Typst bibliography reference](https://typst.app/docs/reference/model/bibliography/)). Pass `style:` for another one, for example `style: "apa"`.
- The template has a heading `== 6. References` and then calls `bibliography`, which by default adds its own heading, so the printed poster may show two headings. Pass `title: none` to the bibliography to keep only yours: `#bibliography("references.bib", title: none)`.
- On a poster you often cite only a few works. Remove the sample `knuth1984` entry and citation once you have your own.

## How do I export a print-ready PDF?

1. Check the preview at several zoom levels and make sure the **Problems** tab is empty.
2. Press `Ctrl+E` (`Cmd+E`), or click **Export PDF** in the preview header, or choose **File > Export PDF**.
3. Choose a folder and a file name, and click **Save**. Manang compiles the poster and writes the PDF.
4. Open the PDF in the built-in viewer, or any viewer, and check the page size in its properties: for the template it should be A1.

What the export does not do: the template adds no bleed, crop marks or colour profile, and Manang has no print settings for them. Ask your print shop what they need; many accept a PDF at the poster's final size. Typst compiles with the fonts bundled with Manang and does not scan the fonts installed on your computer in this release, so a font you use in other programs may not be available.

## Can I save versions while I work?

Yes. Press `Ctrl+Alt+S` (`Cmd+Option+S`) to save a named version, for example *Draft for supervisor* and *Printed*. You can compare versions line by line and restore one file or the whole project. See [Version History and Restore](version-history.md).

## A checklist before you print

1. Placeholder text is gone: `Author Name`, `Co-Author Name`, the affiliations, the sample sections and the sample citation.
2. The page size and orientation match what the event asks for.
3. Text is readable from the distance people will stand at; 18 pt is only the template's starting value.
4. Every figure has a caption and every citation resolves in the reference list.
5. You exported the PDF again after your last edit.

## Sources

- [Typst page reference](https://typst.app/docs/reference/layout/page/): `paper`, `width`, `height`, `flipped` and `margin`.
- [Typst columns reference](https://typst.app/docs/reference/layout/columns/): `count`, `gutter` and `colbreak`.
- [Typst grid reference](https://typst.app/docs/reference/layout/grid/): `columns`, `gutter`, `fr` tracks and `grid.cell`.
- [Typst figure reference](https://typst.app/docs/reference/model/figure/): captions, `placement` and `scope`.
- [Typst place reference](https://typst.app/docs/reference/layout/place/): parent-scoped placement.
- [Typst bibliography reference](https://typst.app/docs/reference/model/bibliography/): sources, default style and the `title` parameter.

Last checked: October 2026.
