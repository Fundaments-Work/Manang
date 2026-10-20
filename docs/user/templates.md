# Templates

Start a new project from one of twelve built-in Typst, LaTeX and Markdown templates, all bundled in the app so they work without a network connection.

Each template is a small, ready-to-compile project. All of them are bundled inside the application, so you can create new projects completely offline.

## 1. Create a project from a template

1. Launch Manang to display the Home screen (or choose **File > New project** from the menu bar).
2. Click **Templates** in the Home screen sidebar.
3. Browse the gallery. Filter it with the chips **All**, **Typst**, **LaTeX** and **Markdown**, or search by template name in the top bar. Each card shows the template's language and a short description.
4. Click **Use template** on the chosen card. The **New project** dialog opens with that template already selected.
5. In the dialog:
   - Enter a **Project name** (for example, `quantum-paper`).
   - Check the **Language** and **Template**; you can pick a different template from the **Template** list.
   - Under **Create in**, type or **Browse** to the folder that will hold the project.
6. Click **Create project**.
7. Manang copies the template files into a new folder named after the project, writes the project's `manang.toml`, and opens the main document in the editor with the live preview.

You can also click **New project** and choose a template from the **Template** list without visiting the gallery.

## 2. Built-in template catalog

### Interactive onboarding
- **Welcome to Manang** (`welcome`, Typst):
  An interactive onboarding project that teaches the editor, preview, figures, citations and shortcuts. It includes a chapter file, a bibliography, a drawn figure and a Mermaid diagram. Recommended when opening Manang for the first time.

### Typst templates
- **Article (Typst):**
  Academic article with title, abstract, sections and bibliography.
- **Blank Typst:**
  A minimal Typst project with clean defaults.
- **Letter (Typst):**
  Formal correspondence letter with sender header, recipient address and signature.
- **Poster (Typst):**
  Conference poster on a large page, with a title header, a three-column layout and a bibliography.
- **Slides (Typst):**
  Presentation slide deck with a 16:9 page and structured headings. See [Slides and Presentations](slides.md).
- **Thesis Chapter (Typst):**
  Structured thesis chapter with an outline, numbered subheadings and a bibliography.

### LaTeX templates
- **Article (LaTeX):**
  Standard academic LaTeX article with title, abstract, sections and BibTeX citations.
- **Blank LaTeX:**
  A minimal LaTeX starter with the standard article class.
- **Lab Report (LaTeX):**
  Scientific laboratory report with objective, procedure, data, analysis and citations.

### Markdown templates
- **Blank Markdown:**
  A minimal Markdown starter with clean structure.
- **Notes (Markdown):**
  Structured research notes with an inline Mermaid workflow diagram.

The main document of a project is `main.typ` or `main.tex`, or `index.md` for Markdown.

## 3. Offline use

Templates need no internet access to create a project. The LaTeX templates still need a TeX installation or the built-in LaTeX engine to compile; see [Preview and PDF](preview-and-pdf.md).

## 4. Reusing your own project structure

Manang has no custom template gallery. To reuse your own layout:

1. Create a clean folder containing your standard preamble, bibliography and asset directories.
2. In your operating system file manager, duplicate the folder when starting a new project.
3. In Manang, choose **File > Open folder…** and select the duplicated directory.
