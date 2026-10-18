#set page(paper: "a4", margin: (x: 3cm, top: 3cm, bottom: 3cm))
#set text(size: 11pt)
#set par(justify: true, leading: 0.8em)
#set heading(numbering: "1.1")

// Title Page (Page 1)
#align(center + horizon)[
  #text(size: 24pt, weight: "bold")[Thermodynamic Limits of Distributed Consensus]

  #v(2em)
  #text(size: 14pt)[A Dissertation Submitted in Partial Fulfillment of the \ Requirements for the Degree of Doctor of Philosophy]

  #v(3em)
  #text(size: 14pt, weight: "medium")[Alexander M. Wright]

  #v(1em)
  #text(size: 12pt, fill: luma(80))[Department of Computer Science and Engineering \ Institute for Advanced Information Architecture]

  #v(4em)
  #text(size: 11pt)[October 2026]
]

#pagebreak()

// Table of Contents (Page 2)
#outline(depth: 1, indent: auto)

#pagebreak()

// Included Chapters (Pages 3 to 40)
#include "chapters/01_introduction.typ"
#pagebreak()

#include "chapters/02_background.typ"
#pagebreak()

#include "chapters/03_methodology.typ"
#pagebreak()

#include "chapters/04_results.typ"
#pagebreak()

#include "chapters/05_conclusion.typ"
