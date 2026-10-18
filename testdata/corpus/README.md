# Realistic Test Corpus

A collection of 12 small but realistic test projects used to validate compilation, preview generation, diagnostics, and export across Manang's supported authoring engines.

| Project | Engine | Description |
| :--- | :--- | :--- |
| `typst-article` | Typst | Academic journal article with title block, abstract, equations, table, embedded SVG figure, BibTeX bibliography (8+ entries), and cross-references. |
| `typst-thesis` | Typst | 40-page doctoral dissertation partitioned across five modular chapter includes (`01_introduction.typ` through `05_conclusion.typ`). |
| `typst-slides` | Typst | 6-slide presentation deck using `presentation-16-9` aspect ratio, large typography, tables, and equations. |
| `typst-poster` | Typst | Single-page A1 conference poster with multi-column layout, banner header, theoretical blocks, tables, and references. |
| `typst-letter` | Typst | Formal academic letter with sender header, recipient address, date, subject line, body paragraphs, and sign-off block. |
| `typst-heavy-math` | Typst | Intensive mathematical physics document exercising multiline aligned equations, Christoffel symbols, path integrals, Grassmann variables, and contour integrals across 3 pages. |
| `typst-cjk-accented` | Typst | Multilingual document testing Unicode handling for accented European Latin scripts and East Asian ideographs (Chinese, Japanese, Korean) using embedded fonts. |
| `typst-broken` | Typst | Deliberately malformed document exercising diagnostic error generation (missing brace, unknown function call, missing include) without compiler panics. |
| `latex-article` | LaTeX | Standard LaTeX article with BibTeX bibliography integration, abstract, mathematical environments, and citations. |
| `latex-report` | LaTeX | Multi-file LaTeX technical report using `\input` chapters, table of contents, and report structure without Beamer. |
| `latex-broken` | LaTeX | Deliberately broken LaTeX file with unclosed environments and undefined macros for compiler diagnostic validation. |
| `markdown-notes` | Markdown | Technical notes file featuring YAML frontmatter, inline and display math, summary tables, and a fenced Mermaid flowchart diagram. |
