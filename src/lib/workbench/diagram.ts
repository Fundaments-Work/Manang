/**
 * Diagram files (Mermaid, Pikchr, ASCII via svgbob): which files are diagrams,
 * the starting templates for a new one, the line that includes the rendered
 * picture in a document, and how a render failure names its position.
 *
 * Pure, so the rules are unit-tested; the `diagram.*` plugin does the work.
 */

import type {
  DiagramDiagnostic,
  DiagramLanguageId,
  DiagramTheme,
  Engine,
} from "$lib/kernel/types";

import type { DialogAnswer, DialogRequest } from "./dialogRequest";
import { basename, extension } from "./paths";
import type { Theme } from "./layout";

/** Extension (without the dot) to language, as `diagram.languages` lists them. */
const BY_EXTENSION: Readonly<Record<string, DiagramLanguageId>> = {
  mmd: "mermaid",
  mermaid: "mermaid",
  pikchr: "pikchr",
  bob: "svgbob",
};

export const DIAGRAM_NAMES: Readonly<Record<DiagramLanguageId, string>> = {
  mermaid: "Mermaid",
  pikchr: "Pikchr",
  svgbob: "ASCII diagram",
};

/** The diagram language of a file, or null when it is not a diagram. */
export function diagramLanguageOf(path: string): DiagramLanguageId | null {
  return BY_EXTENSION[extension(path)] ?? null;
}

/** Diagrams follow the app's theme on screen; `print` is for documents. */
export function diagramTheme(theme: Theme): DiagramTheme {
  return theme;
}

/** What "New diagram" offers. */
export interface DiagramKind {
  readonly id: string;
  readonly label: string;
  /** The label in the dialog's kind selector, which has little room. */
  readonly short: string;
  readonly extension: string;
  readonly template: string;
}

export const DIAGRAM_KINDS: readonly DiagramKind[] = [
  {
    id: "mermaid-flowchart",
    label: "Mermaid flowchart",
    short: "Flowchart",
    extension: "mmd",
    template: `flowchart TD
  %% Nodes are an id and a label; arrows join them.
  A[Start] --> B[Measure]
  B --> C[Done]
`,
  },
  {
    id: "mermaid-sequence",
    label: "Mermaid sequence",
    short: "Sequence",
    extension: "mmd",
    template: `sequenceDiagram
  %% Each arrow is one message between participants.
  participant Author
  participant Manang
  Author->>Manang: Write
  Manang-->>Author: Preview
`,
  },
  {
    id: "pikchr",
    label: "Pikchr",
    short: "Pikchr",
    extension: "pikchr",
    template: `box "Input" fit
arrow
box "Process" fit
arrow
box "Output" fit
`,
  },
  {
    id: "ascii",
    label: "ASCII",
    short: "ASCII",
    extension: "bob",
    template: `+--------+     +---------+
| Source |---->| Picture |
+--------+     +---------+
`,
  },
];

/** `figures/<name>.<ext>` for a new diagram; the name loses a typed extension. */
export function diagramPath(name: string, kind: DiagramKind): string {
  const stem = name.trim().replace(/\.(mmd|mermaid|pikchr|bob)$/i, "");
  return `${DIAGRAM_DIR}/${stem}.${kind.extension}`;
}

/** Why `name` cannot name a new diagram, or null when it can. */
export function diagramNameProblem(name: string): string | null {
  const stem = name.trim();
  if (stem === "") return "Give the diagram a name.";
  if (/[\\/]/.test(stem)) return "A name cannot contain a slash.";
  if (stem.startsWith(".")) return "A name cannot start with a dot.";
  return null;
}

/** The New diagram dialog: a name, the kind as a single-select, one Create. */
export function newDiagramRequest(name: string, kindId: string): DialogRequest {
  const kind = DIAGRAM_KINDS.find((k) => k.id === kindId) ?? DIAGRAM_KINDS[0];
  return {
    title: "New diagram",
    message: `A Mermaid, Pikchr or ASCII diagram in ${DIAGRAM_DIR}/. Saving it writes the picture next to it for your documents.`,
    input: { label: "Name", value: name },
    choice: {
      label: "Kind",
      options: DIAGRAM_KINDS.map((k) => ({ id: k.id, label: k.short })),
      value: kind?.id ?? "",
    },
    actions: [{ id: "create", label: "Create" }],
  };
}

/** The kind a New diagram answer asks for, or null when it was cancelled. */
export function chosenKind(answer: DialogAnswer): DiagramKind | null {
  if (answer.id !== "create") return null;
  return DIAGRAM_KINDS.find((k) => k.id === answer.choice) ?? null;
}

/** Where new diagrams go. */
export const DIAGRAM_DIR = "figures";

/** `figures/x.mmd` -> `figures/x` (the rendered files' path without extension). */
function renderedBase(path: string): { base: string; stem: string } {
  const name = basename(path);
  const dot = name.lastIndexOf(".");
  const stem = dot <= 0 ? name : name.slice(0, dot);
  const dir = path.slice(0, path.length - name.length);
  return { base: `${dir}${stem}`, stem };
}

/** The picture `project.export-diagram` writes next to a diagram file. */
export function renderedSvgPath(path: string): string {
  return `${renderedBase(path).base}.svg`;
}

/**
 * The line that includes a diagram's rendered picture in a document of
 * `engine`: Typst and Markdown take the SVG, LaTeX the PDF (both written
 * next to the source on save).
 */
export function includeLine(engine: Engine, path: string): string {
  const { base, stem } = renderedBase(path);
  switch (engine) {
    case "typst":
      return `#figure(image("${base}.svg"))`;
    case "latex":
      return `\\includegraphics{${base}.pdf}`;
    case "markdown":
      return `![${stem}](${base}.svg)`;
  }
}

/**
 * The position in a `[diagram_error] line L:C: message` render failure, as a
 * diagnostic, or null when the message has no position.
 */
export function parseDiagramError(message: string): DiagramDiagnostic | null {
  const match = /^\[diagram_error\]\s*line (\d+):(\d+):\s*([\s\S]*)$/.exec(message);
  if (match === null) return null;
  const line = Number(match[1]);
  const column = Number(match[2]);
  return {
    severity: "error",
    message: (match[3] ?? "").trim(),
    line,
    column,
    endLine: line,
    endColumn: column,
  };
}

/** A render failure as one sentence: the plugin's `[code]` prefix dropped. */
export function diagramErrorText(message: string): string {
  const parsed = parseDiagramError(message);
  if (parsed !== null) return `Line ${parsed.line}: ${parsed.message}`;
  return message.replace(/^\[[a-z_]+\]\s*/, "");
}
