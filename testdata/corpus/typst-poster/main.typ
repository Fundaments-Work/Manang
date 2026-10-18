#set page(paper: "a1", flipped: false, margin: (x: 2.5cm, y: 2.5cm))
#set text(size: 16pt)
#set par(justify: true, leading: 0.7em)

// Banner Header
#block(
  fill: rgb("#141416"),
  inset: (x: 2cm, y: 1.5cm),
  radius: 0pt,
  stroke: (bottom: 2pt + rgb("#c6f135")),
  width: 100%,
)[
  #text(fill: white, size: 36pt, weight: "bold")[Decentralized Entropy Production in High-Dimensional Spin Networks]

  #v(0.8em)
  #text(fill: rgb("#c6f135"), size: 20pt, weight: "medium")[A. M. Wright, E. R. Vance, and S. K. Thorne]

  #v(0.4em)
  #text(fill: rgb("#a0a0a5"), size: 16pt)[Institute for Quantum Information and Nonequilibrium Dynamics, Cambridge, UK]
]

#v(1.5cm)

#columns(3, gutter: 2cm)[
  #block(stroke: 1pt + rgb("#3d3d42"), inset: 1.2cm, width: 100%)[
    #text(size: 22pt, weight: "bold", fill: rgb("#141416"))[1. Motivation & Background]

    #v(0.5em)
    Classical consensus mechanisms treat communication channels as purely mathematical abstractions, ignoring the physical dissipation incurred by state modifications.

    According to Landauer's principle, the minimal thermodynamic work required to erase one bit of information in a thermal reservoir at temperature $T$ is:

    $ W_"min" = k_B T ln 2 $

    In large-scale distributed networks, collective synchronization requires continuous entropy exchange, creating fundamental energy scaling bounds.
  ]

  #v(1.5cm)

  #block(stroke: 1pt + rgb("#3d3d42"), inset: 1.2cm, width: 100%)[
    #text(size: 22pt, weight: "bold", fill: rgb("#141416"))[2. Theoretical Model]

    #v(0.5em)
    We consider a network graph $G = (V, E)$ with graph Laplacian $L = D - A$. The continuous-time state vector $bold(x)(t)$ evolves via overdamped Langevin dynamics:

    $ d bold(x)(t) = - L bold(x)(t) d t + sqrt(2 D) d bold(W)(t) $

    The total rate of entropy production $dot(S)_"prod"$ satisfies:

    $ dot(S)_"prod" = gamma / T chevron.l dot(bold(x))^T dot(bold(x)) chevron.r >= 0 $

    Spectral gap analysis confirms that the asymptotic relaxation rate depends directly on the algebraic connectivity $lambda_2(L)$.
  ]

  #colbreak()

  #block(stroke: 1pt + rgb("#3d3d42"), inset: 1.2cm, width: 100%)[
    #text(size: 22pt, weight: "bold", fill: rgb("#141416"))[3. Entropy-Aware Consensus]

    #v(0.5em)
    We propose the *Entropy-Aware Consensus (EAC)* protocol. Instead of uniform periodic broadcasting, agents transmit only when local disagreement exceeds an adaptive decaying threshold:

    $ theta_i(k) = theta_0 exp(- lambda k) $

    This adaptive cooling mechanism suppresses redundant transmissions during the initial high-variance phase.

    $ Delta V(k) <= - alpha (1 - alpha) theta_i^2(k) $

    Asymptotic stability is established via Lyapunov analysis.
  ]

  #v(1.5cm)

  #block(stroke: 1pt + rgb("#3d3d42"), inset: 1.2cm, width: 100%)[
    #text(size: 22pt, weight: "bold", fill: rgb("#141416"))[4. Numerical Benchmarks]

    #v(0.5em)
    #table(
      columns: (1.5fr, 1fr, 1.2fr),
      align: (left, center, right),
      table.header([*Protocol*], [*Rounds*], [*Dissipation*]),
      [Synchronous Jacobi], [2400], [185 kJ],
      [Static Threshold], [1650], [74 kJ],
      [EAC (Proposed)], [980], [21 kJ],
    )

    #v(0.8em)
    Empirical measurements confirm an 88% reduction in total dissipated energy compared to classical synchronous baselines.
  ]

  #colbreak()

  #block(stroke: 1pt + rgb("#3d3d42"), inset: 1.2cm, width: 100%)[
    #text(size: 22pt, weight: "bold", fill: rgb("#141416"))[5. Channel Noise Resilience]

    #v(0.5em)
    Under severe channel noise ($sigma_n^2 = 0.1$), standard gossip fails to achieve convergence, settling into an oscillatory noisy cycle.

    In contrast, EAC dynamically throttles transmissions when gradient differences fall below the noise floor, achieving high-fidelity consensus with a residual error variance of $0.0042$.
  ]

  #v(1.5cm)

  #block(stroke: 1pt + rgb("#3d3d42"), inset: 1.2cm, width: 100%)[
    #text(size: 22pt, weight: "bold", fill: rgb("#141416"))[6. Conclusions & References]

    #v(0.5em)
    - Thermodynamic awareness dramatically reduces distributed communication dissipation.
    - Adaptive gradient cooling functions as an informational low-pass filter.
    - Future work explores applications in satellite constellation formations.

    #v(0.5em)
    #text(size: 13pt, fill: luma(100))[
      1. Shannon, C. E. (1948). Bell Syst. Tech. J. \
      2. Landauer, R. (1961). IBM J. Res. Dev. \
      3. Bennett, C. H. (1982). Int. J. Theor. Phys.
    ]
  ]
]
