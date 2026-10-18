/**
 * The kernel used in a plain browser: the editor, project and typst plugins
 * reproduced in memory, with a small demo project so every panel has something
 * real to show. Nothing here is shipped to the desktop app, which talks to the
 * Rust kernel instead.
 */

import type { Kernel } from "./client";
import { createMockKernel } from "./mock";
import {
  MockProjectHost,
  devHooksPlugin,
  projectPlugin,
  type SeedProject,
} from "./mockProject";
import { recoveryPlugin } from "./mockRecovery";
import { citationsPlugin } from "./mockCitations";
import { diagramPlugin } from "./mockDiagram";
import { figurePlugin } from "./mockFigure";
import { latexPlugin } from "./mockLatex";
import { markdownPlugin } from "./mockMarkdown";
import { gitPlugin } from "./mockGit";
import { pluginsPlugin } from "./mockPlugins";
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

const FLOW = `flowchart TD
  %% How a measurement becomes a bound.
  A[Prepare qubit] --> B[Drive protocol]
  B --> C[Measure heat]
  C --> D[Compare with bound]
`;

/**
 * A 240 by 150 PNG (a hairline square and a ring on a dark ground), so the
 * image viewer has a real raster file to open in `pnpm dev`.
 */
const SKETCH_PNG = new Uint8Array([
  137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 240, 0, 0, 0,
  150, 8, 2, 0, 0, 0, 111, 154, 146, 130, 0, 0, 2, 109, 73, 68, 65, 84, 120, 218, 237,
  221, 177, 113, 194, 80, 16, 69, 81, 10, 33, 112, 19, 52, 71, 91, 238, 193, 57, 165,
  208, 1, 25, 67, 104, 70, 210, 215, 238, 219, 51, 115, 83, 219, 255, 49, 39, 240, 32,
  144, 46, 215, 235, 143, 20, 211, 197, 75, 32, 160, 37, 160, 37, 160, 37, 160, 5, 180,
  4, 180, 4, 180, 4, 180, 4, 180, 128, 150, 128, 150, 128, 150, 128, 150, 128, 22, 208,
  18, 208, 18, 208, 18, 208, 18, 208, 2, 90, 2, 90, 2, 90, 2, 90, 2, 90, 64, 75, 64, 75,
  64, 75, 64, 75, 64, 11, 104, 9, 104, 9, 104, 9, 104, 9, 104, 1, 45, 1, 45, 1, 45, 1,
  45, 160, 15, 234, 239, 121, 11, 11, 151, 233, 160, 147, 94, 41, 160, 129, 6, 90, 64,
  3, 45, 160, 129, 22, 208, 179, 230, 252, 62, 238, 255, 15, 104, 160, 19, 28, 7, 200,
  6, 58, 115, 206, 70, 154, 125, 101, 3, 157, 54, 103, 95, 136, 237, 88, 3, 29, 53, 231,
  32, 124, 141, 76, 3, 29, 50, 103, 129, 185, 22, 172, 129, 78, 152, 179, 210, 89, 113,
  211, 64, 183, 159, 179, 94, 88, 101, 211, 64, 247, 158, 115, 150, 173, 178, 166, 129,
  110, 60, 231, 92, 85, 53, 77, 3, 221, 117, 78, 5, 79, 5, 77, 3, 221, 114, 78, 29, 73,
  213, 76, 3, 221, 111, 78, 53, 67, 165, 206, 3, 116, 87, 208, 5, 255, 255, 1, 26, 232,
  4, 205, 165, 14, 6, 116, 167, 57, 149, 223, 0, 46, 114, 54, 160, 251, 129, 46, 126,
  193, 18, 104, 160, 19, 174, 57, 23, 57, 33, 208, 205, 64, 183, 248, 184, 31, 208, 64,
  3, 13, 244, 36, 208, 141, 190, 54, 114, 238, 81, 129, 6, 26, 104, 160, 129, 6, 26,
  232, 9, 154, 79, 63, 48, 208, 64, 3, 13, 52, 208, 64, 3, 13, 52, 208, 64, 3, 13, 52,
  208, 64, 3, 13, 52, 208, 64, 3, 13, 52, 208, 64, 3, 13, 52, 208, 64, 3, 13, 52, 208,
  64, 3, 13, 52, 208, 64, 3, 13, 52, 208, 64, 3, 13, 180, 207, 67, 251, 60, 52, 208, 64,
  3, 13, 52, 208, 64, 3, 29, 102, 218, 183, 190, 129, 6, 26, 232, 193, 160, 221, 57, 9,
  232, 156, 57, 238, 109, 7, 116, 32, 104, 119, 31, 5, 58, 103, 142, 251, 67, 3, 157,
  54, 199, 29, 252, 129, 14, 4, 237, 25, 43, 64, 231, 204, 241, 20, 44, 160, 211, 230,
  120, 78, 33, 208, 105, 115, 60, 73, 22, 232, 180, 57, 158, 245, 13, 116, 218, 156,
  245, 182, 42, 191, 29, 14, 116, 194, 156, 149, 194, 138, 95, 129, 7, 58, 100, 206,
  219, 217, 113, 212, 22, 252, 9, 160, 129, 94, 97, 174, 5, 101, 160, 51, 231, 124, 226,
  219, 238, 111, 223, 223, 6, 52, 208, 251, 64, 252, 214, 226, 198, 31, 7, 26, 232, 117,
  178, 191, 170, 221, 94, 160, 167, 204, 201, 118, 12, 244, 244, 57, 169, 1, 13, 52,
  208, 64, 107, 38, 232, 176, 112, 25, 13, 90, 2, 90, 2, 90, 2, 90, 64, 75, 64, 75, 64,
  75, 64, 75, 64, 11, 104, 9, 104, 9, 104, 9, 104, 1, 45, 1, 45, 1, 45, 1, 45, 1, 45,
  160, 37, 160, 37, 160, 37, 160, 37, 160, 5, 180, 4, 180, 4, 180, 4, 180, 4, 180, 128,
  150, 128, 150, 128, 150, 128, 150, 128, 22, 208, 18, 208, 18, 208, 18, 208, 18, 208,
  2, 90, 2, 90, 90, 220, 11, 31, 183, 128, 5, 70, 70, 36, 196, 0, 0, 0, 0, 73, 69, 78,
  68, 174, 66, 96, 130,
]);

/** A one-page PDF (plain ASCII, so it can live in source), for the PDF tab. */
const NOTE_PDF = new TextEncoder().encode(
  `%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>
endobj
4 0 obj
<< /Length 110 >>
stream
BT /F1 18 Tf 72 720 Td (Landauer bound: kT ln 2 per erased bit.) Tj ET
0.78 0.95 0.21 RG 72 700 m 540 700 l S
endstream
endobj
5 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj
xref
0 6
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000115 00000 n 
0000000241 00000 n 
0000000401 00000 n 
trailer
<< /Size 6 /Root 1 0 R >>
startxref
471
%%EOF
`,
);

/** An empty ZIP archive: a file Manang cannot show, for the info tab. */
const RAW_ZIP = new Uint8Array([
  0x50,
  0x4b,
  0x05,
  0x06,
  ...new Array<number>(18).fill(0),
]);

const MEASUREMENTS_CSV = `cycle,duration_ms,heat_kT
1,10,0.93
2,20,0.81
3,40,0.75
4,80,0.71
`;

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
      "figures/flow.mmd": FLOW,
      "figures/sketch.png": SKETCH_PNG,
      "data/measurements.csv": MEASUREMENTS_CSV,
      "data/raw.zip": RAW_ZIP,
      "papers/landauer-note.pdf": NOTE_PDF,
    },
    committed: [
      "main.typ",
      "talk.typ",
      "sections/background.typ",
      "references.bib",
      "figures/state_lattice.figure.json",
      "figures/state_lattice.svg",
      "figures/dissipation_curve.figure.json",
      "figures/sketch.png",
      "data/measurements.csv",
      "data/raw.zip",
      "papers/landauer-note.pdf",
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
      devHooksPlugin(host),
      recoveryPlugin(host),
      typstPlugin(host),
      latexPlugin(host),
      markdownPlugin(),
      citationsPlugin(host),
      diagramPlugin(),
      figurePlugin(host),
      gitPlugin(host),
      searchPlugin(host),
      pluginsPlugin(),
      settingsPlugin(host),
      writingPlugin(),
    ],
  });
  return Object.assign(kernel, { host });
}
