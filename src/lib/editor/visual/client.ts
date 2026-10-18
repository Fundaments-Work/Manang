/**
 * Typed client of the `visual.*` kernel commands (ADR 0015 section 4).
 *
 * A thin wrapper: it names the commands, shapes the arguments and decodes the
 * binary span table. It holds no state; the session id is the caller's.
 */

import type { Kernel } from "$lib/kernel/client";

import {
  type AdapterInfo,
  type BlocksArgs,
  type CommandState,
  type EditArgs,
  type EditPlan,
  type EditResult,
  type OpenArgs,
  type OpenResult,
  type OutlineEntry,
  type ResetArgs,
  type ResetResult,
  type RunCommandArgs,
  type VisualErrorCode,
} from "./protocol";
import { decodeSpanTable, type DecodeResult } from "./table";

type Transport = Pick<Kernel, "run" | "runBinary">;

export interface VisualApi {
  adapters(): Promise<AdapterInfo[]>;
  open(args: OpenArgs): Promise<OpenResult>;
  reset(args: ResetArgs): Promise<ResetResult>;
  close(session: number): Promise<void>;
  edit(args: EditArgs): Promise<EditResult>;
  /** The decoded span table; a malformed one is a value (`ok: false`), not a throw. */
  blocks(args: BlocksArgs): Promise<DecodeResult>;
  outline(session: number): Promise<OutlineEntry[]>;
  commandState(session: number, from: number, to: number): Promise<CommandState[]>;
  runCommand(args: RunCommandArgs): Promise<EditPlan>;
}

export function visualApi(kernel: Transport): VisualApi {
  return {
    adapters: () => kernel.run<AdapterInfo[]>("visual.adapters"),
    open: (args) => kernel.run<OpenResult>("visual.open", { ...args }),
    reset: (args) => kernel.run<ResetResult>("visual.reset", { ...args }),
    close: async (session) => {
      await kernel.run("visual.close", { session });
    },
    edit: (args) =>
      kernel.run<EditResult>("visual.edit", {
        session: args.session,
        changes: args.changes,
        len: args.len,
      }),
    blocks: async (args) =>
      decodeSpanTable(
        await kernel.runBinary("visual.blocks", {
          session: args.session,
          from: args.from,
          to: args.to,
        }),
      ),
    outline: (session) => kernel.run<OutlineEntry[]>("visual.outline", { session }),
    commandState: (session, from, to) =>
      kernel.run<CommandState[]>("visual.command-state", { session, from, to }),
    runCommand: async (args) =>
      normalizePlan(
        await kernel.run<unknown>("visual.run-command", {
          session: args.session,
          command: args.command,
          from: args.from,
          to: args.to,
          ...(args.args === undefined ? {} : { args: args.args }),
        }),
      ),
  };
}

const CODE = /^\[(visual\.[a-z-]+)\]\s*/;

/** The code and text of a failed `visual.*` call (`[code] message`). */
export function parseVisualError(error: unknown): {
  code: VisualErrorCode | null;
  message: string;
} {
  const raw =
    typeof error === "object" && error !== null && "message" in error
      ? String((error as { message: unknown }).message)
      : String(error);
  const match = CODE.exec(raw);
  return {
    code: match === null ? null : (match[1] as VisualErrorCode),
    message: match === null ? raw : raw.slice(match[0].length),
  };
}

export const isDesync = (error: unknown): boolean =>
  parseVisualError(error).code === "visual.desync";

/** A plan from the wire, with anything that is not a change list refused. */
export function normalizePlan(value: unknown): EditPlan {
  const bad = new Error("[visual.failed] the edit plan is malformed");
  if (typeof value !== "object" || value === null) throw bad;
  const raw = value as { changes?: unknown; selection?: unknown };
  if (!Array.isArray(raw.changes)) throw bad;
  const changes = raw.changes.map((c: unknown) => {
    if (typeof c !== "object" || c === null) throw bad;
    const { from, to, insert } = c as Record<string, unknown>;
    if (
      typeof from !== "number" ||
      typeof to !== "number" ||
      typeof insert !== "string" ||
      !Number.isInteger(from) ||
      !Number.isInteger(to)
    )
      throw bad;
    return { from, to, insert };
  });
  let selection: EditPlan["selection"] = null;
  if (raw.selection !== null && raw.selection !== undefined) {
    const { from, to } = raw.selection as Record<string, unknown>;
    if (typeof from !== "number" || typeof to !== "number") throw bad;
    selection = { from, to };
  }
  return { changes, selection };
}

/**
 * Whether a plan can be applied to a document of `docLength`: changes sorted,
 * non-overlapping and inside the text, the selection inside the result. An
 * invalid plan is dropped, never applied half way.
 */
export function planFits(plan: EditPlan, docLength: number): boolean {
  let at = 0;
  let length = docLength;
  for (const c of plan.changes) {
    if (c.from < at || c.to < c.from || c.to > docLength || c.from < 0) return false;
    at = c.to;
    length += c.insert.length - (c.to - c.from);
  }
  const s = plan.selection;
  return s === null || (s.from >= 0 && s.from <= s.to && s.to <= length);
}
