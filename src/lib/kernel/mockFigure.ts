/**
 * In-memory `figure` plugin.
 *
 * Mirrors `crates/manang-figure`: open figures live in a store with a revision
 * counter and an undo/redo history; edits arrive through `figure.apply`; saving
 * writes the scene JSON and its SVG next to it. Paths are absolute, as in the
 * desktop app, and resolve inside the open mock project. Keep the command
 * shapes in step with `manifest.toml` and `src/plugin.rs`.
 */

import { applyEdit, type FigureEdit } from "$lib/figure/edit";
import { arrowHead, hitTest, polygonToPath, strokeToPath } from "$lib/figure/geometry";
import { createScene, type FigureNode, type FigureScene } from "$lib/figure/scene";

import type { MockPlugin, MockPluginFactory } from "./mock";
import type { MockProjectFs, MockProjectHost } from "./mockProject";

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
  ["figure.hit-test", "Hit test coordinates"],
];

const MAX_HISTORY = 100;
const DEFAULT_WIDTH = 800;
const DEFAULT_HEIGHT = 600;

function escapeXml(text: string): string {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function nodeSvg(node: FigureNode): string {
  switch (node.kind) {
    case "stroke":
      return `<path id="${node.id}" d="${strokeToPath(node)}" fill="${node.color}"/>`;
    case "rect":
      return `<rect id="${node.id}" x="${node.x}" y="${node.y}" width="${node.width}" height="${node.height}" fill="${node.fill ?? "none"}" stroke="${node.stroke}"/>`;
    case "ellipse":
      return `<ellipse id="${node.id}" cx="${node.cx}" cy="${node.cy}" rx="${node.rx}" ry="${node.ry}" fill="${node.fill ?? "none"}" stroke="${node.stroke}"/>`;
    case "arrow":
      return (
        `<g id="${node.id}"><line x1="${node.from.x}" y1="${node.from.y}" x2="${node.to.x}" y2="${node.to.y}" stroke="${node.color}"/>` +
        `<path d="${polygonToPath(arrowHead(node.from, node.to))}" fill="${node.color}"/></g>`
      );
    case "math":
      return `<text id="${node.id}" x="${node.x}" y="${node.y}" font-size="${node.size}" fill="${node.color}">${escapeXml(node.tex)}</text>`;
  }
}

export function renderSceneSvg(scene: FigureScene): string {
  const body = scene.nodes.map(nodeSvg).join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${scene.width} ${scene.height}" width="${scene.width}" height="${scene.height}">${body}</svg>\n`;
}

/** `x.figure.json`, `x.svg` or `x` → the scene path and its SVG. */
export function figurePaths(path: string): { json: string; svg: string } {
  const stem = path.endsWith(".figure.json")
    ? path.slice(0, -".figure.json".length)
    : path.replace(/\.(svg|json)$/, "");
  return { json: `${stem}.figure.json`, svg: `${stem}.svg` };
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

const EDITS = new Set(["add-node", "update-node", "remove-node", "translate", "clear"]);

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

    const result = (doc: Doc, applied: boolean) => ({
      revision: doc.revision,
      applied,
      can_undo: doc.undo.length > 0,
      can_redo: doc.redo.length > 0,
    });

    const apply = (doc: Doc, op: unknown) => {
      const raw: unknown = typeof op === "string" ? { op } : op;
      const tag =
        typeof raw === "object" && raw !== null
          ? (raw as Record<string, unknown>)["op"]
          : undefined;
      if (tag === "undo" || tag === "redo") {
        const from = tag === "undo" ? doc.undo : doc.redo;
        const to = tag === "undo" ? doc.redo : doc.undo;
        const target = from.pop();
        if (target === undefined) return result(doc, false);
        pushBounded(to, doc.scene);
        doc.scene = target;
        doc.revision += 1;
        return result(doc, true);
      }
      if (typeof tag !== "string" || !EDITS.has(tag)) {
        throw fail(`invalid \`op\`: unknown figure operation \`${String(tag)}\``);
      }
      const edit = raw as FigureEdit;
      if (edit.op === "translate" && edit.dx === 0 && edit.dy === 0) {
        applyEdit(doc.scene, edit); // still refuses a missing node
        return result(doc, false);
      }
      let next: FigureScene;
      try {
        next = applyEdit(doc.scene, edit);
      } catch (error) {
        throw fail(error instanceof Error ? error.message : String(error));
      }
      pushBounded(doc.undo, doc.scene);
      doc.redo = [];
      doc.scene = next;
      doc.revision += 1;
      return result(doc, true);
    };

    return {
      name: "figure",
      commands: COMMANDS,
      handlers: {
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
            scene = JSON.parse(text) as FigureScene;
          } catch (error) {
            throw fail(`JSON error: ${String(error)}`);
          }
          if (!Array.isArray(scene.nodes)) throw fail("JSON error: missing `nodes`");
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
          const scene = args["scene"] as FigureScene | undefined;
          if (scene === undefined || !Array.isArray(scene.nodes)) {
            throw fail("invalid `scene`");
          }
          return new TextEncoder().encode(renderSceneSvg(scene));
        },
        "figure.export-svg": (args) =>
          new TextEncoder().encode(renderSceneSvg(resolve(args).scene)),
      },
    };
  };
}
