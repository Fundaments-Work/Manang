/**
 * In-memory `figure` plugin.
 *
 * Mirrors `crates/manang-figure`: open figures live in a store with a revision
 * counter and an undo/redo history; edits arrive through `figure.apply` and
 * are applied with `$lib/figure/ops` (the TypeScript mirror of the Rust
 * operations, held in step by `parity.test.ts`); v1 files migrate on open;
 * saving writes the scene JSON and its SVG next to it. Paths are absolute, as
 * in the desktop app, and resolve inside the open mock project. Keep the
 * command shapes in step with `manifest.toml` and `src/plugin.rs`.
 */

import { hitTest } from "$lib/figure/geometry";
import { approxMath, paintApprox } from "$lib/figure/mathApprox";
import { parseHexColour } from "$lib/figure/palette";
import type { MathLanguage } from "$lib/figure/scene";
import { applyOp, type FigureOp } from "$lib/figure/ops";
import { renderSceneSvg } from "$lib/figure/render";
import { createScene, migrateScene, type FigureScene } from "$lib/figure/scene";

import agentTools from "../../../crates/manang-figure/src/agent_tools.json";

import type { MockPlugin, MockPluginFactory } from "./mock";
import { mockMermaidLayout } from "./mockDiagram";
import * as tools from "./mockFigureTools";
import type { MockProjectFs, MockProjectHost } from "./mockProject";

export { renderSceneSvg };

const COMMANDS: readonly (readonly [string, string])[] = [
  ["figure.create", "New figure"],
  ["figure.open", "Open figure"],
  ["figure.save", "Save figure"],
  ["figure.close", "Close figure"],
  ["figure.list", "List open figures"],
  ["figure.apply", "Apply edit operation"],
  ["figure.get", "Get figure scene"],
  ["figure.render-svg", "Render a scene to SVG"],
  ["figure.export-svg", "Export SVG"],
  ["figure.render-math", "Render a formula to vector SVG"],
  ["figure.hit-test", "Hit test coordinates"],
  ["figure.describe", "Describe a figure (agent tool)"],
  ["figure.add-shapes", "Add shapes (agent tool)"],
  ["figure.connect", "Connect shapes (agent tool)"],
  ["figure.update", "Change shapes (agent tool)"],
  ["figure.remove", "Remove shapes (agent tool)"],
  ["figure.layout", "Lay out a figure"],
  ["figure.from-mermaid", "Insert Mermaid as shapes"],
  ["figure.to-mermaid", "Figure as Mermaid"],
  ["figure.style", "Apply a style preset"],
  ["figure.export", "Save and export a figure"],
];

const MAX_HISTORY = 100;
const DEFAULT_WIDTH = 800;
const DEFAULT_HEIGHT = 600;

/** `x.figure`, `x.figure.json` (kept as given), `x.svg` or `x` → the scene path and its SVG. */
export function figurePaths(path: string): { json: string; svg: string } {
  // A legacy `.figure.json` keeps its own name (saving never creates a second
  // file); everything else is saved as `.figure`.
  if (path.endsWith(".figure.json")) {
    return { json: path, svg: `${path.slice(0, -".figure.json".length)}.svg` };
  }
  const stem = path.endsWith(".figure")
    ? path.slice(0, -".figure".length)
    : path.replace(/\.(svg|json)$/, "");
  return { json: `${stem}.figure`, svg: `${stem}.svg` };
}

interface Doc {
  readonly id: number;
  path: string | null;
  scene: FigureScene;
  revision: number;
  savedRevision: number;
  undo: FigureScene[];
  redo: FigureScene[];
}

function fail(message: string): { message: string } {
  return { message };
}

function str(args: Record<string, unknown>, key: string): string {
  const value = args[key];
  if (typeof value !== "string") throw fail(`\`${key}\` must be a string`);
  return value;
}

function num(args: Record<string, unknown>, key: string): number {
  const value = args[key];
  if (typeof value !== "number") throw fail(`\`${key}\` must be a number`);
  return value;
}

function optionalId(args: Record<string, unknown>): number | null {
  const value = args["id"];
  if (value === undefined || value === null) return null;
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0) {
    throw fail("`id` must be an unsigned integer");
  }
  return value;
}

function pushBounded(stack: FigureScene[], scene: FigureScene): void {
  if (stack.length >= MAX_HISTORY) stack.shift();
  stack.push(scene);
}

const KNOWN_OPS = new Set([
  "add",
  "update",
  "remove",
  "move",
  "resize",
  "rotate",
  "reorder",
  "group",
  "ungroup",
  "bind",
  "align",
  "distribute",
  "set-style",
  "set-canvas",
  "batch",
  "clear",
  "undo",
  "redo",
  "add-node",
  "update-node",
  "remove-node",
  "translate",
]);

/** An `op` as the Rust plugin parses it: a bare string or a `type`/`kind` tag work too. */
function parseOp(op: unknown): FigureOp {
  const raw: Record<string, unknown> =
    typeof op === "string"
      ? { op: op.toLowerCase() }
      : typeof op === "object" && op !== null && !Array.isArray(op)
        ? { ...(op as Record<string, unknown>) }
        : {};
  if (!("op" in raw)) {
    for (const alt of ["type", "kind"]) {
      if (alt in raw) {
        raw["op"] = raw[alt];
        delete raw[alt];
        break;
      }
    }
  }
  const tag = raw["op"];
  if (typeof tag !== "string" || !KNOWN_OPS.has(tag)) {
    throw fail(`invalid argument: invalid \`op\`: unknown variant \`${String(tag)}\``);
  }
  return raw as unknown as FigureOp;
}

/** Ids whose node differs between two scenes, sorted. */
function changedBetween(a: FigureScene, b: FigureScene): string[] {
  const ids = new Set([...a.nodes, ...b.nodes].map((n) => n.id));
  const out: string[] = [];
  for (const id of ids) {
    const x = a.nodes.find((n) => n.id === id);
    const y = b.nodes.find((n) => n.id === id);
    if (JSON.stringify(x) !== JSON.stringify(y)) out.push(id);
  }
  return out.sort();
}

export function figurePlugin(host: MockProjectHost): MockPluginFactory {
  return (emit): MockPlugin => {
    const docs = new Map<number, Doc>();
    let nextId = 0;
    let active: number | null = null;

    /** The open project and the project-relative form of an absolute path. */
    const locate = (path: string): { fs: MockProjectFs; rel: string } => {
      const fs = host.require();
      const prefix = `${fs.root}/`;
      if (!path.startsWith(prefix)) {
        throw fail(`I/O error at \`${path}\`: outside the open project`);
      }
      return { fs, rel: path.slice(prefix.length) };
    };

    const resolve = (args: Record<string, unknown>): Doc => {
      const id = optionalId(args) ?? active;
      if (id === null) throw fail("no figure is currently active");
      const doc = docs.get(id);
      if (doc === undefined) throw fail(`no figure with id ${id}`);
      return doc;
    };

    const add = (path: string | null, scene: FigureScene): Doc => {
      nextId += 1;
      const doc: Doc = {
        id: nextId,
        path,
        scene,
        revision: 0,
        savedRevision: 0,
        undo: [],
        redo: [],
      };
      docs.set(doc.id, doc);
      active = doc.id;
      return doc;
    };

    const write = (
      doc: Doc,
    ): { json: string; svg: string; revision: number; pdf: string | null } => {
      if (doc.path === null) throw fail("figure has no file path associated");
      const paths = figurePaths(doc.path);
      const json = locate(paths.json);
      const svg = locate(paths.svg);
      // LaTeX projects also get the PDF `\includegraphics` takes, as the
      // real plugin asks `project.export-pdf` for.
      const pdfRel =
        svg.fs.engine === "latex" ? svg.rel.replace(/\.svg$/, ".pdf") : null;
      const touched = [
        json,
        svg,
        ...(pdfRel === null ? [] : [{ fs: svg.fs, rel: pdfRel }]),
      ];
      const changes = touched.map(({ fs, rel }) => ({
        path: rel,
        kind: fs.files.has(rel) ? "modified" : "created",
      }));
      json.fs.files.set(json.rel, `${JSON.stringify(doc.scene, null, 2)}\n`);
      svg.fs.files.set(svg.rel, renderSceneSvg(doc.scene));
      if (pdfRel !== null) {
        svg.fs.files.set(pdfRel, "%PDF-1.7\n% Manang browser mock: one page.\n%%EOF\n");
      }
      doc.savedRevision = doc.revision;
      emit("project.changed", { root: json.fs.root, changes });
      return { json: paths.json, svg: paths.svg, revision: doc.revision, pdf: pdfRel };
    };

    const result = (doc: Doc, applied: boolean, changed: string[]) => ({
      revision: doc.revision,
      applied,
      can_undo: doc.undo.length > 0,
      can_redo: doc.redo.length > 0,
      changed,
    });

    const apply = (doc: Doc, raw: unknown) => {
      const op = parseOp(raw);
      if (op.op === "undo" || op.op === "redo") {
        const from = op.op === "undo" ? doc.undo : doc.redo;
        const to = op.op === "undo" ? doc.redo : doc.undo;
        const target = from.pop();
        if (target === undefined) return result(doc, false, []);
        pushBounded(to, doc.scene);
        const changed = changedBetween(doc.scene, target);
        doc.scene = target;
        doc.revision += 1;
        return result(doc, true, changed);
      }
      let next;
      try {
        next = applyOp(doc.scene, op);
      } catch (error) {
        throw fail(error instanceof Error ? error.message : String(error));
      }
      if (!next.applied) return result(doc, false, []);
      pushBounded(doc.undo, doc.scene);
      doc.redo = [];
      doc.scene = next.scene;
      doc.revision += 1;
      return result(doc, true, next.changed);
    };

    /* ------------------------------------------------------------------ */
    /* TOOLS: the agent-tool commands, mirroring crates/manang-figure's   */
    /* tools.rs (see mockFigureTools.ts for where the mock is simpler).   */
    /* ------------------------------------------------------------------ */

    /** The figure a tool names: `figure` (an id), else `path`, else active. */
    const target = (args: Record<string, unknown>): Doc => {
      const figure = args["figure"];
      if (typeof figure === "number") {
        const doc = docs.get(figure);
        if (doc === undefined)
          throw tools.toolError("not_found", `no figure with id ${figure}`);
        return doc;
      }
      if (typeof args["path"] === "string") {
        const json = figurePaths(args["path"]).json;
        const open = [...docs.values()].find((d) => d.path === json);
        if (open !== undefined) return open;
        const { fs, rel } = locate(json);
        const text = fs.files.get(rel);
        if (typeof text !== "string") {
          throw tools.toolError(
            "io",
            `I/O error at \`${args["path"]}\`: file not found`,
          );
        }
        return add(json, migrateScene(JSON.parse(text)));
      }
      if (active === null)
        throw tools.toolError("not_found", "no figure is currently active");
      const doc = docs.get(active);
      if (doc === undefined)
        throw tools.toolError("not_found", "no figure is currently active");
      return doc;
    };

    const figureRef = (doc: Doc) => ({
      id: doc.id,
      path: doc.path,
      revision: doc.revision,
    });

    /** Applies a tool's edit, announces it and answers with the ids. */
    const commit = (doc: Doc, op: FigureOp, ids: string[]) => {
      let outcome;
      try {
        outcome = apply(doc, op);
      } catch (error) {
        const message =
          error instanceof Error
            ? error.message
            : (error as { message?: string }).message;
        throw tools.toolError("invalid_argument", String(message));
      }
      if (outcome.applied)
        emit("figure.changed", { id: doc.id, revision: doc.revision });
      return { figure: figureRef(doc), ids };
    };

    const toolHandlers: Record<string, (args: Record<string, unknown>) => unknown> = {
      "figure.describe": (args) => {
        const doc = target(args);
        return { figure: figureRef(doc), ...tools.describe(doc.scene) };
      },
      "figure.add-shapes": (args) => {
        const doc = target(args);
        const edit = tools.addShapes(doc.scene, args);
        return {
          ...commit(doc, edit.op, edit.ids),
          shapes: edit.shapes.map(tools.shapeSummary),
        };
      },
      "figure.connect": (args) => {
        const doc = target(args);
        const edit = tools.connect(doc.scene, args);
        return commit(doc, edit.op, edit.ids);
      },
      "figure.update": (args) => {
        const doc = target(args);
        const edit = tools.update(doc.scene, args);
        return commit(doc, edit.op, edit.ids);
      },
      "figure.remove": (args) => {
        const doc = target(args);
        const edit = tools.remove(doc.scene, args);
        return commit(doc, edit.op, edit.ids);
      },
      "figure.layout": (args) => {
        const doc = target(args);
        const edit = tools.layout(doc.scene, args);
        return commit(doc, edit.op, edit.ids);
      },
      "figure.style": (args) => {
        const doc = target(args);
        const edit = tools.style(doc.scene, args);
        return commit(doc, edit.op, edit.ids);
      },
      "figure.to-mermaid": (args) => {
        const doc = target(args);
        return { figure: figureRef(doc), ...tools.toMermaid(doc.scene) };
      },
      "figure.from-mermaid": (args) => {
        const doc = target(args);
        const source = args["source"];
        if (typeof source !== "string" || source.trim() === "") {
          throw tools.toolError("invalid_argument", "`source` is empty");
        }
        const mode = typeof args["mode"] === "string" ? args["mode"] : "auto";
        if (!["auto", "shapes", "block"].includes(mode)) {
          throw tools.toolError(
            "invalid_argument",
            `unknown mode \`${mode}\`; use auto, shapes or block`,
          );
        }
        const x = typeof args["x"] === "number" ? args["x"] : undefined;
        const y = typeof args["y"] === "number" ? args["y"] : undefined;
        const origin = tools.insertionPoint(doc.scene, x, y);
        const counter = tools.highestCounter(doc.scene);
        let fallback: string | null = null;
        if (mode !== "block") {
          try {
            const layout = mockMermaidLayout(source);
            const nodes = tools.nodesFromLayout(layout, origin, counter);
            return {
              ...commit(
                doc,
                { op: "add", nodes },
                nodes.map((n) => n.id),
              ),
              mode: "shapes",
              family: layout["family"],
              fallback: null,
            };
          } catch (error) {
            fallback = error instanceof Error ? error.message : String(error);
          }
          if (mode === "shapes") {
            throw tools.toolError(
              "unavailable",
              `this Mermaid cannot become shapes: ${fallback}`,
            );
          }
        }
        const id = `n${(counter + 1).toString(36)}`;
        const block = {
          kind: "mermaid" as const,
          id,
          x: origin.x,
          y: origin.y,
          width: 360,
          height: 240,
          source,
        };
        return {
          ...commit(doc, { op: "add", nodes: [block] }, [id]),
          mode: "block",
          family: null,
          fallback,
        };
      },
      "figure.export": (args) => {
        const doc = target(args);
        const format = typeof args["format"] === "string" ? args["format"] : "svg";
        if (format !== "svg" && format !== "pdf") {
          throw tools.toolError(
            "invalid_argument",
            `unknown format \`${format}\`; use svg or pdf`,
          );
        }
        const saved = write(doc);
        return {
          figure: figureRef(doc),
          json: saved.json,
          svg: saved.svg,
          pdf: saved.pdf,
          frame_svg: null,
        };
      },
    };

    return {
      name: "figure",
      commands: COMMANDS,
      agentTools,
      handlers: {
        ...toolHandlers,
        "figure.create": (args) => {
          const width =
            typeof args["width"] === "number" ? args["width"] : DEFAULT_WIDTH;
          const height =
            typeof args["height"] === "number" ? args["height"] : DEFAULT_HEIGHT;
          if (args["path"] === undefined || args["path"] === null) {
            return { id: add(null, createScene(width, height)).id };
          }
          const paths = figurePaths(str(args, "path"));
          const { fs, rel } = locate(paths.json);
          if (fs.files.has(rel)) {
            throw fail(`a figure already exists at \`${paths.json}\``);
          }
          const doc = add(paths.json, createScene(width, height));
          const saved = write(doc);
          return { id: doc.id, json: saved.json, svg: saved.svg };
        },
        "figure.open": (args) => {
          const path = str(args, "path");
          const paths = figurePaths(path);
          const { fs, rel } = locate(paths.json);
          const text = fs.files.get(rel);
          if (typeof text !== "string") {
            throw fail(`I/O error at \`${path}\`: file not found`);
          }
          let scene: FigureScene;
          try {
            scene = migrateScene(JSON.parse(text));
          } catch (error) {
            throw fail(
              error instanceof Error ? error.message : `JSON error: ${String(error)}`,
            );
          }
          return { id: add(paths.json, scene).id };
        },
        "figure.get": (args) => resolve(args).scene,
        "figure.apply": (args) => {
          const doc = resolve(args);
          if (!("op" in args)) throw fail("missing `op` field");
          return apply(doc, args["op"]);
        },
        "figure.save": (args) => {
          const doc = resolve(args);
          if (typeof args["path"] === "string") {
            doc.path = figurePaths(args["path"]).json;
          }
          return write(doc);
        },
        "figure.close": (args) => {
          const id = optionalId(args);
          if (id === null) throw fail("`id` is required");
          const closed = docs.delete(id);
          if (closed && active === id) active = [...docs.keys()].pop() ?? null;
          return { closed };
        },
        "figure.list": () =>
          [...docs.values()].map((doc) => ({
            id: doc.id,
            path: doc.path,
            nodes: doc.scene.nodes.length,
            revision: doc.revision,
            dirty: doc.revision !== doc.savedRevision,
          })),
        "figure.hit-test": (args) => {
          const doc = resolve(args);
          const node = hitTest(doc.scene, { x: num(args, "x"), y: num(args, "y") });
          return node === undefined
            ? { hit: false, node_id: null, node: null }
            : { hit: true, node_id: node.id, node };
        },
      },
      binaryHandlers: {
        "figure.render-svg": (args) => {
          let scene: FigureScene;
          try {
            scene = migrateScene(args["scene"]);
          } catch (error) {
            const why = error instanceof Error ? error.message : String(error);
            throw fail(`invalid \`scene\`: ${why}`);
          }
          return new TextEncoder().encode(renderSceneSvg(scene));
        },
        "figure.export-svg": (args) =>
          new TextEncoder().encode(renderSceneSvg(resolve(args).scene)),
        // The real command is Typst (Typst math) or RaTeX (LaTeX math); the
        // browser mock answers with an approximation (see mathApprox.ts) and
        // the same structured `[math_error]` for broken source.
        "figure.render-math": (args) => {
          const language = (args["language"] ?? "typst") as MathLanguage;
          if (language !== "typst" && language !== "latex") {
            throw fail(
              `unknown math language \`${String(language)}\`; use typst or latex`,
            );
          }
          const size = typeof args["size"] === "number" ? args["size"] : 24;
          try {
            const m = approxMath(language, str(args, "source"), size);
            const colour = args["color"];
            if (typeof colour === "string" && colour !== "currentColor") {
              if (parseHexColour(colour) === undefined) {
                throw fail(
                  `[invalid_argument] \`color\` must be currentColor or #rrggbb, not \`${colour}\``,
                );
              }
              return new TextEncoder().encode(paintApprox(m.svg, colour.toLowerCase()));
            }
            return new TextEncoder().encode(m.svg);
          } catch (error) {
            if (
              (error as { message?: string }).message?.startsWith("[invalid_argument]")
            )
              throw error;
            throw fail(`[math_error] ${JSON.stringify(error)}`);
          }
        },
      },
    };
  };
}
