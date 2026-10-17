# DESIGN.md — Visual Design & UI/UX Master Specification

**Project:** Manang  
**Role:** Canonical Visual Design & UI/UX System Specification  
**Design System Authority:** [Fundaments.work Design System](file:///run/media/sapiens/Development/Fundaments/design-system)  
**Version:** 3.0.0 (Visual Master Plan)  

---

## 1. Product Thesis: "Paper & Pen" Immediacy

Manang is designed to be the tool people instinctively open whenever they think of document preparation—academic papers, conference slide decks, technical reports, mathematical notes, or books.

Traditional authoring tools bury the writer in technical friction: arcane LaTeX package errors, shifting margins, multi-button toolbars, and laggy compilation loops.

**Manang's core design promise is radical simplicity:** writing in Manang feels as immediate, natural, and calm as opening a blank notebook and writing with a fountain pen. Under the hood, a high-performance Rust core and WebAssembly plugin architecture do all the heavy lifting, but the user interface remains completely quiet, focused, and frictionless.

---

## 2. Visual Identity & Design Tokens

Manang is strictly governed by the **Fundaments.work Design System**: *"Square, quiet, and set in two weights."*

### 2.1. The Mark & Identity
* **The Mark (`[⩓]`):** Constructed on a strict 24-unit geometric grid with zero corner radius. Two upright pilasters bridged by an apex chevron, evoking both an open book and an ink nib.
* **The Wordmark:** Set cleanly in **DM Sans 300** (`font-light`, tracking `-0.02em`, 15px). Unadorned, self-contained, confident.
* **Credit Attribution:** Placed respectfully in settings and about dialogues as `Made by Fundaments.work` in DM Mono 400.

### 2.2. Foundational Tokens & Invariants
* **Zero Corner Radius (`--radius: 0`):** Every element terminates in sharp 90-degree right angles without exception. No rounded rectangles, no pills, no circles.
* **Hairline Dividers (`--hairline: 1px`):** Depth is created exclusively via 1px hairline boundaries (`#3d3d42` dark, `#d4d3ce` light) and tonal surface steps, rejecting all drop shadows, ambient glows, or dimensional blurring.
* **Typography (Two Families, Two Weights):**
  - **DM Sans (300 Light & 400 Regular):** Interface chrome, headings, dialogs, and narrative prose.
    - Display: Sans 300, tracking `-3%` (`-0.03em`)
    - Headline: Sans 300, tracking `-2%` (`-0.02em`)
    - Body: Sans 400, tracking `-1%` (`-0.01em`)
  - **DM Mono (400 Regular):** Data, code editor buffers, line numbers, coordinate readouts, citations, and metadata.
* **Color Hierarchy:**
  - **Dark Base (Default):** Background `#141416`, Surface `#1c1c20`, Surface-Hi `#28282c`, Line `#3d3d42`, Text Primary `#e8e6e1`, Text Secondary `#9a9a9a`.
  - **Light Paper Override:** Page `#faf9f7`, Surface `#f4f3f0`, Line `#d4d3ce`, Ink `#1c1c1c`, Ink-60 `#6b6b6b`.
  - **Accent:** **Acid lime (`#c6f135`)** is the mark and one action per view (one signal per frame), used for active selection and focus. Brand page is the source of truth. Always paired with a text label or icon.
  - **Secondary:** **Sage (`#b3d17a`)**, used sparingly for math for mathematical equation delimiters and coordinates.

---

## 3. Screen 1: The Desk (The Daily Writing Home)

The Desk is the primary workspace where authors spend 90% of their time. It balances a distraction-free writing surface with a live rendered paper sheet.

```
┌────────────────────────────────────────────────────────────────────────┐
│ [⩓] Manang     papers / quantum_entropy.typ      [✎ Draw]  [◫ Split]   │
├───────────────────────────────────┬────────────────────────────────────┤
│ 140  // Non-equilibrium Entropy   │ 2. Quantum Thermodynamic Bounds    │
│ 141  = Irreversible Dynamics      │                                    │
│ 142  Under unitary coupling, the  │ Under unitary coupling, the        │
│ 143  rate $Sigma$ satisfies:      │ entropy production rate satisfies: │
│ 144                               │                                    │
│ 145  $ Delta S_text("irr") >=     │      ΔS_irr ≥ ∫ (δQ / T) + k_B D   │
│ 146    integral_0^t (delta Q)/T $ │                                    │
│  ·   [p.14 missing in @penrose]   │ [1] Landauer, R. IBM J. (1961)     │
│ 148  This generalizes Landauer.   │                                    │
└───────────────────────────────────┴────────────────────────────────────┘
```

### 3.1. Header Bar (h-10, px-4, Border-b 1px `#3d3d42`)
* **Left:** Mark `[⩓]` + Wordmark `Manang` (DM Sans 300).
* **Center:** Breadcrumb trail (`papers / quantum_entropy.typ`) in DM Mono text-xs text `#9a9a9a`.
* **Right (Responsive Actions):**
  - `[✎ Draw]` — Toggles the Canvas overlay.
  - `[◫ Split]` — Toggles single-pane vs. dual-pane paper preview.
  - `[⛶ Zen]` — Fullscreen distraction-free mode.
  - When screen width drops below 1024px, buttons collapse automatically to icon-only buttons (`[✎]`, `[◫]`, `[⛶]`) with instant tooltips.

### 3.2. Left Pane: The Writing Surface
* **Editor Core:** Virtualized CodeMirror 6 text buffer in DM Mono 400 (13px, 1.6 line height).
* **Quiet Syntax Highlighting:**
  - Prose: High-contrast `#e8e6e1`.
  - Math Delimiters (`$ ... $`): Highlighted in Sage (`#b3d17a`).
  - Headings (`= Section`): Clean DM Sans 300 warm ivory.
  - Functions & Commands: Acid lime (`#c6f135`).
  - Comments: Low-contrast `#7a7a7a` italic.
* **Margin Whispers (Diagnostics):**
  - Missing brackets, broken references, or warnings never trigger modal popups or terminal dumps.
  - A subtle 4px square dot appears in the line number gutter.
  - Hovering reveals a quiet one-line callout: `Missing page locator in @penrose` with a 1-click `[Auto-Fix]` action.

### 3.3. Right Pane: The Live Paper Sheet
* **Real-Time Rendering:** The document updates in real time as ink dries on paper.
* **Bidirectional Sync (SyncTeX):**
  - Clicking any sentence or equation in the paper instantly jumps the cursor to that line in the manuscript.
  - Typing in the manuscript smoothly aligns the preview page.

---

## 4. Screen 2: The Canvas (Natural Sketching & Vector Studio)

Triggered instantly by typing `/draw` anywhere in the manuscript or clicking `[✎ Draw]`. The canvas overlay glides in over the writing pane.

```
┌────────────────────────────────────────────────────────────────────────┐
│ [⩓] Manang     figures / vectors / state_lattice.svg      [Done ↵] [✕] │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│               ┌──────────┐                                             │
│               │ State |0⟩│ ───(Hamiltonian H)───► [ State |1⟩ ]        │
│               └──────────┘                                             │
│                                                                        │
│  [ ✎ Freehand ]   [ □ Geometry ]   [ → Arrow ]   [ T Math ]   [ ⌫ ]    │
└────────────────────────────────────────────────────────────────────────┘
```

### 4.1. Design & Interaction Affordances
* **The Overlay Transition:** Glides smoothly into view without modal borders or backdrop darkness.
* **The Pen Tray (Bottom Floating HUD):**
  - Centered floating bar (h-9, bg `#141416`, 1px hairline `#3d3d42`, zero radius).
  - **`[ ✎ Freehand ]`:** Natural sketching powered by Manang's own vector ink engine (Svelte scene graph + `perfect-freehand` stroke geometry, rendered as native SVG).
  - **`[ □ Geometry ]`:** Sharp mathematical shapes (rectangles, ellipses, coordinate grids).
  - **`[ → Arrow ]`:** Directional connectors that magnetically snap to node boundaries.
  - **`[ T Math ]`:** Type any LaTeX/Typst formula (`$|\psi\rangle$`) to stamp rendered mathematical text directly onto the figure.
* **Embedding the Ink:**
  - Pressing `Enter` or clicking `[Done ↵]` quietly closes the canvas.
  - The drawing is automatically saved as a vector `.svg` / `.pdf` in the project's `figures/` folder and embedded into the manuscript:
    ```typst
    #figure(image("figures/state_lattice.svg", width: 80%))
    ```
  - Double-clicking the figure in the document re-opens the canvas at any time for editing.

---

## 5. Screen 3: The Library Shelf (Document Organization)

Accessible via `Cmd+P` or clicking the shelf icon `[≡]`. A clean, quiet rail sliding out from the left.

```
┌──────────────────────────────────────┐
│ [≡] LIBRARY SHELF                [✕] │
├──────────────────────────────────────┤
│ ▾ papers                             │
│   📄 01_formalism.typ       (Active) │
│   📄 02_entropy_bounds.typ           │
│   📄 03_numerical_sim.typ            │
│ ▾ figures                            │
│   🎨 state_lattice.svg               │
│   🎨 dissipation_curve.svg           │
│ ▾ bibliography                       │
│   📚 references.bib        (3 items) │
├──────────────────────────────────────┤
│ [+ New Document]     [+ New Diagram] │
└──────────────────────────────────────┘
```

### 5.1. Design Affordances
* **Spatial Layout:** 260px wide, dark background (`#141416`), 1px right border (`#3d3d42`).
* **Active State:** The active document is highlighted with a 2px lime left boundary and surface fill (`#1c1c20`).
* **Instant Filter:** Type immediately to fuzzy-search across files, sections, and figures.

---

## 6. Screen 4: The Instrument Shelf (Plugin Manager)

Manang is **plugin-first**. Everything—compilers, drawing engines, citation verifiers, slide frameworks—is an isolated WebAssembly instrument. The Instrument Shelf makes extensions approachable and clean.

```
┌────────────────────────────────────────────────────────────────────────┐
│ [⩓] Manang     INSTRUMENT SHELF (PLUGINS)                          [✕] │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│  ┌──────────────────────────────┐    ┌──────────────────────────────┐  │
│  │ [✓] Typst Compiler Engine    │    │ [✓] Vector Ink Engine       │  │
│  │ Native in-memory compilation │    │ Infinite vector sketch pad   │  │
│  │ v0.12.0 · Core Instrument    │    │ v1.4.0 · Drawing Plugin      │  │
│  └──────────────────────────────┘    └──────────────────────────────┘  │
│  ┌──────────────────────────────┐    ┌──────────────────────────────┐  │
│  │ [✓] CeTZ Mathematical Vectors│    │ [✓] Touying Slide Decks      │  │
│  │ Precise coordinate geometry  │    │ 16:9 presentation studio     │  │
│  │ v0.3.1 · Graphic Plugin      │    │ v0.5.2 · Presentation Plugin │  │
│  └──────────────────────────────┘    └──────────────────────────────┘  │
│                                                                        │
│  [+ Install WASM Plugin (.wasm)]              [Browse Curated Lab]     │
└────────────────────────────────────────────────────────────────────────┘
```

### 6.1. Design Affordances
* **Clean Instrument Cards:** 1px hairline border (`#3d3d42`), surface `#1c1c20`, zero radius.
* **Toggle Controls:** Square zero-radius checkboxes. Checking or unchecking enables or sandboxes capabilities instantly.
* **Capability Transparency:** Hovering over any plugin card reveals its permissions in clean DM Mono: `Buffer: read | Net: none | FS: figures/`.

---

## 7. Screen 5: The Command & Co-Pilot Palette (`Cmd+K`)

The single, universal keyboard launcher uniting navigation, plugin tools, and AI actions into one quiet dialog.

```
┌────────────────────────────────────────────────────────────────────────┐
│  > Ask or type a command...                                            │
├────────────────────────────────────────────────────────────────────────┤
│  AI ACTIONS                                                            │
│  ✨ Verify citations against Crossref & OpenAlex                       │
│  ✨ Generate 6 Touying presentation slides from Section 2              │
│  ✨ Add commutative diagram in CeTZ                                    │
│                                                                        │
│  PLUGINS & COMMANDS                                                    │
│  ✎ Open Drawing Canvas (/draw)                                         │
│  ◫ Toggle Split Screen (/split)                                        │
│  ⬇ Export Publication PDF (/export)                                    │
│  ⚙ Open Instrument Shelf (/plugins)                                    │
└────────────────────────────────────────────────────────────────────────┘
```

### 7.1. Design Affordances
* **Placement:** Centered floating modal (600px wide, zero radius, 1px border `#3d3d42`, bg `#1c1c20`).
* **Keyboard-First:** Navigate with `↑` and `↓`, execute with `Enter`, dismiss with `Esc`.
* **Zero Jargon:** Prompts read as direct human actions (*"Verify citations"*, *"Generate slides"*).

---

## 8. Screen 6: The Presentation Studio (Slide Mode)

When authoring slides using Typst **Touying** or **Polylux**, the live sheet adapts into a 16:9 presentation studio.

```
┌────────────────────────────────────────────────────────────────────────┐
│ [⩓] Manang     slides / quantum_talk.typ           [Present F5]  [✕]   │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │  QUANTUM THERMODYNAMICS · §2                      SLIDE 3 / 6  │   │
│   │                                                                │   │
│   │  Landauer Bound in Quantum Regimes                             │   │
│   │                                                                │   │
│   │  Minimal heat dissipated per erased bit of physical entropy:   │   │
│   │                                                                │   │
│   │          ΔQ_diss ≥ k_B · T · ln 2 + D(ρ || ρ_eq)               │   │
│   │                                                                │   │
│   │  [1] Landauer, R. (1961) IBM J. Res. Dev.                      │   │
│   └────────────────────────────────────────────────────────────────┘   │
│                                                                        │
│   [ 01. Title ]  [ 02. Formalism ]  [ 03. Landauer ]  [ 04. Data ]     │
└────────────────────────────────────────────────────────────────────────┘
```

### 8.1. Design Affordances
* **16:9 Canvas:** Framed in a clean `#141416` dark background with a 1px border `#3d3d42`.
* **Bottom Slide Carousel:** Miniature cards for rapid jumping across slides with an active lime hairline indicator.
* **One-Click Presentation:** Pressing `F5` launches distraction-free full-screen presentation mode with instant keyboard slide navigation.

---

## 9. Screen 7: Zen Mode (Distraction-Free Deep Focus)

Triggered via `Cmd+\` or `F11`.

```
┌────────────────────────────────────────────────────────────────────────┐
│                                                                        │
│                                                                        │
│        = Quantum Thermodynamic Bounds                                  │
│                                                                        │
│        Under unitary coupling to a heat reservoir at inverse           │
│        temperature β = (k_B T)⁻¹, the irreversible entropy             │
│        production rate Σ satisfies the finite-time lower bound:        │
│                                                                        │
│        $ Delta S_text("irr") >= integral_0^t (delta Q) / T $           │
│                                                                        │
│        This confirms that quantum coherence retains finite             │
│        work capacity even under thermal dissipation.                   │
│                                                                        │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

### 9.1. Design Affordances
* **Zero UI Chrome:** Header, file rails, status indicators, and preview panes smoothly dissolve away.
* **Centered Paper Column:** 680px optimal reading column width, keeping lines within comfortable 70–80 character measure.
* **Instant Reversibility:** Pressing `Esc` or `Cmd+\` seamlessly brings back the Desk view.

---

## 10. Summary of UI/UX Invariants & Quality Standards

| Dimension | Rule | Rationale |
| :--- | :--- | :--- |
| **Corner Radius** | `0px` universally (`--radius: 0`) | Strictly complies with Fundaments.work identity. Architectural, quiet, and serious. |
| **Borders** | `1px` hairlines (`#3d3d42`) | Structural containment without visual weight or fuzzy shadows. |
| **Speed Indicators** | **Hidden** (no millisecond counters) | Speed is an invisible reality. Users care about flow, not benchmark telemetry. |
| **Error Handling** | Margin Whispers | Low-cognitive-load gutter squares with 1-click fixes, replacing intimidating terminal dumps. |
| **Drawing** | Natural `/draw` sheet | Eliminates file management friction; drawings are vector ink embedded directly in prose. |
| **Typography** | DM Sans (300/400), DM Mono (400) | Strictly two typefaces and two weights for maximum typographic calm. |


