import { describe, expect, it } from "vitest";

import {
  getDocumentOutline,
  parseLatexOutline,
  parseMarkdownOutlineTS,
  parseTypstOutline,
} from "./outline";

describe("outline: typst parser", () => {
  it("extracts hierarchical headings and inline/next-line labels", () => {
    const doc = `
= Quantum thermodynamic bounds

Under unitary coupling...

== Results <sec-results>
Monte Carlo trajectories confirm the bound.

=== Numerical simulations
<sec-sims>
Details of simulations.

// = Commented heading
/*
== Block commented
*/

\`\`\`typ
= Not a heading
\`\`\`

= Discussion
Final remarks.
`;

    const outline = parseTypstOutline(doc);
    expect(outline).toHaveLength(4);

    expect(outline[0]).toMatchObject({
      title: "Quantum thermodynamic bounds",
      level: 1,
      line: 2,
    });

    expect(outline[1]).toMatchObject({
      title: "Results",
      level: 2,
      line: 6,
      label: "sec-results",
      id: "sec-results",
    });

    expect(outline[2]).toMatchObject({
      title: "Numerical simulations",
      level: 3,
      line: 9,
      label: "sec-sims",
      id: "sec-sims",
    });

    expect(outline[3]).toMatchObject({
      title: "Discussion",
      level: 1,
      line: 22,
    });
  });
});

describe("outline: latex parser", () => {
  it("extracts sections, subsections, and labels", () => {
    const tex = `
\\documentclass{article}
\\begin{document}

\\section{Introduction}\\label{sec:intro}
Here is the intro.

% \\section{Hidden}

\\begin{verbatim}
\\section{Code block}
\\end{verbatim}

\\subsection*{Experimental \\textbf{Setup}}
\\label{sec:setup}
Setup details.

\\subsubsection{Apparatus}
Microscope.

\\end{document}
`;

    const outline = parseLatexOutline(tex);
    expect(outline).toHaveLength(3);

    expect(outline[0]).toMatchObject({
      title: "Introduction",
      level: 2,
      line: 5,
      label: "sec:intro",
    });

    expect(outline[1]).toMatchObject({
      title: "Experimental Setup",
      level: 3,
      line: 14,
      label: "sec:setup",
    });

    expect(outline[2]).toMatchObject({
      title: "Apparatus",
      level: 4,
      line: 18,
    });
  });
});

describe("outline: markdown parser", () => {
  it("extracts ATX headings and attribute IDs", () => {
    const md = `
# Title

## Background {#sec-bg}
Some background text.

\`\`\`markdown
# Ignored in code block
\`\`\`

### Methodology
Steps.
`;

    const outline = parseMarkdownOutlineTS(md);
    expect(outline).toHaveLength(3);

    expect(outline[0]).toMatchObject({
      title: "Title",
      level: 1,
      line: 2,
    });

    expect(outline[1]).toMatchObject({
      title: "Background",
      level: 2,
      line: 4,
      id: "sec-bg",
    });

    expect(outline[2]).toMatchObject({
      title: "Methodology",
      level: 3,
      line: 11,
    });
  });
});

describe("outline: getDocumentOutline router", () => {
  it("dispatches correctly by engine and returns empty for plain", async () => {
    const typst = "= A";
    const typstOut = await getDocumentOutline(typst, "typst");
    expect(typstOut[0]?.title).toBe("A");

    const latex = "\\section{B}";
    const latexOut = await getDocumentOutline(latex, "latex");
    expect(latexOut[0]?.title).toBe("B");

    const md = "# C";
    const mdOut = await getDocumentOutline(md, "markdown");
    expect(mdOut[0]?.title).toBe("C");

    const plain = "# D";
    const plainOut = await getDocumentOutline(plain, "plain");
    expect(plainOut).toEqual([]);
  });
});
