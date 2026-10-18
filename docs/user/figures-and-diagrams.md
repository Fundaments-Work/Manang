# Figures and Diagrams

You can sketch vector figures, write text-based diagrams in Mermaid, Pikchr, and ASCII format, and include them in Typst, LaTeX, and Markdown documents.

## 1. Use the figure editor

Manang includes a built-in vector figure editor. Figures are saved as readable JSON scenes (`<name>.figure.json`), and saving automatically produces a companion deterministic SVG (`<name>.svg`) alongside a PDF for LaTeX projects.

*The refreshed figure studio is coming soon.*

### Create a new figure
1. Open the command palette (`Ctrl+K` (`Cmd+K`)), type `New figure`, and press `Enter`.
   - Or click the **Figures** icon in the Activity bar and click the **+** (New figure) button in the panel header.
2. Enter a figure name (for example, `lattice`).
3. Manang creates `figures/lattice.figure.json` and opens it in an editor tab.

### Draw on the canvas
Select tools using the toolbar above the canvas or by pressing single-letter keys while the canvas is focused:
1. **Select (`P`):** Click any shape to select it. Drag to move it, or press `Delete` to remove it. Press the arrow keys to nudge the selection by 1 point, or hold `Shift` to nudge by 10 points.
2. **Freehand pen (`B`):** Draw variable-width ink strokes. If you use a stylus or drawing tablet, Manang records natural pen pressure.
3. **Rectangle (`G`):** Drag on the canvas to draw rectangular nodes and frames.
4. **Ellipse (`E`):** Drag to draw circles and ellipses.
5. **Arrow (`A`):** Drag between shapes. Arrowheads automatically snap to shape anchor points.
6. **Math formula (`M`):** Click to insert a mathematical formula. Double-click an existing formula (or press `Enter` while it is selected) to edit the LaTeX expression in the toolbar input box.

### Save and embed figures
1. Press `Ctrl+S` (`Cmd+S`) or click **Save** in the toolbar.
2. Click **Copy embed line** in the figure toolbar.
3. Paste the line into your document.

## 2. Write text diagrams

Text diagrams let you maintain diagrams directly in version control. Saving a diagram file writes a crisp, deterministic vector SVG (and companion PDF for LaTeX) next to it in your project's `figures/` directory.

### Supported diagram languages
- **Mermaid (`.mmd`, `.mermaid`):** Flowcharts, sequence diagrams, class diagrams, state machines, and Gantt charts.
- **Pikchr (`.pikchr`):** Clean diagrams using SQLite's PIC-like diagram language.
- **ASCII diagrams (`.bob`):** Hand-drawn plain-text ASCII boxes, arrows, and circles rendered as clean SVGs via `svgbob`.
- *Support for PlantUML (`.puml`, `.plantuml`) and Ditaa diagrams is coming soon.*

### Create a diagram file
1. Open the command palette (`Ctrl+K` (`Cmd+K`)), type `New diagram`, and press `Enter`.
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

## 4. Include figures in documents

When editing any diagram file or vector figure, run **Copy include line** (`doc.copy-include`) from the command palette (`Ctrl+K` (`Cmd+K`)) or click the copy button to copy the correct inclusion markup for your project engine.

### Include in Typst
```typst
#figure(
  image("figures/lattice.svg"),
  caption: [Crystal lattice simulation],
) <fig-lattice>
```
Reference the figure in your text using `@fig-lattice`.

### Include in LaTeX
```latex
\begin{figure}[htbp]
  \centering
  \includegraphics[width=0.85\textwidth]{figures/lattice.pdf}
  \caption{Crystal lattice simulation}
  \label{fig:lattice}
\end{figure}
```
Reference the figure in your text using `\ref{fig:lattice}`.

### Include in Markdown
```markdown
![Crystal lattice simulation](figures/lattice.svg)
```
