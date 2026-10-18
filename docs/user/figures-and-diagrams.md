# Figures and Diagrams

You can sketch vector figures in the built-in figure studio, write text-based diagrams in Mermaid, Pikchr, and ASCII format, and include them in Typst, LaTeX, and Markdown documents.

## 1. Use the vector figure studio

Manang includes an in-house vector figure studio built specifically for scientific figures. Figures are stored as human-readable JSON scene files (`<name>.figure`), and saving automatically produces a companion deterministic SVG (`<name>.svg`) alongside a companion PDF for LaTeX documents. (Existing `<name>.figure.json` files continue to open transparently.)

### Create a new figure
1. Choose **File > New figure** from the menu bar.
   - Or open the command palette (`Ctrl+K` (`Cmd+K`)), type `New figure`, and press `Enter`.
   - Or click the **Figures** icon in the Activity bar and click the **+** (New figure) button in the panel header.
2. Enter a figure name (for example, `circuit`).
3. Manang creates `figures/circuit.figure` and opens it in an editor tab.

### Tools and keyboard shortcuts
Select tools from the top toolbar or press single keys while the canvas is focused:

- **Select (`V` or `1`):** Select, move, and transform shapes. Click a shape to select it, or drag a selection box around multiple shapes.
- **Hand (`H` or `2`):** Pan the canvas without selecting shapes.
- **Rectangle (`R` or `3`):** Drag on the canvas to draw rectangular boxes.
- **Diamond (`D` or `4`):** Drag to create flowchart decision diamonds.
- **Ellipse (`O` or `5`):** Drag to draw circles and ellipses.
- **Arrow (`A` or `6`):** Drag between shapes to create directional connectors.
- **Line (`L` or `7`):** Drag to create straight connector lines without arrowheads.
- **Draw (`P` or `B` or `8`):** Freehand ink pen with pressure-sensitive strokes for drawing tablets or stylus pens.
- **Text (`T` or `9`):** Click to insert text labels. Double-click any existing label to edit.
- **Frame (`F`):** Drag to draw labeled container frames that group and organize nested elements.
- **Eraser (`E`):** Click or drag across shapes to delete them.
- **Laser pointer (`K`):** Temporary glowing laser trail for live presentations and walkthroughs.
- **Formula (`M`):** Insert mathematical formulas typeset with Typst or LaTeX syntax, rendered deterministically as vector math nodes.

### Bound arrows and connectors
1. Drag with the **Arrow (`A`)** or **Line (`L`)** tool from one shape to another.
2. The connector automatically snaps and binds to anchor points on the target shapes (top, right, bottom, left).
3. When you move, resize, or transform connected shapes, bound arrows dynamically update their trajectory and endpoints.
4. With a connector selected, use the properties panel to select arrowhead styles: *none*, *arrow*, *triangle*, *dot*, or *bar*.

### Canvas manipulation and arrangement
- **Pan the canvas:** Hold `Space` and drag with the left mouse button, drag with the middle mouse button, or scroll with two fingers.
- **Zooming:**
  - `Ctrl+=` (`Cmd+=`): Zoom in
  - `Ctrl+-` (`Cmd+-`): Zoom out
  - `Ctrl+0` (`Cmd+0`): Zoom to 100% original scale
  - `Shift+1`: Zoom to fit all shapes on canvas
  - `Shift+2`: Zoom to current selection
- **Nudge:** Press arrow keys to nudge selected shapes by 1 point. Hold `Shift` with arrow keys to nudge by 10 points.
- **Move without snapping:** Hold `Alt` (or `Option`) while dragging shapes to bypass anchor snapping.
- **Clipboard:** Press `Ctrl+C` (`Cmd+C`) to copy selected shapes, and `Ctrl+V` (`Cmd+V`) to paste them. Press `Ctrl+D` (`Cmd+D`) to duplicate directly.
- **Grouping:** Select multiple shapes and press `Ctrl+G` (`Cmd+G`) to group them. Press `Ctrl+Shift+G` (`Cmd+Shift+G`) to ungroup.
- **Locking:** Press `Ctrl+Shift+L` (`Cmd+Shift+L`) to lock shapes in place and prevent accidental dragging.
- **Z-Order:** Bring to front with `Ctrl+Shift+]`, send to back with `Ctrl+Shift+[`, or step forward/backward with `Ctrl+]` / `Ctrl+[`.
- **Undo and Redo:** Press `Ctrl+Z` to undo and `Ctrl+Shift+Z` (or `Ctrl+Y`) to redo.

### Shape styling and colors
When one or more shapes are selected, the context properties panel lets you configure styles:
- **Stroke widths:** Choose between hairline (`1 pt`), regular (`1.5 pt`), medium (`2.5 pt`), and bold (`4 pt`).
- **Tonal palette & custom colors:** Select stroke and fill colors from the balanced scientific palette (*ink*, *muted*, *red*, *orange*, *yellow*, *green*, *teal*, *blue*, *purple*, *magenta*), or enter custom hex color values (`#rrggbb`). The palette automatically saves recent custom colors for quick reuse.

### Import Mermaid diagrams
Click **Insert Mermaid** in the figure toolbar to open the Mermaid importer. Enter your diagram definition to import it directly onto the canvas as editable shapes and bound arrows.

### Save and embed figures
1. Press `Ctrl+S` (`Cmd+S`) or click **Save** in the figure toolbar.
2. Saving writes `<name>.figure` and produces the companion vector `<name>.svg` directly beside it.
3. Click **Copy embed line** in the figure toolbar.
4. Paste the generated inclusion snippet directly into your Typst, LaTeX, or Markdown source file.

---

## 2. Write text diagrams

Text diagrams let you maintain diagrams directly in version control. Saving a diagram file writes a crisp, deterministic vector SVG (and companion PDF for LaTeX) next to it in your project's `figures/` directory.

### Supported diagram languages
- **Mermaid (`.mmd`, `.mermaid`):** Flowcharts, sequence diagrams, class diagrams, state machines, and Gantt charts.
- **Pikchr (`.pikchr`):** Clean diagrams using SQLite's PIC-like diagram language.
- **ASCII diagrams (`.bob`):** Hand-drawn plain-text ASCII boxes, arrows, and circles rendered as clean SVGs via `svgbob`.

### Create a diagram file
1. Choose **File > New diagram** from the menu bar (or open the command palette `Ctrl+K` (`Cmd+K`) and type `New diagram`).
2. Choose a template:
   - **Mermaid flowchart:**
     ```text
     flowchart TD
       A[Start] --> B[Measure]
       B --> C[Done]
     ```
   - **Mermaid sequence:**
     ```text
     sequenceDiagram
       participant Author
       participant Manang
       Author->>Manang: Write
       Manang-->>Author: Preview
     ```
   - **Pikchr:**
     ```text
     box "Input" fit
     arrow
     box "Process" fit
     arrow
     box "Output" fit
     ```
   - **ASCII (`svgbob`):**
     ```text
     +--------+     +---------+
     | Source |---->| Picture |
     +--------+     +---------+
     ```
3. Enter a name and click **Create**.
4. Edit the diagram source in the CodeMirror editor. The live preview on the right renders the SVG output.
5. If there is a syntax error, Manang reports the exact line and column in the **Problems** panel (`Ctrl+Shift+M` (`Cmd+Shift+M`)).
6. Press `Ctrl+S` (`Cmd+S`) to save. Manang writes `<name>.svg` (and `<name>.pdf` in LaTeX projects) directly beside the source file.

---

## 3. Fenced diagrams in Markdown

You can embed text diagrams directly inside Markdown documents without creating standalone files.

1. Open a Markdown (`.md`) document.
2. Add a fenced code block specifying the diagram language:
   ````markdown
   ```mermaid
   flowchart LR
     A[Raw Data] --> B[Processing]
     B --> C[Visualisation]
   ```
   ````
3. You can also use ```` ```pikchr ````, ```` ```bob ````, or ```` ```svgbob ```` fences.
4. The live preview automatically renders the diagram inline as an SVG image.

---

## 4. Include figures in documents

When editing any diagram file or vector figure, choose **Edit > Copy include line** (`doc.copy-include`) or click the toolbar button to copy the correct inclusion markup for your project engine.

### Include in Typst
```typst
#figure(
  image("figures/circuit.svg"),
  caption: [Circuit schematic],
) <fig-circuit>
```
Reference the figure in your text using `@fig-circuit`.

### Include in LaTeX
```latex
\begin{figure}[htbp]
  \centering
  \includegraphics[width=0.85\textwidth]{figures/circuit.pdf}
  \caption{Circuit schematic}
  \label{fig:circuit}
\end{figure}
```
Reference the figure in your text using `\ref{fig:circuit}`.

### Include in Markdown
```markdown
![Circuit schematic](figures/circuit.svg)
```
