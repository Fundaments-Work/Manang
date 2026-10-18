import { describe, expect, it } from "vitest";

import { diagramApi, markdownApi } from "$lib/kernel/api";
import { createDevKernel } from "$lib/kernel/dev";
import type { DiagramLanguageId, DiagramTheme, MarkdownFence } from "$lib/kernel/types";

import {
  FenceRenderer,
  PAPER_DIAGRAM_THEME,
  fenceProblems,
  fillPlaceholders,
  type FenceRendererDeps,
} from "./markdownDiagrams";

const SVG = new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg"/>');

function fence(
  source: string,
  line = 3,
  language: DiagramLanguageId = "mermaid",
): MarkdownFence {
  return {
    language,
    source,
    line,
    endLine: line + 2,
    key: line.toString(16).padStart(16, "0"),
  };
}

/** A fake diagram plugin and URL factory that count what they did. */
function fakes(): FenceRendererDeps & {
  renders: string[];
  revoked: string[];
} {
  const renders: string[] = [];
  const revoked: string[] = [];
  let next = 0;
  return {
    renders,
    revoked,
    api: {
      check: (_language: DiagramLanguageId, source: string) =>
        Promise.resolve({
          diagnostics: source.includes("BAD")
            ? [
                {
                  severity: "error" as const,
                  message: "unexpected token",
                  line: 2,
                  column: 3,
                  endLine: 2,
                  endColumn: 6,
                },
              ]
            : [],
        }),
      render: (_language: DiagramLanguageId, source: string, theme: DiagramTheme) => {
        renders.push(`${theme}:${source}`);
        return source.includes("BAD")
          ? Promise.reject(new Error("[diagram_error] line 2:3: unexpected token"))
          : Promise.resolve(SVG);
      },
    },
    urls: {
      create: () => `blob:fake/${(next += 1)}`,
      revoke: (url) => revoked.push(url),
    },
  };
}

const placeholder = (f: MarkdownFence): string =>
  `<figure class="mng-diagram" data-diagram="${f.key}" data-language="${f.language}" data-line="${f.line}"></figure>`;

describe("diagram fences in the Markdown preview", () => {
  it("draws paper-coloured pictures: the page is paper in every theme", () => {
    expect(PAPER_DIAGRAM_THEME).toBe("light");
  });

  it("fills each placeholder with a blob image, never SVG markup", async () => {
    const deps = fakes();
    const renderer = new FenceRenderer(deps);
    const a = fence("flowchart TD\n  A --> B\n", 3);
    const results = await renderer.prepare([a], "light");
    const html = fillPlaceholders(`<h1>T</h1>\n${placeholder(a)}\n<p>x</p>`, results);
    expect(html).toContain(
      `<figure class="mng-diagram" data-diagram="${a.key}" data-language="mermaid" data-line="3"><img src="blob:fake/1" alt="Mermaid diagram (line 3)" data-diagram-fence></figure>`,
    );
    expect(html).not.toContain("<svg");
    expect(html.startsWith("<h1>T</h1>")).toBe(true);
  });

  it("re-renders only fences that changed, and releases pictures that went away", async () => {
    const deps = fakes();
    const renderer = new FenceRenderer(deps);
    const a = fence("A\n", 3);
    const b = fence("B\n", 9);
    await renderer.prepare([a, b], "light");
    expect(deps.renders).toEqual(["light:A\n", "light:B\n"]);

    // b edited: only b renders again, and its old picture is released.
    const b2 = { ...b, source: "B2\n", key: "9b".padStart(16, "0") };
    const second = await renderer.prepare([a, b2], "light");
    expect(deps.renders).toEqual(["light:A\n", "light:B\n", "light:B2\n"]);
    expect(deps.revoked).toEqual(["blob:fake/2"]);
    expect(second.get(a.key)?.url).toBe("blob:fake/1");

    await renderer.release();
    expect(deps.revoked.sort()).toEqual(["blob:fake/1", "blob:fake/2", "blob:fake/3"]);
  });

  it("shows a quiet, escaped error box and a Problem on the document line", async () => {
    const deps = fakes();
    const renderer = new FenceRenderer(deps);
    const bad = fence("flowchart TD\n  A BAD <b>\n", 12);
    const results = await renderer.prepare([bad], "light");
    const html = fillPlaceholders(placeholder(bad), results);
    expect(html).toContain("data-diagram-error");
    expect(html).toContain("Mermaid diagram, line 12: Line 2: unexpected token");
    expect(html).not.toContain("<img");

    const problems = fenceProblems("notes.md", [bad], results);
    expect(problems).toHaveLength(1);
    expect(problems[0]).toMatchObject({
      file: "notes.md",
      line: 14,
      column: 3,
      severity: "error",
      source: "diagram",
    });
  });

  it("leaves anything that is not exactly a placeholder alone", () => {
    const results = new Map([
      ["0123456789abcdef", { url: "blob:x", error: "", diagnostics: [] }],
    ]);
    const escaped =
      "&lt;figure class=&quot;mng-diagram&quot; data-diagram=&quot;0123456789abcdef&quot;&gt;";
    expect(fillPlaceholders(escaped, results)).toBe(escaped);
    const unknown =
      '<figure class="mng-diagram" data-diagram="fedcba9876543210" data-language="mermaid" data-line="1"></figure>';
    expect(fillPlaceholders(unknown, results)).toBe(unknown);
  });

  it("works end to end with the dev kernel's markdown and diagram plugins", async () => {
    const kernel = createDevKernel();
    const source =
      "# Notes\n\n```mermaid\nflowchart TD\n  A[Start] --> B[Done]\n```\n\n```mermaid\nflowchart TD\n  A -->>> B\n```\n";
    const rendered = await markdownApi(kernel).renderHtml(source);
    expect(rendered.diagrams.map((d) => [d.language, d.line, d.endLine])).toEqual([
      ["mermaid", 3, 6],
      ["mermaid", 8, 11],
    ]);
    const listed = await markdownApi(kernel).diagrams(source);
    expect(listed.diagrams).toEqual(rendered.diagrams);

    const urls: string[] = [];
    const renderer = new FenceRenderer({
      api: diagramApi(kernel),
      urls: {
        create: () => {
          urls.push(`blob:dev/${urls.length}`);
          return urls[urls.length - 1] ?? "";
        },
        revoke: () => undefined,
      },
    });
    const results = await renderer.prepare(rendered.diagrams, PAPER_DIAGRAM_THEME);
    const html = fillPlaceholders(rendered.html, results);
    expect(html.match(/<img src="blob:dev\/0"/g)).toHaveLength(1);
    expect(html).toContain("data-diagram-error");
    const problems = fenceProblems("notes.md", rendered.diagrams, results);
    expect(problems.map((p) => [p.line, p.column])).toEqual([[10, 5]]);
  });
});
