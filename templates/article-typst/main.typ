#set document(title: "{{title_string}}")
#set page(paper: "a4", margin: (x: 2.5cm, y: 3cm))
#set text(size: 11pt)
#set heading(numbering: "1.1")

#align(center)[
  #text(size: 20pt, weight: "bold")[{{title}}]

  #v(0.5em)
  #text(size: 11pt, fill: rgb("#666666"))[Author Name]

  #text(size: 10pt, fill: rgb("#888888"))[Department of Computer Science \ University Research Lab]
]

#v(1em)

#block(
  inset: (x: 2cm),
  [
    #set text(size: 10pt)
    *Abstract* --- This document provides a foundational template for academic writing in Typst. It includes standard sections, equations, citations, and an automated bibliography.
  ]
)

#v(1em)

= Introduction

Scientific communication requires clear structure, reproducible formatting, and precise citation management. Writing in Typst enables instant feedback while maintaining publication-grade typography @knuth1984.

= Methodology

We formulate the analysis using standard mathematical expressions:

$ Delta S >= integral_0^t (delta Q)/T $

= Results

The empirical results validate the theoretical predictions across all tested conditions.

= Conclusion

This template provides an extensible foundation for scientific articles.

#bibliography("references.bib")
