# The Visual Editor

See Markdown and Typst formatted as you write in the Visual editor, switch back to Source at any time, and use the toolbar; the file stays plain text.

The Visual editor shows Markdown and Typst files as formatted text while you write: headings are large, bold is bold, lists have bullets, formulas and images are drawn. It is still your file. Nothing is converted and written back, so the text on disk, your version history and your co-authors all see ordinary Markdown or Typst, and switching back to **Source** shows exactly what is in the file.

Visual is optional. Every file opens in Source unless you choose otherwise.

## 1. Switch between Source and Visual

For a `.md`, `.markdown` or `.typ` file, the strip above the editor has a **Source | Visual** switch.

- Click **Visual**, or press `Ctrl+Alt+V` (`Cmd+Option+V`), or choose **View > Visual editor**.
- Click **Source** or press the same keys to go back. Your caret, undo history and unsaved edits stay as they are.
- Each tab remembers its own mode while it is open.
- Manang also remembers your choice for the file type: after you switch a Markdown file to Visual, the next Markdown file you open starts in Visual too. Switch it back to Source and it is forgotten. The setting behind this is **Settings > Editor > Visual default** (`editor.visual-default`), a list of `markdown` and `typst` (empty by default).

Other formats (LaTeX, BibTeX) have no Visual mode yet and show no switch.

> `Ctrl+Shift+V` already cycles the preview layout, so Visual uses `Ctrl+Alt+V`.

## 2. What you see

| In the file | In Visual |
| :--- | :--- |
| `# Title`, `= Title` | The title in large type, without the `#` or `=` |
| `**bold**`, `*italic*`, `` `code` ``, `~~strike~~` | Bold, italic, code on a darker tone, struck through, without the marks |
| `[text](https://example.org)` | The link text, underlined |
| `- item`, `1. item` | A bullet, or the number |
| `> quote` | A quote with a bar at its left |
| `---` | A rule |
| `$x^2$` and display formulas | The typeset formula |
| `![alt](figures/plot.svg)`, `#image("...")`, `#figure(...)` | The image |
| Front matter between `---` lines | A single "Front matter" line |
| Typst `#set`, `#show`, `#let`, `#import` | Left as source text on a darker tone, never hidden |

Anything Manang does not recognise stays as plain source. A broken formula stays as its source with a wavy underline.

## 3. The caret shows the source

Where the caret is, you see the real text so you can edit it. Put the caret in a bold word and its `**` marks appear; move away and they hide again. A heading shows its `#` while the caret is on its line. A formula, image, rule or front matter turns back into its source while the caret is in it. The caret never lands inside a hidden mark.

Selecting, dragging, typing with an input method (IME), undo and redo work as in Source. **Copy gives the plain source text**, marks included.

Double-click an image that is a figure exported from the figure studio to open the figure. Double-click any other image to see its source.

## 4. The toolbar

In Visual mode one row of buttons appears under the tab strip. Hover for the name and shortcut.

| Button | Does |
| :--- | :--- |
| **Bold** `Ctrl+B`, **Italic** `Ctrl+I`, **Code**, **Strikethrough** | Wrap the selection (or the word at the caret); press again inside the formatting to remove it |
| **Heading** | A menu: Paragraph, Heading 1 to 6. Choosing the current level clears it |
| **Bulleted list**, **Numbered list**, **Quote** | Add or remove the marker on the selected lines |
| **Link** | Asks for the address, then wraps the selection |
| **Insert image or figure** | Lists the project's figures and images, or take another path. In Typst it inserts a `#figure` with a caption |
| **Formula** | Inline or display |
| **Horizontal rule** | Inserts a rule after the current line (Markdown) |

A button looks pressed when the caret is inside that formatting. Every button is one edit: `Ctrl+Z` undoes it in one step, separate from your typing.

## 5. Editing together

Visual works in a live session ([Edit together live](edit-together-live.md)). A collaborator's edits are drawn the same way as yours, and your undo only takes back your own changes.

## 6. If something goes wrong

If the Visual engine cannot read a file, or its answer is not valid, the file shows as plain source and the toolbar says so. Nothing in the file is lost or changed. Switch to Source, or switch Visual off and on again to start fresh.
