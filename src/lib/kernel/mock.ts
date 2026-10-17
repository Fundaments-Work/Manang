/**
 * In-memory kernel for the browser.
 *
 * `pnpm dev`, Playwright and vitest run without the Tauri shell, so there is no
 * Rust editor plugin to talk to. This mirrors that plugin's contract exactly:
 * documents indexed by Unicode scalar (never UTF-16), a revision bumped by every
 * edit, the same argument names and the same failure behaviour. Keep it in step
 * with `crates/manang-editor/manifest.toml`.
 */

import type { Args, Kernel, KernelCommand, KernelError } from "./client";

export type MockHandler = (args: Args) => unknown;

export interface MockOptions {
  /** Extra commands, e.g. `lint.diagnostics`, keyed by id. */
  readonly handlers?: Readonly<Record<string, MockHandler>>;
  /** Extra commands that return binary artefacts, keyed by id. */
  readonly binaryHandlers?: Readonly<Record<string, (args: Args) => Uint8Array>>;
  /** Titles for the extra commands; defaults to the id. */
  readonly titles?: Readonly<Record<string, string>>;
  /** Files `editor.open` can read. */
  readonly files?: Readonly<Record<string, string>>;
}

interface Doc {
  /** One entry per Unicode scalar, matching ropey's `char` indexing. */
  chars: string[];
  path: string | null;
  revision: number;
  savedRevision: number;
}

const EDITOR_COMMANDS: readonly [string, string][] = [
  ["editor.activate", "Switch document"],
  ["editor.delete", "Delete range"],
  ["editor.info", "Document info"],
  ["editor.insert", "Insert text"],
  ["editor.new", "New document"],
  ["editor.open", "Open document"],
  ["editor.save", "Save document"],
  ["editor.text", "Read document text"],
];

function fail(message: string): KernelError {
  return { message };
}

function uint(args: Args, key: string): number {
  const value = args[key];
  if (typeof value !== "number" || !Number.isInteger(value) || value < 0) {
    throw fail(`\`${key}\` must be an unsigned integer`);
  }
  return value;
}

function str(args: Args, key: string): string {
  const value = args[key];
  if (typeof value !== "string") throw fail(`\`${key}\` must be a string`);
  return value;
}

export function createMockKernel(options: MockOptions = {}): Kernel & {
  /** Edit a document as if the kernel itself changed it (test hook). */
  mutate(id: number | null, at: number, deleteCount: number, insert: string): number;
} {
  const docs = new Map<number, Doc>();
  const files = new Map(Object.entries(options.files ?? {}));
  let nextId = 0;
  let active: number | null = null;

  function create(text: string, path: string | null): number {
    const id = nextId;
    nextId += 1;
    docs.set(id, { chars: Array.from(text), path, revision: 0, savedRevision: 0 });
    active = id;
    return id;
  }

  function resolve(args: Args): [number, Doc] {
    const raw = args["id"];
    let id: number;
    if (raw === undefined || raw === null) {
      if (active === null) throw fail("no document is active");
      id = active;
    } else if (typeof raw === "number" && Number.isInteger(raw) && raw >= 0) {
      id = raw;
    } else {
      throw fail("`id` must be an unsigned integer");
    }
    const doc = docs.get(id);
    if (doc === undefined) throw fail(`no document with id ${id}`);
    return [id, doc];
  }

  function range(doc: Doc, from: number, to: number): void {
    if (from > to || to > doc.chars.length) {
      throw fail(
        `range ${from}..${to} is outside the document (${doc.chars.length} characters)`,
      );
    }
  }

  function splice(doc: Doc, from: number, to: number, insert: string): number {
    range(doc, from, to);
    doc.chars.splice(from, to - from, ...Array.from(insert));
    doc.revision += 1;
    return doc.revision;
  }

  const handlers: Record<string, MockHandler> = {
    "editor.new": () => ({ id: create("", null) }),
    "editor.open": (args) => {
      const path = str(args, "path");
      const text = files.get(path);
      if (text === undefined) throw fail(`${path}: No such file or directory`);
      return { id: create(text, path) };
    },
    "editor.save": (args) => {
      const [, doc] = resolve(args);
      const given = args["path"];
      const path = typeof given === "string" ? given : doc.path;
      if (path === null) {
        throw fail("document has no path; open or save it with a path first");
      }
      files.set(path, doc.chars.join(""));
      doc.path = path;
      doc.savedRevision = doc.revision;
      return null;
    },
    "editor.activate": (args) => {
      if (args["id"] === undefined || args["id"] === null)
        throw fail("`id` is required");
      const [id] = resolve(args);
      active = id;
      return null;
    },
    "editor.insert": (args) => {
      const [, doc] = resolve(args);
      const at = uint(args, "at");
      return { revision: splice(doc, at, at, str(args, "text")) };
    },
    "editor.delete": (args) => {
      const [, doc] = resolve(args);
      return { revision: splice(doc, uint(args, "from"), uint(args, "to"), "") };
    },
    "editor.text": (args) => resolve(args)[1].chars.join(""),
    "editor.info": (args) => {
      const [id, doc] = resolve(args);
      const text = doc.chars.join("");
      return {
        id,
        path: doc.path,
        chars: doc.chars.length,
        lines: text.split("\n").length,
        revision: doc.revision,
        dirty: doc.revision !== doc.savedRevision,
      };
    },
    ...options.handlers,
  };

  const binary = options.binaryHandlers ?? {};

  return {
    commands(): Promise<KernelCommand[]> {
      const list: KernelCommand[] = EDITOR_COMMANDS.map(([id, title]) => ({
        id,
        title,
        plugin: "editor",
      }));
      for (const id of [
        ...Object.keys(options.handlers ?? {}),
        ...Object.keys(binary),
      ]) {
        list.push({
          id,
          title: options.titles?.[id] ?? id,
          plugin: id.split(".")[0] ?? id,
        });
      }
      list.sort((a, b) => a.id.localeCompare(b.id));
      return Promise.resolve(list);
    },

    run<T = unknown>(command: string, args: Args = {}): Promise<T> {
      const handler = handlers[command];
      if (handler === undefined) {
        return Promise.reject(fail(`unknown command \`${command}\``));
      }
      try {
        // Structured clone mimics the IPC boundary: callers never share state.
        return Promise.resolve(structuredClone(handler(args)) as T);
      } catch (error) {
        return Promise.reject(error);
      }
    },

    runBinary(command: string, args: Args = {}): Promise<Uint8Array> {
      const handler = binary[command];
      if (handler === undefined) {
        return Promise.reject(fail(`unknown command \`${command}\``));
      }
      try {
        return Promise.resolve(handler(args));
      } catch (error) {
        return Promise.reject(error);
      }
    },

    mutate(id, at, deleteCount, insert): number {
      const [, doc] = resolve({ id });
      return splice(doc, at, at + deleteCount, insert);
    },
  };
}
