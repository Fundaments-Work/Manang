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
The toolbar is one row in groups: Select, Hand, Eraser and Laser; the shapes; Arrow, Line and Draw; Text, Formula and Mermaid; Undo and Redo; Zoom. Hover any button to see its shortcut. Tab enters the toolbar on the active tool and the arrow keys move along it. Press single keys while the canvas is focused to pick a tool:

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
4. With a connector selected, the **Stroke** section of the inspector sets its route (*straight*, *elbow*, *curved*) and the start and end heads: *none*, *arrow*, *triangle*, *dot*, or *bar*.

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

### The layers panel
The panel on the left lists every node of the figure, topmost first, with an icon for its type, its name, and lock and visibility toggles. Collapse it from its header to give the canvas room.

- **Select:** click a layer to select it, `Shift`+click for a range, `Ctrl`+click (`Cmd`+click) to add or remove one. The selection stays in step with the canvas in both directions, and a layer you pick on the canvas scrolls into view.
- **Rename:** double-click a name or press `F2`. A node has no separate name in the file: its label is its name, so renaming edits the label. Formulas and drawings take their name from their source or type. Without a label a layer is called by its type and id (*Rectangle n3*).
- **Lock and hide:** the padlock locks a layer so the canvas ignores it (you can still select it here to unlock it); the eye hides it. A hidden layer is not drawn, not exported and not selectable on the canvas, and is saved as `"hidden": true` in the scene file.
- **Reorder:** drag a layer above or below another (a line shows where it lands), or focus it and press `Alt`+`Up` or `Alt`+`Down`. Each move is one undo step.
- **Filter:** the magnifier in the header turns the title into a filter field (by name or type); `Escape` clears it.
- The list is a keyboard listbox: `Up` and `Down` move, `Home` and `End` jump, `Enter` or `Space` selects, `F2` renames.

### The inspector: shape styling and colors
The panel on the right depends on the selection. With nothing selected it shows the figure itself: **Canvas** (size and background), **Grid and snapping** (a grid behind the page, and snapping to the edges and centres of other shapes) and **Export** (download the SVG). Snapping and the grid are preferences of your Manang, not part of the figure file.

With a selection it shows sections in a fixed order. Click a section header to collapse it; Manang remembers which are collapsed.

- **Layout:** X, Y, width and height in pixels, and rotation for shapes. For several nodes X, Y, W and H describe the box around them, and changing the size scales them all. *Keep proportions* locks the aspect ratio.
- **Appearance:** overall opacity.
- **Fill:** the fill colour and its own opacity (shapes only).
- **Stroke:** colour, width, dash (*solid*, *dashed*, *dotted*), and for arrows and lines the route and heads.
- **Text:** label, font (*sans* or *mono*), size, alignment and text colour. For a formula this holds its source, language and size; for a Mermaid block, its source.
- **Effects:** *clean*, *sketchy* or *rough* hand-drawn style.
- **Arrange:** align, distribute, bring forward or back, group and ungroup, lock, delete.

**Numeric fields** have their label inside. Drag the label sideways to change the value (one step per 4 pixels; hold `Shift` for ten times as much), press `Up` or `Down` to step it (`Shift` for ten), `Enter` to apply and `Escape` to put the old value back. You can type a unit (`24px`, `45°`) or a short sum (`120+16`). When the selected shapes disagree about a setting the field says **Mixed**; typing a value sets it on all of them as one undo step.

**Colours** are one row of swatches: the palette tokens first (*ink*, *muted*, *accent*, *orange*, *sky*, *green*, *yellow*, *blue*, *vermillion*, *purple*, and for fills *none*, *surface*, *paper*), which follow the theme and print well. The **+** opens a picker beside the inspector for any `#rrggbb` colour. Your recent custom colours follow the tokens in a second row; they do not follow the theme and are remembered per user.

In a narrow window (under about 860 pixels for the figure tab) the panels do not squeeze the canvas: the toolbar gets **Layers** and **Inspector** buttons that open them over it, and `Escape` closes them.

### Import Mermaid diagrams
Click the **Mermaid** button in the figure toolbar (tooltip: *Insert Mermaid*) to open the Mermaid importer. Enter your diagram definition to import it directly onto the canvas as editable shapes and bound arrows.

### Save and embed figures
1. Press `Ctrl+S` (`Cmd+S`) or click **Save** at the right of the figure toolbar.
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
