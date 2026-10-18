# Templates

Manang provides built-in project templates for Typst, LaTeX, and Markdown. Templates provide ready-to-compile project scaffolds complete with document metadata, recommended packages and imports, sample bibliographies, and vector figure includes.

All templates are bundled locally inside the application—you can create new projects completely offline without downloading external assets.

## 1. Create a project from a template

1. Launch Manang to display the Home screen (or choose **File > New project** from the menu bar).
2. Click **Templates** in the Home screen sidebar.
3. Browse the template gallery:
   - Each template card displays the document engine badge (**Typst**, **LaTeX**, or **Markdown**), a brief description, and typical use cases.
4. Click **Use template** on the chosen card.
5. In the creation dialog:
   - Enter a **Project Name** (for example, `quantum-paper`).
   - Select a destination **Folder** using the system file picker.
6. Click **Create project**.
7. Manang copies the template files into the target folder, sets up local project configuration, and opens the root document in the editor with live preview active.

## 2. Built-in template catalog

### Interactive Onboarding
- **Welcome to Manang (`welcome`):**  
  An interactive walkthrough project that introduces Typst and LaTeX editing, live PDF preview, vector drawing in Figure Studio, citation autocomplete, and keybindings. Recommended when opening Manang for the first time.

### Typst Templates
- **Academic Article (`article-typst`):**  
  Standard two-column or single-column academic journal paper template with abstract, section headings, math equations, and bibliography configuration.
- **Blank Typst (`blank-typst`):**  
  A clean, minimal `.typ` document scaffold with zero boilerplate for custom styling.
- **Formal Letter (`letter-typst`):**  
  A clean correspondence and cover letter layout with sender/recipient metadata, dates, and signature lines.
- **Conference Poster (`poster-typst`):**  
  A large-format scientific poster scaffold with structured column layout, figures, and author institutional affiliations.
- **Presentation Slides (`slides-typst`):**  
  Slide presentation template formatted for 16:9 widescreen displays with slide headers, speaker notes, and bullet reveals.
- **Thesis Chapter (`thesis-chapter-typst`):**  
  Multi-file book or doctoral thesis scaffold with separate chapter files, appendices, and shared BibTeX reference library.

### LaTeX Templates
- **Standard Article (`article-latex`):**  
  Traditional LaTeX article scaffold preconfigured with `amsmath`, `graphicx`, `hyperref`, and `biblatex`. Compiles cleanly with Tectonic or TeX Live.
- **Blank LaTeX (`blank-latex`):**  
  A minimal LaTeX document structure ready for custom preamble configurations.
- **Lab Report (`lab-report-latex`):**  
  Structured scientific laboratory report featuring experimental procedures, measurement tables, error propagation formulas, and figures.

### Markdown Templates
- **Blank Markdown (`blank-markdown`):**  
  Minimal CommonMark document for rapid notes and export.
- **Research Notes (`notes-markdown`):**  
  Structured research notebook scaffold featuring GitHub-style alerts, equation fences, tables, and task lists.

## 3. Template reliability and determinism

All built-in templates adhere to strict verification standards:
- **Offline compilation:** Templates do not require internet access or remote font downloads.
- **Instant compilation:** Every template compiles to valid PDF or HTML in under 50 milliseconds upon creation.
- **Reproducible packages:** External package imports use pinned versions to ensure reproducible output across machines.

## 4. Reusing custom project structures

To use your own project layout as a template:
1. Create a clean template folder containing your standard preamble, bibliography, and asset directories.
2. In your operating system file manager, duplicate the folder when starting a new project.
3. In Manang, choose **File > Open folder…** and select the duplicated directory.
