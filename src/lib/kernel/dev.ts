/**
 * The kernel used in a plain browser: the editor, project and typst plugins
 * reproduced in memory, with a small demo project so every panel has something
 * real to show. Nothing here is shipped to the desktop app, which talks to the
 * Rust kernel instead.
 */

import type { Kernel } from "./client";
import { createMockKernel } from "./mock";
import { MockProjectHost, projectPlugin, type SeedProject } from "./mockProject";
import { citationsPlugin } from "./mockCitations";
import { figurePlugin } from "./mockFigure";
import { latexPlugin } from "./mockLatex";
import { markdownPlugin } from "./mockMarkdown";
import { gitPlugin } from "./mockGit";
import { searchPlugin } from "./mockSearch";
import { settingsPlugin } from "./mockSettings";
import { typstPlugin } from "./mockTypst";
import { writingPlugin } from "./mockWriting";

export const DEMO_ROOT = "/home/author/entropy-paper";

const MAIN = `#set document(title: "Quantum thermodynamic bounds")

= Quantum thermodynamic bounds

Under unitary coupling to a heat reservoir at inverse temperature
$beta = (k_B T)^(-1)$, the irreversible entropy production rate
$Sigma$ satisfies the finite-time lower bound:

$ Delta S_irr >= integral_0^t (delta Q)/T $

This generalizes Landauer to finite-time operation, as shown in
@landauer1961 and confirmed experimentally in @berut2012.

#figure(image("figures/state_lattice.svg", width: 80%), caption: [Two-level state lattice.])

#include "sections/results.typ"

#bibliography("references.bib")
`;

const RESULTS_COMMITTED = `== Results

Monte Carlo trajectories of a driven qubit confirm the analytic bound across
ten thousand cycles.
`;

const RESULTS = `${RESULTS_COMMITTED}
The dissipated heat approaches $k_B T ln 2$ as the protocol slows, matching
the quasistatic limit of @landauer1961.
`;

const BACKGROUND = `== Background

Landauer's principle ties information erasure to heat. Finite-time protocols
add a penalty that scales with the speed of the drive.
`;

const TALK = `#set page(paper: "presentation-16-9")

= Landauer bound in quantum regimes

== Why erasure costs heat

Minimal heat dissipated per erased bit, extended to coherently controlled erasure.

$ Delta Q >= k_B T ln 2 $

== Irreversibility without heat

Under unitary coupling the entropy production rate is bounded below even when the bath is cold.

== Finite-time operation

The bound survives finite-time protocols, where quasistatic assumptions fail.

$ Delta S_irr >= integral_0^t (delta Q)/T $

== Numerical simulation

Monte Carlo trajectories of a driven qubit confirm the analytic bound.

== Open questions

Measurement-only erasure, non-Markovian baths, and the cost of quantum corrections.
`;

const BIB = `@article{landauer1961,
  author  = {Landauer, Rolf},
  title   = {Irreversibility and heat generation in the computing process},
  journal = {IBM Journal of Research and Development},
  year    = {1961},
  volume  = {5},
  pages   = {183--191}
}

@article{berut2012,
  author  = {B{\\'e}rut, Antoine and Arakelyan, Artak and Petrosyan, Artyom and Ciliberto, Sergio and Dillenschneider, Raoul and Lutz, Eric},
  title   = {Experimental verification of Landauer's principle linking information and thermodynamics},
  journal = {Nature},
  year    = {2012},
  volume  = {483}
}

@article{lloyd2008,
  author  = {Lloyd, Seth},
  title   = {Quantum thermodynamics: excuse our ignorance},
  journal = {Physical Review A},
  year    = {2008}
}

@book{nielsen2010,
  author    = {Nielsen, Michael A. and Chuang, Isaac L.},
  title     = {Quantum Computation and Quantum Information},
  publisher = {Cambridge University Press},
  year      = {2010}
}
`;

const NOTES = `# Notes

- Check the dissipation curve against the 2012 data.
- Ask about the cold-bath regime.
`;

const SCENE = {
  version: 1,
  width: 960,
  height: 540,
  nodes: [
    {
      kind: "math",
      id: "n1",
      x: 120,
      y: 120,
      tex: "|0⟩  ⟶  |1⟩",
      size: 28,
      color: "#b3d17a",
    },
    {
      kind: "rect",
      id: "n2",
      x: 120,
      y: 200,
      width: 190,
      height: 84,
      stroke: "#e8e6e1",
      fill: "#1c1c20",
    },
    {
      kind: "arrow",
      id: "n3",
      from: { x: 310, y: 242 },
      to: { x: 640, y: 242 },
      color: "#c6f135",
      label: "H",
    },
    {
      kind: "rect",
      id: "n4",
      x: 640,
      y: 200,
      width: 190,
      height: 84,
      stroke: "#e8e6e1",
      fill: "#1c1c20",
    },
  ],
};

const CURVE = {
  version: 1,
  width: 960,
  height: 540,
  nodes: [
    {
      kind: "arrow",
      id: "n1",
      from: { x: 120, y: 420 },
      to: { x: 840, y: 420 },
      color: "#e8e6e1",
      label: "t",
    },
    {
      kind: "arrow",
      id: "n2",
      from: { x: 120, y: 420 },
      to: { x: 120, y: 100 },
      color: "#e8e6e1",
    },
  ],
};

const FIGURE_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 960 540"><rect x="120" y="200" width="190" height="84" fill="none" stroke="#1c1c1c"/><rect x="640" y="200" width="190" height="84" fill="none" stroke="#1c1c1c"/><line x1="310" y1="242" x2="640" y2="242" stroke="#1c1c1c"/></svg>`;

const LECTURE_INDEX = `# Lecture notes

Week one covers the basics of statistical mechanics.

See [chapter one](chapter1.md).
`;

const LECTURE_CH1 = `## Chapter one

Entropy counts the microstates compatible with what we can measure.
`;

const THESIS = `\\documentclass{article}
\\begin{document}
\\section{Chapter three}
Open questions on finite-time erasure.
\\end{document}
`;

const POSTER = `#set page(paper: "a4")

= Landauer bound poster

Finite-time operation costs extra heat.
`;

/** A tiny paper-white page, standing in for a stored first-page thumbnail. */
const PAGE_THUMBNAIL = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 210 297"><rect width="210" height="297" fill="#faf9f7"/><rect x="40" y="48" width="130" height="8" fill="#1c1c1c"/><rect x="40" y="80" width="130" height="3" fill="#7a7a7a"/><rect x="40" y="90" width="130" height="3" fill="#7a7a7a"/><rect x="40" y="100" width="100" height="3" fill="#7a7a7a"/></svg>`;

const THESIS_MAIN = `\\documentclass{article}
\\begin{document}

\\section{Introduction}
Heat and information are linked by Landauer's principle, which bounds the
energy cost of erasing one bit of memory from below.

\\input{chapters/methods}

\\end{document}
`;

const THESIS_METHODS = `\\section{Methods}
We simulate a driven two-level system coupled to a thermal reservoir and
average the dissipated heat over ten thousand cycles.
`;

export const DEMO_PROJECTS: readonly SeedProject[] = [
  {
    root: DEMO_ROOT,
    name: "Entropy paper",
    engine: "typst",
    main: "main.typ",
    files: {
      "main.typ": MAIN,
      "talk.typ": TALK,
      "sections/background.typ": BACKGROUND,
      "sections/results.typ": RESULTS,
      "references.bib": BIB,
      "notes.md": NOTES,
      "figures/state_lattice.figure.json": JSON.stringify(SCENE, null, 2),
      "figures/state_lattice.svg": FIGURE_SVG,
      "figures/dissipation_curve.figure.json": JSON.stringify(CURVE, null, 2),
    },
    committed: [
      "main.typ",
      "talk.typ",
      "sections/background.typ",
      "references.bib",
      "figures/state_lattice.figure.json",
      "figures/state_lattice.svg",
      "figures/dissipation_curve.figure.json",
    ],
    // results.typ was committed before its last paragraph was added.
    committedEarlier: { "sections/results.typ": RESULTS_COMMITTED },
    openedAt: 1_760_600_000,
    created: 1_758_000_000,
    pinned: true,
    tags: ["physics", "draft"],
  },
  {
    root: "/home/author/lecture-notes",
    name: "Lecture notes",
    engine: "markdown",
    main: "index.md",
    files: { "index.md": LECTURE_INDEX, "chapter1.md": LECTURE_CH1 },
    committed: ["index.md", "chapter1.md"],
    openedAt: 1_760_000_000,
    created: 1_740_000_000,
    tags: ["teaching"],
  },
  {
    root: "/home/author/thesis-chapter-3",
    name: "Thesis chapter 3",
    engine: "latex",
    main: "main.tex",
    files: { "main.tex": THESIS },
    committed: ["main.tex"],
    openedAt: 1_759_000_000,
    created: 1_750_000_000,
    tags: ["thesis"],
    thumbnail: PAGE_THUMBNAIL,
  },
  {
    root: "/home/author/conference-poster",
    name: "Conference poster",
    engine: "typst",
    main: "main.typ",
    files: { "main.typ": POSTER },
    committed: ["main.typ"],
    openedAt: 1_755_000_000,
    created: 1_754_000_000,
    archived: true,
  },
  {
    root: "/home/author/thesis-draft",
    name: "Thesis draft",
    engine: "latex",
    main: "main.tex",
    files: { "main.tex": THESIS_MAIN, "chapters/methods.tex": THESIS_METHODS },
    committed: ["main.tex", "chapters/methods.tex"],
    openedAt: 1_750_000_001,
  },
];

/** Listed as recent but gone from disk, to show the missing-folder state. */
const MISSING = [
  {
    path: "/home/author/old-thesis",
    name: "Old thesis",
    openedAt: 1_750_000_000,
    engine: "latex",
  },
] as const;

export function createDevKernel(
  seeds: readonly SeedProject[] = DEMO_PROJECTS,
): Kernel & { host: MockProjectHost } {
  const host = new MockProjectHost(seeds, MISSING);
  const kernel = createMockKernel({
    plugins: [
      projectPlugin(host),
      typstPlugin(host),
      latexPlugin(host),
      markdownPlugin(),
      citationsPlugin(host),
      figurePlugin(),
      gitPlugin(host),
      searchPlugin(host),
      settingsPlugin(host),
      writingPlugin(),
    ],
  });
  return Object.assign(kernel, { host });
}
