# Writing

Write in Typst, LaTeX or Markdown with syntax highlighting, spell checking, snippets, math symbol completion, formatting, an outline and a live word count.

## 1. Use the text editor

Manang includes a lightweight, responsive text editor built with CodeMirror 6.

1. Open any text document from the Files sidebar (`Ctrl+Shift+E` (`Cmd+Shift+E`)), quick open with `Ctrl+P` (`Cmd+P`), or choose **File > New file** (`Ctrl+N` (`Cmd+N`)).
2. The editor provides syntax highlighting for:
   - Typst (`.typ`)
   - LaTeX (`.tex`, `.sty`, `.cls`, `.dtx`, `.ltx`)
   - Markdown (`.md`, `.markdown`)
   - BibTeX (`.bib`, `.biblatex`)
   - Mermaid diagrams (`.mmd`, `.mermaid`)
3. Use common editing keyboard shortcuts:
   - **Save file:** `Ctrl+S` (`Cmd+S`)
   - **Save all files:** `Ctrl+Shift+S` (`Cmd+Shift+S`)
   - **Undo:** `Ctrl+Z` (`Cmd+Z`)
   - **Redo:** `Ctrl+Shift+Z` (`Cmd+Shift+Z` or `Ctrl+Y`)
   - **Find in document:** `Ctrl+F` (`Cmd+F`)
   - **Replace in document:** Choose **Edit > Replace** from the menu bar.
4. Focus completely on your text with **Zen mode**:
   - Press `Ctrl+\` (`Cmd+\`) or choose **View > Zen mode**. The Activity bar, sidebar, and panels disappear, centering your document in a comfortable reading column.
   - Press `Escape` or `Ctrl+\` (`Cmd+\`) to return to the full workspace layout.
5. Create snapshot checkpoints with **Save a version** (`Ctrl+Alt+S` (`Cmd+Option+S`)) to inspect changes and restore prior drafts. See [Version History and Restore](version-history.md).
6. **Ask the AI assistant:** open it with `Ctrl+Shift+A`. It uses your own key and only proposes changes you accept. See [AI Assistant](ai-assistant.md).
7. **See the structure:** click **Outline** in the Activity bar to list the headings of the open Typst, LaTeX or Markdown document (`= Heading`, `\section{...}` or `# Heading`); click a heading to jump to it.
8. **Find text:** `Ctrl+F` searches the open document. To search every file in the project, see [Search and Replace](search-and-replace.md).

## 2. Work with document engines

### Typst (`.typ`)
Typst provides fast compilation and concise syntax designed for modern academic publishing.

1. Write structure using equals signs for headings:
   ```typst
   = Introduction
   == Background
   === Related Work
   ```
2. Write mathematics enclosed in dollar signs:
   - Inline math: `$E = m c^2$`
   - Display math with spacing: `$ sum_(i=1)^n x_i = (n(n+1)) / 2 $`
3. Style text using asterisks for `*bold*` and underscores for `_italics_`.
4. Include images or diagrams: `#figure(image("figures/diagram.svg"), caption: [Flowchart])`.
5. Present slides: every heading in a Typst document starts a slide. Switch to the slide view by choosing **View > Slides** or by clicking **Slides** at the right of the tab strip, and press `F5` to present. See [Slides and Presentations](slides.md).

### LaTeX (`.tex`)
LaTeX supports existing TeX workflows, academic journal classes, and BibTeX bibliographies.

1. Use standard LaTeX environments:
   ```latex
   \section{Introduction}
   \begin{equation}
     \nabla \times \mathbf{E} = -\frac{\partial \mathbf{B}}{\partial t}
   \end{equation}
   ```
2. Cite sources using `\cite{key}` or `\citep{key}`.
3. Manang compiles LaTeX with your system TeX installation (`latexmk`, `pdflatex`, `xelatex`, or `lualatex`). See [Preview and PDF](preview-and-pdf.md) for setup instructions.

### Markdown (`.md`)
Markdown offers a distraction-free environment for notes, drafts, and documentation.

1. Write standard CommonMark headings (`# Heading`), lists, tables, and blockquotes.
2. Write inline math with `$x$` and display math blocks with `$$...$$`.
3. Render diagrams directly in your document with fenced code blocks:
   ````markdown
   ```mermaid
   flowchart LR
     A[Input] --> B[Output]
   ```
   ````
4. **Convert Markdown to Typst:**
   - To turn a Markdown note into a publication-ready Typst file, choose **Edit > Convert to Typst** from the menu bar, or open the command palette (`Ctrl+K` (`Cmd+K`)) and run `Convert to Typst`.
   - Manang parses the Markdown source into idiomatic Typst markup using the markdown plugin (`markdown.to-typst`), writes `<name>.typ` directly next to your Markdown file, and opens it in a new editor tab.
   - If a `<name>.typ` file already exists, Manang prompts you to confirm replacing its contents before overwriting.

## 3. Check spelling and manage personal dictionaries

Manang checks spelling automatically as you type prose, ignoring markup commands, code blocks, and math formulas.

1. Misspelled words appear with a squiggly underline in the editor and are listed in the Problems panel (`Ctrl+Shift+M` (`Cmd+Shift+M`)).
2. Right-click an underlined word to see spelling suggestions and **Add to dictionary**. A word you add is never flagged again.
3. To manage your personal word list:
   - Open Settings (`Ctrl+,` (`Cmd+,`)).
   - Under **Plugins**, open the **Writing** section to view your **Personal dictionary**.
   - Review saved words or click the delete button next to a word to remove it.
4. To change the spelling language:
   - Open Settings (`Ctrl+,` (`Cmd+,`)).
   - In the **Writing** section, set **Spelling language** to `en_US` (American English) or `en_GB` (British English).
   - This setting can be applied globally (User scope) or for the active project only (This project).
5. To disable spell check entirely:
   - In the same section, turn off the **Spell check** toggle.
6. To run the check again on the open document, press `Ctrl+K` (`Cmd+K`) and run `Check spelling`.

## 4. Format source files

You can tidy document whitespace, indentation, and alignment in one step.

1. Open the document you wish to format.
2. Press `Shift+Alt+F` (`Shift+Option+F`).
   - Or choose **Edit > Format document** from the menu bar.
   - Or press `Ctrl+K` (`Cmd+K`), type `Format document`, and press `Enter`.
3. Manang formats the file in place without losing your undo history.
4. To adjust the target line width for Typst documents:
   - Open Settings (`Ctrl+,` (`Cmd+,`)).
   - In **Plugins > Writing**, edit **Format line width** (default is `80` characters).

## 5. Insert snippets and math symbols

Both appear in the editor's autocomplete list as you type, in the syntax of the file you are editing.

1. **Snippets:**
   - Type at least two letters of a snippet name, such as `theorem`, `equation`, `figure`, `table`, `cite` or `algorithm`, and pick it from the list with `Enter`.
   - The template is inserted with fields. Press `Tab` to move to the next field and `Shift+Tab` to go back.
   - To add your own snippets, create a `snippets.toml` file in Manang's configuration folder. A snippet with the same trigger as a built-in one replaces it.
2. **Math symbols:**
   - In LaTeX, type a backslash and the start of a symbol name, such as `\alp`.
   - In Typst, type the name inside a formula (between `$` signs), such as `alpha`, `integral` or `approx`.
   - Pick the symbol from the list to insert the correct syntax. To open the list yourself, press `Ctrl+Space`.

## 6. Check word count and statistics

1. Open the document you want to inspect. The status bar always shows its word count; hover over it to see characters, sentences, paragraphs and the reading time.
2. For a message instead, open the command palette (`Ctrl+K` (`Cmd+K`)), type `Word count and statistics` and press `Enter`.
3. Manang reports the words, characters and sentences of the active document.
