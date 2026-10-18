# Writing

You can write and edit scientific papers in Typst, LaTeX, and Markdown, format code, check spelling against custom dictionaries, search mathematical symbols, and monitor document word counts.

## 1. Use the text editor

Manang includes a lightweight, responsive text editor built with CodeMirror 6.

1. Open any text document from the Files sidebar (`Ctrl+Shift+E` (`Cmd+Shift+E`)), quick open with `Ctrl+P` (`Cmd+P`), or choose **File > New file** (`Ctrl+N` (`Cmd+N`)).
2. The editor provides syntax highlighting for:
   - Typst (`.typ`)
   - LaTeX (`.tex`, `.sty`, `.cls`, `.dtx`, `.ltx`)
   - Markdown (`.md`, `.markdown`)
   - BibTeX (`.bib`, `.biblatex`)
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
6. *An integrated AI authoring assistant is coming soon.*

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
5. Present slides: If you structure your Typst document with slide headings (`== Slide Title`), switch to presentation view by choosing **View > Slides** or running `Show document as slides` from the command palette (`Ctrl+K` (`Cmd+K`)). Press `F5` to present slides full screen.

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
2. To accept a word and add it to your personal dictionary:
   - Right-click the underlined word and select **Add to dictionary**.
   - Or open the command palette (`Ctrl+K` (`Cmd+K`)), type `Add word to user dictionary`, and enter the word.
3. To manage your personal word list:
   - Open Settings (`Ctrl+,` (`Cmd+,`)).
   - Scroll to the **Writing** section to view your **Personal dictionary**.
   - Review saved words or click the delete button next to a word to remove it.
4. To change the spelling language:
   - Open Settings (`Ctrl+,` (`Cmd+,`)).
   - Under **Writing**, choose between `en_US` (American English) and `en_GB` (British English).
   - This setting can be applied globally (User scope) or for the active project only (Project scope).
5. To disable spell check entirely:
   - Open Settings and turn off the **Spell check** toggle.

## 4. Format source files

You can tidy document whitespace, indentation, and alignment in one step.

1. Open the document you wish to format.
2. Press `Shift+Alt+F` (`Shift+Option+F`).
   - Or choose **Edit > Format document** from the menu bar.
   - Or press `Ctrl+K` (`Cmd+K`), type `Format document`, and press `Enter`.
3. Manang formats the file in place without losing your undo history.
4. To adjust the target line width for Typst documents:
   - Open Settings (`Ctrl+,` (`Cmd+,`)).
   - Under **Writing**, edit **Format line width** (default is `80` characters).

## 5. Insert snippets and search math symbols

1. **Snippets:**
   - Type snippet triggers for frequent patterns (such as figure environments, tables, or theorem blocks) and press `Tab` to expand template code.
   - List available snippets by opening the command palette (`Ctrl+K` (`Cmd+K`)) and running `List snippets`.
2. **Math symbols:**
   - Open the command palette (`Ctrl+K` (`Cmd+K`)).
   - Type `Search math symbols` and enter the symbol name (e.g., `alpha`, `int`, `approx`).
   - Select the symbol to insert the appropriate syntax directly into your document.

## 6. Check word count and statistics

1. Open the document you want to inspect.
2. Open the command palette (`Ctrl+K` (`Cmd+K`)).
3. Type `Word count and statistics` and press `Enter`.
4. Manang reports the total number of words, characters, lines, and reading time for the active document.
