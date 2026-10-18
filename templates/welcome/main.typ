#set document(title: "{{title_string}}")
#set page(paper: "a4", margin: (x: 2.5cm, y: 2.5cm))
#set text(size: 10.5pt)
#set par(justify: true)
#set heading(numbering: "1.1")

#align(center)[
  #text(size: 20pt, weight: "bold")[{{title}}]

  #v(0.4em)
  #text(size: 10.5pt, fill: rgb("#71717a"))[An Interactive Onboarding Tour of the Manang Authoring Suite]
]

#v(1em)

#block(
  fill: rgb("#f4f4f5"),
  inset: 12pt,
  radius: 0pt,
  stroke: 1pt + rgb("#e4e4e7"),
  [
    *Welcome to Manang!* This document teaches you the core authoring workflow by letting you interact with it directly. Follow the numbered steps below.
  ]
)

= The First 5 Minutes: Try This

+ *Edit text live:* Edit this line of text right now. Notice that the preview updates instantly with sub-50ms latency.
+ *SyncTeX navigation:* In the preview panel on the right, double-click or `Ctrl`-click (`Cmd`-click on macOS) any text or heading to jump straight back to its exact source line in this editor. Press `Ctrl+Alt+J` (`Cmd+Alt+J`) to jump forward to preview.
+ *Command palette:* Press `Ctrl+K` (`Cmd+K` on macOS) to open the palette. Filter available actions by typing `export`, `settings`, or `/`.
+ *Settings & scope:* Press `Ctrl+,` (`Cmd+,` on macOS) to open Settings. Switch between global User preferences and project-specific configuration.
+ *Save a version:* Local history snapshots and git-backed versioning ("Save a version") are coming soon in a later beta release.

#include "sections/tour.typ"

= Mathematical Typesetting

Equations in Typst are concise, legible, and compiled with publication precision @shannon1948:

$ Delta S >= integral_0^t (delta Q) / T $

$ e^(i pi) + 1 = 0 $

= Vector Figures

Vector illustrations and diagrams integrate directly into documents without rasterization:

#align(center)[
  #figure(
    image("figures/welcome.svg", width: 85%),
    caption: [Architecture of the Manang authoring engine: zero-copy IPC and instant live preview.],
  )
]

= Keyboard Shortcuts Reference

#table(
  columns: (auto, auto, 1fr),
  stroke: 0.5pt + rgb("#d4d4d8"),
  inset: 7pt,
  table.header([*Action*], [*Shortcut*], [*Description*]),
  [Command Palette], [`Ctrl+K` (`Cmd+K`)], [Fuzzy search all commands and tools],
  [Settings], [`Ctrl+,` (`Cmd+,`)], [View and modify user or project options],
  [Export PDF], [`Ctrl+E` (`Cmd+E`)], [Compile and save publication-ready PDF],
  [Toggle Preview], [`Ctrl+Shift+V` (`Cmd+Shift+V`)], [Show, hide, or split preview pane],
  [Shortcut Sheet], [`Ctrl+/` (`Cmd+/`)], [Browse the complete workbench shortcut list],
)

= References

Literature citations reference `references.bib` using standard citation keys @knuth1984.

#bibliography("references.bib")
