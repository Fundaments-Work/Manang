# Supported File Types

See which file types Manang edits, previews, compiles or opens in a viewer, and how it handles very large text files, binary files and manang.toml.

Manang decides how to open a file from its name, and when the name says nothing, from the first bytes of the file. Every way of opening a file (the Files panel, Go to file, Search results, Problems) follows the same rules.

## 1. Documents you write

| File | Opens as | Compiled or previewed as |
| :--- | :--- | :--- |
| `.typ` | Text editor with Typst highlighting | Typst pages in the live preview, PDF export |
| `.tex`, `.sty`, `.cls`, `.dtx`, `.ltx` | Text editor with LaTeX highlighting | LaTeX through your TeX installation or the built-in engine; PDF export. `.tex` files are compiled; the others are included from them |
| `.md`, `.markdown` | Text editor with Markdown highlighting | Formatted Markdown preview; no PDF export, but **Edit > Convert to Typst** makes a Typst file |
| `.bib`, `.biblatex` | Text editor with BibTeX highlighting | Read by the References panel |

Markdown and Typst files can also be shown formatted while you write; see [The Visual Editor](visual-editor.md). Typst files can also be shown as slides; see [Slides and Presentations](slides.md).

## 2. Bibliographies

The References panel reads BibTeX and BibLaTeX files (`.bib`, `.biblatex`) and CSL-JSON files (`.json`, `.csl`). See [References and Bibliographies](references.md).

## 3. Figures and diagrams

| File | What it is |
| :--- | :--- |
| `.figure` | A vector figure scene, opened in the figure studio. Older `.figure.json` files still open and are upgraded when needed. Saving also writes a picture, `<name>.svg`, next to it. |
| `.mmd`, `.mermaid` | A Mermaid diagram, edited as text with a live preview |
| `.pikchr` | A Pikchr diagram |
| `.bob` | An ASCII diagram drawn with svgbob |

Saving a diagram writes `<name>.svg` beside it (and a PDF for LaTeX projects). See [Figures and Diagrams](figures-and-diagrams.md).

## 4. Images and PDFs

- **Images** (`.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.svg`, `.bmp`, `.avif`, `.ico`) open in a viewer tab. For `.svg`, **Open as text** shows the markup.
- **PDF** files open in the built-in PDF viewer, with search, outline, zoom and dark pages. See [Preview and PDF](preview-and-pdf.md).

## 5. Other text files

Many other plain-text files open in the editor without highlighting, for example `.toml`, `.yaml`, `.yml`, `.ini`, `.cfg`, `.html`, `.css`, `.txt`, `.csv`, `.json`, `.log` and common source-code files. A file with an unknown or missing extension opens as text when its first bytes are valid UTF-8 with no NUL bytes.

### Large text files

Text files larger than 5 MiB do not open in the editor. They open in a read-only viewer that shows the start of the file (the first 256 KiB) and says why.

## 6. Binary files

Archives, Office documents, fonts, audio, video, executables and anything else that is not text open in an information tab instead of the editor. Click **Open with system app** in the toolbar to open the file with the program your operating system associates with it.

## 7. The manang.toml project file

A project can have a `manang.toml` file in its root folder. Manang creates it for projects started from Home, and you can edit it by hand. It uses these keys under `[project]`:

```toml
[project]
engine = "typst"      # required: typst, latex or markdown
name = "My paper"     # optional display name
main = "main.typ"     # optional root document
```

If `main` is missing, Manang uses `main.typ`, `main.tex` or `index.md`, depending on the engine. Choose **Set as main file** on a file in the Files panel to change it; see [Projects and Files](projects-and-files.md). Unknown keys are reported as errors, with a hint, so a typo never silently falls back to a default.
