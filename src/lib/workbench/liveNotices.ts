/**
 * The session's file and people events, turned into short sentences and
 * batched: a branch checkout or a paste of fifty files must not raise fifty
 * toasts.
 */

import type { LiveNoticeEvent, LiveTreeOp } from "$lib/kernel/liveApi";
import { basename } from "$lib/workbench/paths";

/** Collects items and hands them over together, at most once per `ms`. */
export class Throttled<T> {
  #items: T[] = [];
  #timer: ReturnType<typeof setTimeout> | undefined;

  constructor(
    private readonly flush: (items: readonly T[]) => void,
    private readonly ms = 600,
  ) {}

  push(item: T): void {
    this.#items.push(item);
    if (this.#timer === undefined) {
      this.#timer = setTimeout(() => this.flushNow(), this.ms);
    }
  }

  flushNow(): void {
    clearTimeout(this.#timer);
    this.#timer = undefined;
    if (this.#items.length === 0) return;
    const items = this.#items;
    this.#items = [];
    this.flush(items);
  }

  cancel(): void {
    clearTimeout(this.#timer);
    this.#timer = undefined;
    this.#items = [];
  }
}

/** Who a peer id is, for sentences. */
export type NameOf = (peer: string) => string;

export interface LiveNoticeLine {
  readonly kind: LiveNoticeEvent["kind"] | "created" | "files";
  readonly text: string;
  readonly path?: string;
}

/** One sentence for a notice event. */
export function noticeLine(event: LiveNoticeEvent, name: NameOf): LiveNoticeLine {
  const who = name(event.peer);
  const file = event.path === undefined ? "a file" : basename(event.path);
  const path = event.path === undefined ? {} : { path: event.path };
  switch (event.kind) {
    case "joined":
      return { kind: "joined", text: `${who} joined.` };
    case "left":
      return { kind: "left", text: `${who} left.` };
    case "deleted":
      return { kind: "deleted", text: `${who} deleted ${file}.`, ...path };
    case "renamed":
      return { kind: "renamed", text: `${who} renamed a file to ${file}.`, ...path };
  }
}

/** One sentence for a batch of tree operations, by whoever made them. */
export function treeSummary(
  ops: readonly LiveTreeOp[],
  name: NameOf,
): LiveNoticeLine | null {
  const [first] = ops;
  if (first === undefined) return null;
  const who = name(first.by);
  if (ops.length === 1) {
    const file = basename(first.path);
    const text =
      first.op === "create"
        ? `${who} added ${file}.`
        : first.op === "delete"
          ? `${who} deleted ${file}.`
          : `${who} moved ${basename(first.from ?? first.path)} to ${first.path}.`;
    return {
      kind: first.op === "create" ? "created" : "files",
      text,
      path: first.path,
    };
  }
  const everyone = new Set(ops.map((op) => op.by));
  const by = everyone.size === 1 ? who : `${everyone.size} people`;
  return { kind: "files", text: `${by} changed ${ops.length} files.` };
}
