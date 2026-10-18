/**
 * Copy and paste of figure nodes through the system clipboard (as JSON
 * text), re-identified on paste, and recognising pasted Mermaid text.
 */

import { isConnector, type Binding, type FigureNode } from "./scene";

export const CLIPBOARD_TYPE = "manang/figure-nodes";

/** Largest image accepted as a `data:` URI in a scene. */
export const MAX_IMAGE_BYTES = 512 * 1024;

/** Offset of a paste or duplicate from the originals. */
export const PASTE_OFFSET = 16;

export function serializeNodes(nodes: readonly FigureNode[]): string {
  return JSON.stringify({ type: CLIPBOARD_TYPE, nodes });
}

/** The nodes in clipboard text, or undefined when it is not ours. */
export function parseNodes(text: string): FigureNode[] | undefined {
  if (!text.trimStart().startsWith("{")) return undefined;
  try {
    const value: unknown = JSON.parse(text);
    if (typeof value !== "object" || value === null) return undefined;
    const v = value as { type?: unknown; nodes?: unknown };
    if (v.type !== CLIPBOARD_TYPE || !Array.isArray(v.nodes)) return undefined;
    return v.nodes.filter(
      (n): n is FigureNode =>
        typeof n === "object" &&
        n !== null &&
        typeof (n as { id?: unknown }).id === "string",
    );
  } catch {
    return undefined;
  }
}

function moved(node: FigureNode, d: number): FigureNode {
  switch (node.kind) {
    case "arrow":
    case "line":
      return { ...node, points: node.points.map((p) => ({ x: p.x + d, y: p.y + d })) };
    case "freedraw":
      return {
        ...node,
        points: node.points.map((p) => ({ ...p, x: p.x + d, y: p.y + d })),
      };
    default:
      return { ...node, x: node.x + d, y: node.y + d };
  }
}

/**
 * Copies of `nodes` with fresh ids (from `nextId`), fresh group ids (from
 * `nextGroup`), moved by `offset`. Bindings and groups among the copies are
 * remapped; bindings to nodes outside the copies are dropped.
 */
export function reidentify(
  nodes: readonly FigureNode[],
  nextId: () => string,
  nextGroup: () => string,
  offset: number = PASTE_OFFSET,
): FigureNode[] {
  const ids = new Map<string, string>();
  for (const n of nodes) ids.set(n.id, nextId());
  const groups = new Map<string, string>();
  const groupFor = (g: string) => {
    let next = groups.get(g);
    if (next === undefined) {
      next = nextGroup();
      groups.set(g, next);
    }
    return next;
  };
  const rebind = (b: Binding | undefined): Binding | undefined => {
    if (b === undefined) return undefined;
    const target = ids.get(b.node);
    if (target === undefined) return undefined;
    return b.anchor === undefined
      ? { node: target }
      : { node: target, anchor: b.anchor };
  };
  return nodes.map((n) => {
    let copy: FigureNode = { ...moved(n, offset), id: ids.get(n.id) as string };
    if (n.groups !== undefined && n.groups.length > 0) {
      copy = { ...copy, groups: n.groups.map(groupFor) };
    }
    if (isConnector(copy)) {
      const { start: s0, end: e0, ...rest } = copy;
      const start = rebind(s0);
      const end = rebind(e0);
      copy = {
        ...rest,
        ...(start === undefined ? {} : { start }),
        ...(end === undefined ? {} : { end }),
      };
    }
    return copy;
  });
}

const MERMAID_START =
  /^(flowchart|graph|sequenceDiagram|stateDiagram(-v2)?|classDiagram|erDiagram|gantt|pie|mindmap|journey|gitGraph|timeline|quadrantChart|requirementDiagram|C4Context|sankey(-beta)?|xychart(-beta)?|block(-beta)?|kanban|architecture(-beta)?|packet(-beta)?|radar(-beta)?|treemap(-beta)?)\b/;

/** Whether pasted text looks like a Mermaid diagram: its first line that is not
 * blank, a `%%` comment or YAML frontmatter names a diagram type. */
export function looksLikeMermaid(text: string): boolean {
  const body = text.replace(/^\s*---\r?\n[\s\S]*?\r?\n---[^\S\n]*\r?\n/, "");
  for (const raw of body.split("\n")) {
    const line = raw.trim();
    if (line === "" || line.startsWith("%%")) continue;
    return MERMAID_START.test(line);
  }
  return false;
}
