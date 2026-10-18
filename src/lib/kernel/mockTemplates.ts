/**
 * Built-in starter templates for the mock kernel.
 *
 * Keeps browser dev mode (`pnpm dev`) and headless UI tests in parity with the
 * Rust plugin without needing OS filesystem access.
 */

import type { Engine, TemplateSummary } from "./types";

export interface MockTemplate extends TemplateSummary {
  readonly files: Readonly<Record<string, string>>;
}

const BIB = `@article{knuth1984,
  author  = {Donald E. Knuth},
  title   = {Literate Programming},
  journal = {The Computer Journal},
  volume  = {27},
  number  = {2},
  pages   = {97--111},
  year    = {1984},
}
`;

export const BUILTIN_TEMPLATES: readonly MockTemplate[] = [
  {
    id: "blank-typst",
    name: "Blank Typst",
    engine: "typst",
    description: "A minimal Typst project with clean defaults.",
    tags: ["blank", "minimal", "typst"],
    main: "main.typ",
    files: {
      "main.typ": `#set document(title: "{{title_string}}")
#set page(paper: "a4")
#set text(size: 11pt)

#align(center)[#text(size: 18pt)[{{title}}]]

Start writing here.
`,
    },
  },
  {
    id: "blank-latex",
    name: "Blank LaTeX",
    engine: "latex",
    description: "A minimal LaTeX starter with standard article class.",
    tags: ["blank", "minimal", "latex"],
    main: "main.tex",
    files: {
      "main.tex": `\\documentclass[11pt,a4paper]{article}
\\usepackage[utf8]{inputenc}
\\usepackage[T1]{fontenc}

\\title{{{title}}}
\\author{}
\\date{\\today}

\\begin{document}
\\maketitle

Start writing here.

\\end{document}
`,
    },
  },
  {
    id: "blank-markdown",
    name: "Blank Markdown",
    engine: "markdown",
    description: "A minimal Markdown starter with clean structure.",
    tags: ["blank", "minimal", "markdown"],
    main: "index.md",
    files: {
      "index.md": `# {{title}}

Start writing here.
`,
    },
  },
  {
    id: "article-typst",
    name: "Article (Typst)",
    engine: "typst",
    description: "Academic article with title, abstract, sections, and bibliography.",
    tags: ["article", "academic", "typst"],
    main: "main.typ",
    files: {
      "main.typ": `#set document(title: "{{title_string}}")
#set page(paper: "a4", margin: (x: 2.5cm, y: 3cm))
#set text(size: 11pt)
#set heading(numbering: "1.1")

#align(center)[
  #text(size: 20pt, weight: "bold")[{{title}}]

  #v(0.5em)
  #text(size: 11pt, fill: rgb("#666666"))[Author Name]

  #text(size: 10pt, fill: rgb("#888888"))[Department of Computer Science \\ University Research Lab]
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
`,
      "references.bib": BIB,
    },
  },
  {
    id: "article-latex",
    name: "Article (LaTeX)",
    engine: "latex",
    description:
      "Standard academic LaTeX article with title, abstract, sections, and BibTeX citations.",
    tags: ["article", "academic", "latex"],
    main: "main.tex",
    files: {
      "main.tex": `\\documentclass[11pt,a4paper]{article}
\\usepackage[utf8]{inputenc}
\\usepackage[T1]{fontenc}
\\usepackage{amsmath,amssymb}

\\title{{{title}}}
\\author{Author Name\\\\Department of Computer Science\\\\\\texttt{author@example.edu}}
\\date{\\today}

\\begin{document}
\\maketitle

\\begin{abstract}
This document provides a standard academic article template in \\LaTeX, including abstract, sections, equations, and BibTeX references.
\\end{abstract}

\\section{Introduction}

Scientific documentation requires rigor, consistency, and precise typography. As demonstrated by Knuth~\\cite{knuth1984}, literate programming and structured formatting improve clarity.

\\section{Methodology}

The theoretical framework is governed by the relation:
\\begin{equation}
\\Delta S \\ge \\int_0^t \\frac{\\delta Q}{T}
\\end{equation}

\\section{Results}

Our observations align with the predicted theoretical bounds.

\\section{Conclusion}

This template serves as a starting point for journal submissions and conference papers.

\\bibliographystyle{plain}
\\bibliography{references}

\\end{document}
`,
      "references.bib": BIB,
    },
  },
  {
    id: "thesis-chapter-typst",
    name: "Thesis Chapter (Typst)",
    engine: "typst",
    description:
      "Structured thesis chapter with outline, numbered subheadings, and bibliography.",
    tags: ["thesis", "chapter", "academic", "typst"],
    main: "main.typ",
    files: {
      "main.typ": `#set document(title: "{{title_string}}")
#set page(paper: "a4", numbering: "1", margin: (x: 3cm, top: 3cm, bottom: 2.5cm))
#set text(size: 11pt)
#set heading(numbering: "1.1")

#text(size: 14pt, fill: rgb("#666666"))[Chapter 1]
#v(0.2em)
#text(size: 24pt, weight: "bold")[{{title}}]
#v(1em)

= Chapter Overview

This chapter investigates the foundational principles and theoretical background underpinning this dissertation. We examine existing literature, focusing on the seminal contributions of @knuth1984.

= Background and Foundations

== Historical Context

The evolution of structured documentation reflects ongoing efforts to bridge formal specifications with human readability.

== Formal Formulation

Consider an open system defined over state space $Omega$:

$ d/d t rho(t) = -i [H, rho(t)] + cal(D)(rho(t)) $

= Experimental Design

The methodology leverages deterministic evaluation pipelines to ensure reproducibility across experimental trials.

= Summary

The formulations presented in this chapter establish the baseline metrics utilized throughout the remainder of this thesis.

#bibliography("references.bib")
`,
      "references.bib": BIB,
    },
  },
  {
    id: "letter-typst",
    name: "Letter (Typst)",
    engine: "typst",
    description:
      "Formal correspondence letter with sender header, recipient address, and signature.",
    tags: ["letter", "formal", "typst"],
    main: "main.typ",
    files: {
      "main.typ": `#set document(title: "{{title_string}}")
#set page(paper: "a4", margin: (x: 2.5cm, top: 3cm, bottom: 2.5cm))
#set text(size: 11pt)

#grid(
  columns: (1fr, 1fr),
  align(left)[
    *Author Name* \\
    Department of Science \\
    University of Excellence \\
    City, Country
  ],
  align(right)[
    #datetime.today().display("[month repr:long] [day], [year]")
  ]
)

#v(2em)

*Recipient Name* \\
Hiring Committee / Editorial Office \\
Institution Name \\
Address Line \\
City, Postcode

#v(1.5em)

*Subject: {{title}}*

#v(1em)

Dear Committee Members,

I am writing to formally submit our application regarding the position referenced above. With extensive background in scientific research and systems engineering, I welcome the opportunity to contribute to your ongoing initiatives.

During recent projects, our laboratory developed automated workflows that substantially reduced iteration times while maintaining rigorous verification standards. Our team places primary emphasis on clarity, reproducible methodology, and principled design.

Thank you for your time and consideration. I look forward to hearing from you.

#v(2em)

Sincerely,

#v(1.5em)

*Author Name* \\
Senior Researcher
`,
    },
  },
  {
    id: "slides-typst",
    name: "Slides (Typst)",
    engine: "typst",
    description:
      "Presentation slide deck with 16:9 aspect ratio and structured headings.",
    tags: ["slides", "presentation", "typst"],
    main: "main.typ",
    files: {
      "main.typ": `#set document(title: "{{title_string}}")
#set page(paper: "presentation-16-9", margin: (x: 2cm, y: 1.5cm))
#set text(size: 20pt)

= {{title}}

#v(1em)
#text(size: 16pt)[Author Name] \\
#text(size: 14pt, fill: rgb("#666666"))[Research Group / Conference 2026]

== Motivation and Objectives

- High-performance authoring for modern scientific documents
- Sub-millisecond responsiveness with instant vector preview
- Deterministic compilation and reproducible research

== Theoretical Framework

The entropy production rate satisfies the finite-time bound:

$ Delta S_"irr" >= integral_0^t (delta Q)/T $

- Quasistatic limits are preserved as drive speed vanishes
- Non-equilibrium dynamics govern fast protocols

== Empirical Evaluation

- Benchmarked across ten thousand experimental cycles
- Verified zero degradation in numerical fidelity
- Consistent performance across distributed runs

== Conclusions and Next Steps

- Clean architectural separation ensures stability
- Native slide presentations compile instantly offline
- Open questions: extension to non-Markovian environments
`,
    },
  },
  {
    id: "poster-typst",
    name: "Poster (Typst)",
    engine: "typst",
    description:
      "Conference poster with title header, multi-column layout, and bibliography.",
    tags: ["poster", "conference", "academic", "typst"],
    main: "main.typ",
    files: {
      "main.typ": `#set document(title: "{{title_string}}")
#set page(paper: "a1", flipped: false, margin: (x: 2cm, y: 2cm))
#set text(size: 18pt)

#align(center)[
  #block(
    fill: rgb("#f5f5f5"),
    inset: 2cm,
    width: 100%,
    [
      #text(size: 42pt, weight: "bold")[{{title}}] \\
      #v(0.5em)
      #text(size: 24pt)[Author Name#super[1], Co-Author Name#super[2]] \\
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

  $ Delta S_"irr" >= integral_0^t (delta Q)/T $

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
`,
      "references.bib": BIB,
    },
  },
  {
    id: "lab-report-latex",
    name: "Lab Report (LaTeX)",
    engine: "latex",
    description:
      "Scientific laboratory report with objective, procedure, data, analysis, and citations.",
    tags: ["lab-report", "science", "latex"],
    main: "main.tex",
    files: {
      "main.tex": `\\documentclass[11pt,a4paper]{article}
\\usepackage[utf8]{inputenc}
\\usepackage[T1]{fontenc}
\\usepackage{amsmath,amssymb}

\\title{{{title}}}
\\author{Lab Partner 1 \\and Lab Partner 2\\\\Department of Physics}
\\date{\\today}

\\begin{document}
\\maketitle

\\section{Objective}
The purpose of this laboratory experiment is to quantitatively measure thermal dissipation in driven systems and verify classical thermodynamic bounds as discussed in the literature~\\cite{knuth1984}.

\\section{Theoretical Background}
The governing equation relates entropy change to heat transfer:
\\begin{equation}
\\Delta S \\ge \\int_0^t \\frac{\\delta Q}{T}
\\end{equation}

\\section{Apparatus and Procedure}
\\begin{enumerate}
  \\item Calibrate thermal sensors against standard reference reservoirs.
  \\item Initialize the sample under controlled ambient pressure.
  \\item Record sensor trajectories at 1~kHz sampling rate.
\\end{enumerate}

\\section{Data and Results}
Measurements collected over multiple trials demonstrated high consistency with theoretical limits. Uncertainty bounds were evaluated at the $95\\%$ confidence interval.

\\section{Discussion and Error Analysis}
Systematic uncertainties were dominated by thermal sensor response times, while random noise remained well within nominal operating tolerance.

\\section{Conclusion}
The experimental results successfully validate the theoretical prediction within the established error margins.

\\bibliographystyle{plain}
\\bibliography{references}

\\end{document}
`,
      "references.bib": BIB,
    },
  },
  {
    id: "notes-markdown",
    name: "Notes (Markdown)",
    engine: "markdown",
    description: "Structured research notes with an inline Mermaid workflow diagram.",
    tags: ["notes", "markdown", "diagram"],
    main: "index.md",
    files: {
      "index.md": `# {{title}}

## Research Summary

Structured research notes documenting workflow, experimental architecture, and key findings.

## Process Flow

\`\`\`mermaid
graph TD
  A[Literature Review] --> B[Hypothesis Formulation]
  B --> C[Experimentation]
  C --> D{Analysis}
  D -->|Validated| E[Publication]
  D -->|Iterate| B
\`\`\`

## Action Items

- [x] Verify offline compilation of starters
- [x] Integrate Mermaid workflow fences
- [ ] Document benchmark parameters
`,
    },
  },
  {
    id: "welcome",
    name: "Welcome to Manang",
    engine: "typst",
    description:
      "Interactive onboarding project teaching Manang's editor, preview, figures, citations, and shortcuts.",
    tags: ["welcome", "onboarding", "tutorial", "typst"],
    main: "main.typ",
    files: {
      "main.typ": `#set document(title: "{{title_string}}")
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
+ *SyncTeX navigation:* In the preview panel on the right, double-click or \`Ctrl\`-click (\`Cmd\`-click on macOS) any text or heading to jump straight back to its exact source line in this editor. Press \`Ctrl+Alt+J\` (\`Cmd+Alt+J\`) to jump forward to preview.
+ *Command palette:* Press \`Ctrl+K\` (\`Cmd+K\` on macOS) to open the palette. Filter available actions by typing \`export\`, \`settings\`, or \`/\`.
+ *Settings & scope:* Press \`Ctrl+,\` (\`Cmd+,\` on macOS) to open Settings. Switch between global User preferences and project-specific configuration.
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
  [Command Palette], [\`Ctrl+K\` (\`Cmd+K\`)], [Fuzzy search all commands and tools],
  [Settings], [\`Ctrl+,\` (\`Cmd+,\`)], [View and modify user or project options],
  [Export PDF], [\`Ctrl+E\` (\`Cmd+E\`)], [Compile and save publication-ready PDF],
  [Toggle Preview], [\`Ctrl+Shift+V\` (\`Cmd+Shift+V\`)], [Show, hide, or split preview pane],
  [Shortcut Sheet], [\`Ctrl+/\` (\`Cmd+/\`)], [Browse the complete workbench shortcut list],
)

= References

Literature citations reference \`references.bib\` using standard citation keys @knuth1984.

#bibliography("references.bib")
`,
      "references.bib": `@article{knuth1984,
  author  = {Donald E. Knuth},
  title   = {Literate Programming},
  journal = {The Computer Journal},
  volume  = {27},
  number  = {2},
  pages   = {97--111},
  year    = {1984},
}

@article{turing1936,
  author  = {Alan M. Turing},
  title   = {On Computable Numbers, with an Application to the {Entscheidungsproblem}},
  journal = {Proceedings of the London Mathematical Society},
  volume  = {42},
  pages   = {230--265},
  year    = {1936},
}

@article{shannon1948,
  author  = {Claude E. Shannon},
  title   = {A Mathematical Theory of Communication},
  journal = {Bell System Technical Journal},
  volume  = {27},
  number  = {3},
  pages   = {379--423},
  year    = {1948},
}
`,
      "notes.md": `# Welcome Notes & Quick Tips

This markdown document accompanies your onboarding project and illustrates Manang's multi-format capabilities.

## What You Can Try in Manang

1. **Typst Editing:** Open \`main.typ\` to explore real-time mathematical typesetting and instant compilation.
2. **SyncTeX Bidirectional Navigation:** In the preview panel, double-click or \`Ctrl\`-click (\`Cmd\`-click) text to jump directly to its source line. Press \`Ctrl+Alt+J\` (\`Cmd+Alt+J\`) to return to preview.
3. **Command Palette:** Press \`Ctrl+K\` (\`Cmd+K\`) to fuzzy search commands, recent files, and export options.
4. **Diagrams:** Inspect \`figures/flow.mmd\` to see how Mermaid flowcharts compile to standalone SVGs.
5. **Settings Scoping:** Open Settings (\`Ctrl+,\`) to toggle between global User preferences and project-specific configuration.
6. **Version Snapshots:** Git-backed versioning and local snapshots ("Save a version") are coming soon in a later beta release.
`,
      "sections/tour.typ": `= Key Capabilities

Manang combines sub-millisecond responsive editing with deterministic compilation across scientific document formats.

== Multi-Engine Core
Whether authoring in Typst for instant feedback, LaTeX for legacy journal submissions, or Markdown for rapid technical notes, your documents remain human-readable UTF-8 files on disk.

== Vector Figures & Text Diagrams
Embed hand-drawn or precise vector figures (\`.svg\` and \`.figure.json\`) and text diagrams (such as Mermaid flowcharts in \`figures/flow.mmd\`) directly into your documents without rasterization artifacts.

== Citation Integrity
Cite literature using standard BibTeX keys (@knuth1984, @turing1936). The References panel indexes your bibliography files in memory, provides instant search, and formats references in APA and IEEE styles.
`,
      "figures/welcome.svg": `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 540 180" width="540" height="180">
  <defs>
    <style>
      .bg { fill: #141416; }
      .box { fill: #1c1c1f; stroke: #3d3d42; stroke-width: 1; }
      .box-active { fill: #1c1c1f; stroke: #c6f135; stroke-width: 1.5; }
      .text-title { font-family: 'DM Sans', -apple-system, sans-serif; font-size: 13px; font-weight: 500; fill: #faf9f7; }
      .text-sub { font-family: 'DM Mono', monospace; font-size: 10px; fill: #9e9ea0; }
      .text-accent { font-family: 'DM Mono', monospace; font-size: 10px; fill: #c6f135; }
      .arrow { stroke: #b3d17a; stroke-width: 1.5; fill: none; stroke-linecap: round; }
      .arrow-head { fill: #b3d17a; }
    </style>
  </defs>

  <!-- Canvas Background -->
  <rect width="540" height="180" class="bg" />

  <!-- Node 1: Text Editor -->
  <g transform="translate(20, 30)">
    <rect width="140" height="120" class="box" />
    <text x="16" y="32" class="text-title">Text Editor</text>
    <text x="16" y="54" class="text-sub">CodeMirror 6</text>
    <text x="16" y="74" class="text-sub">Rope Buffer</text>
    <text x="16" y="94" class="text-sub">Tree-sitter AST</text>
    <rect x="16" y="108" width="6" height="6" fill="#c6f135" />
    <text x="28" y="114" class="text-accent">&lt; 8ms latency</text>
  </g>

  <!-- Arrow 1 -> 2 -->
  <path d="M 160 90 L 190 90" class="arrow" />
  <polygon points="190,87 198,90 190,93" class="arrow-head" />

  <!-- Node 2: Kernel Engine -->
  <g transform="translate(200, 30)">
    <rect width="140" height="120" class="box-active" />
    <text x="16" y="32" class="text-title">Kernel Engine</text>
    <text x="16" y="54" class="text-sub">Typst Compiler</text>
    <text x="16" y="74" class="text-sub">LaTeX Tectonic</text>
    <text x="16" y="94" class="text-sub">Rayon Pool</text>
    <rect x="16" y="108" width="6" height="6" fill="#b3d17a" />
    <text x="28" y="114" class="text-sub" fill="#b3d17a">zero-copy IPC</text>
  </g>

  <!-- Arrow 2 -> 3 -->
  <path d="M 340 90 L 370 90" class="arrow" />
  <polygon points="370,87 378,90 370,93" class="arrow-head" />

  <!-- Node 3: Live Preview -->
  <g transform="translate(380, 30)">
    <rect width="140" height="120" class="box" />
    <text x="16" y="32" class="text-title">Live Preview</text>
    <text x="16" y="54" class="text-sub">Streaming SVG</text>
    <text x="16" y="74" class="text-sub">PDF Text Layer</text>
    <text x="16" y="94" class="text-sub">SyncTeX Jumps</text>
    <rect x="16" y="108" width="6" height="6" fill="#c6f135" />
    <text x="28" y="114" class="text-accent">60 / 120 FPS</text>
  </g>
</svg>
`,
      "figures/flow.mmd": `flowchart LR
    A["Write (Typst / LaTeX / MD)"] --> B["Instant Live Preview"]
    B --> C["Export PDF / SVG"]
`,
    },
  },
];

export function listMockTemplates(): TemplateSummary[] {
  return BUILTIN_TEMPLATES.map((t) => ({
    id: t.id,
    name: t.name,
    engine: t.engine,
    description: t.description,
    tags: [...t.tags],
    main: t.main,
  }));
}

export function getMockTemplate(id: string): MockTemplate | undefined {
  return BUILTIN_TEMPLATES.find((t) => t.id === id);
}

function escapeTypst(title: string): string {
  return title.replace(/([\\#$*_`@<>[\]~/=])/g, "\\$1");
}

function escapeTypstString(title: string): string {
  return title.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

function escapeLatex(title: string): string {
  return title.replace(/([&%$#_{}])/g, "\\$1");
}

function renderContent(engine: Engine, text: string, title: string): string {
  switch (engine) {
    case "typst":
      return text
        .replaceAll("{{title_string}}", escapeTypstString(title))
        .replaceAll("{{title}}", escapeTypst(title));
    case "latex":
      return text.replaceAll("{{title}}", escapeLatex(title));
    case "markdown":
      return text.replaceAll("{{title}}", title);
  }
}

export function renderTemplateFiles(
  id: string,
  projectName: string,
): Record<string, string> | undefined {
  const tmpl = getMockTemplate(id);
  if (!tmpl) return undefined;
  const out: Record<string, string> = {};
  for (const [name, content] of Object.entries(tmpl.files)) {
    out[name] = renderContent(tmpl.engine, content, projectName);
  }
  return out;
}
