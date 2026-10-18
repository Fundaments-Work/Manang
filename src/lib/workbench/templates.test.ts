import { describe, expect, it } from "vitest";

import type { TemplateSummary } from "$lib/kernel/types";
import {
  groupTemplatesByEngine,
  matchesTemplateQuery,
  selectTemplates,
} from "./templates";

const SAMPLE_TEMPLATES: readonly TemplateSummary[] = [
  {
    id: "blank-typst",
    name: "Blank Typst",
    engine: "typst",
    description: "A minimal Typst project with clean defaults.",
    tags: ["blank", "minimal", "typst"],
    main: "main.typ",
  },
  {
    id: "blank-latex",
    name: "Blank LaTeX",
    engine: "latex",
    description: "A minimal LaTeX starter with standard article class.",
    tags: ["blank", "minimal", "latex"],
    main: "main.tex",
  },
  {
    id: "blank-markdown",
    name: "Blank Markdown",
    engine: "markdown",
    description: "A minimal Markdown starter with clean structure.",
    tags: ["blank", "minimal", "markdown"],
    main: "index.md",
  },
  {
    id: "article-typst",
    name: "Article (Typst)",
    engine: "typst",
    description: "Academic article with title, abstract, sections, and bibliography.",
    tags: ["article", "academic", "typst"],
    main: "main.typ",
  },
  {
    id: "article-latex",
    name: "Article (LaTeX)",
    engine: "latex",
    description:
      "Standard academic LaTeX article with title, abstract, sections, and BibTeX citations.",
    tags: ["article", "academic", "latex"],
    main: "main.tex",
  },
  {
    id: "thesis-chapter-typst",
    name: "Thesis Chapter (Typst)",
    engine: "typst",
    description:
      "Structured thesis chapter with outline, numbered subheadings, and bibliography.",
    tags: ["thesis", "chapter", "academic", "typst"],
    main: "main.typ",
  },
  {
    id: "letter-typst",
    name: "Letter (Typst)",
    engine: "typst",
    description:
      "Formal correspondence letter with sender header, recipient address, and signature.",
    tags: ["letter", "formal", "typst"],
    main: "main.typ",
  },
  {
    id: "slides-typst",
    name: "Slides (Typst)",
    engine: "typst",
    description:
      "Presentation slide deck with 16:9 aspect ratio and structured headings.",
    tags: ["slides", "presentation", "typst"],
    main: "main.typ",
  },
  {
    id: "poster-typst",
    name: "Poster (Typst)",
    engine: "typst",
    description:
      "Conference poster with title header, multi-column layout, and bibliography.",
    tags: ["poster", "conference", "academic", "typst"],
    main: "main.typ",
  },
  {
    id: "lab-report-latex",
    name: "Lab Report (LaTeX)",
    engine: "latex",
    description:
      "Scientific laboratory report with objective, procedure, data, analysis, and citations.",
    tags: ["lab-report", "science", "latex"],
    main: "main.tex",
  },
  {
    id: "notes-markdown",
    name: "Notes (Markdown)",
    engine: "markdown",
    description: "Structured research notes with an inline Mermaid workflow diagram.",
    tags: ["notes", "markdown", "diagram"],
    main: "index.md",
  },
];

describe("templates workbench logic", () => {
  it("matches by name, id, description, or tag", () => {
    const article = SAMPLE_TEMPLATES.find((t) => t.id === "article-typst")!;
    expect(matchesTemplateQuery(article, "Article")).toBe(true);
    expect(matchesTemplateQuery(article, "article-typst")).toBe(true);
    expect(matchesTemplateQuery(article, "academic")).toBe(true);
    expect(matchesTemplateQuery(article, "bibliography")).toBe(true);
    expect(matchesTemplateQuery(article, "nonexistent")).toBe(false);
  });

  it("selects all templates with empty query", () => {
    const res = selectTemplates(SAMPLE_TEMPLATES, { query: "" });
    expect(res).toHaveLength(SAMPLE_TEMPLATES.length);
  });

  it("filters templates by text query", () => {
    const res = selectTemplates(SAMPLE_TEMPLATES, { query: "slides" });
    expect(res).toHaveLength(1);
    expect(res[0]?.id).toBe("slides-typst");

    const latex = selectTemplates(SAMPLE_TEMPLATES, { query: "latex" });
    expect(latex).toHaveLength(3);
  });

  it("filters templates by engine", () => {
    const typstOnly = selectTemplates(SAMPLE_TEMPLATES, {
      query: "",
      engine: "typst",
    });
    expect(typstOnly.every((t) => t.engine === "typst")).toBe(true);
    expect(typstOnly).toHaveLength(6);

    const markdownOnly = selectTemplates(SAMPLE_TEMPLATES, {
      query: "",
      engine: "markdown",
    });
    expect(markdownOnly.every((t) => t.engine === "markdown")).toBe(true);
    expect(markdownOnly).toHaveLength(2);
  });

  it("combines text query and engine filtering", () => {
    const res = selectTemplates(SAMPLE_TEMPLATES, {
      query: "blank",
      engine: "latex",
    });
    expect(res).toHaveLength(1);
    expect(res[0]?.id).toBe("blank-latex");
  });

  it("groups templates by engine correctly", () => {
    const groups = groupTemplatesByEngine(SAMPLE_TEMPLATES);
    expect(groups.typst).toHaveLength(6);
    expect(groups.latex).toHaveLength(3);
    expect(groups.markdown).toHaveLength(2);
  });
});
