#set document(title: "{{title_string}}")
#set page(paper: "a1", flipped: false, margin: (x: 2cm, y: 2cm))
#set text(size: 18pt)

#align(center)[
  #block(
    fill: rgb("#f5f5f5"),
    inset: 2cm,
    width: 100%,
    [
      #text(size: 42pt, weight: "bold")[{{title}}] \
      #v(0.5em)
      #text(size: 24pt)[Author Name#super[1], Co-Author Name#super[2]] \
      #v(0.2em)
      #text(size: 18pt, fill: rgb("#555555"))[#super[1]Department of Computer Science, #super[2]Institute of Advanced Studies]
    ]
  )
]

#v(1em)

#columns(3, gutter: 2cm)[
  == 1. Introduction

  Modern scientific communication requires clear visual hierarchy and rapid compilation. As explored by @knuth1984, structured layout enhances comprehensibility in dense research presentations.

  == 2. Theoretical Model

  The fundamental bounds governing non-equilibrium processes are given by:

  $ Delta S_irr >= integral_0^t (delta Q)/T $

  #colbreak()

  == 3. Methodology

  We implemented an isolated test harness evaluating convergence rates across multiple benchmark suites:
  - Automated parameter sweeps
  - High-precision sampling
  - Lossless vector reproduction

  == 4. Results

  Empirical measurements confirm that the bound holds under arbitrary finite-time drive schedules.

  #colbreak()

  == 5. Discussion

  The framework demonstrates robust scaling and zero runtime overhead in local-first environments.

  == 6. References

  #bibliography("references.bib")
]
