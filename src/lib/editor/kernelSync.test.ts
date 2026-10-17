import { EditorState, type TransactionSpec } from "@codemirror/state";
import { describe, expect, it } from "vitest";

import { createMockKernel } from "$lib/kernel/mock";
import { diffRange } from "$lib/kernel/offsets";
import { KernelSync, toKernelOps } from "$lib/kernel/sync";

import { fromKernel, opsForTransactions } from "./kernelSync";

/** A headless stand-in for the view: state plus the same wiring as the real listener. */
function harness(initial: string, failAfter?: { n: number }) {
  const mock = createMockKernel();
  let calls = 0;
  const kernel = {
    ...mock,
    run: <T>(command: string, args?: Record<string, unknown>) => {
      if (
        failAfter !== undefined &&
        command === "editor.insert" &&
        ++calls > failAfter.n
      ) {
        return Promise.reject({ message: "boom" });
      }
      return mock.run<T>(command, args);
    },
  };
  let state = EditorState.create({ doc: initial });
  const echoed: number[] = [];
  const sync = new KernelSync(kernel, {
    applyText: (text) => {
      const { from, to, insert } = diffRange(state.doc.toString(), text);
      const tr = state.update({
        changes: { from, to, insert },
        annotations: fromKernel.of(true),
      });
      state = tr.state;
      // What the real update listener would do with this transaction.
      echoed.push(opsForTransactions([tr]).length);
    },
  });
  return {
    mock,
    sync,
    echoed,
    get text() {
      return state.doc.toString();
    },
    edit(spec: TransactionSpec) {
      const tr = state.update(spec);
      state = tr.state;
      for (const batch of opsForTransactions([tr])) sync.submit(batch);
    },
    kernelText: () => mock.run<string>("editor.text", {}),
  };
}

describe("toKernelOps", () => {
  it("emits scalar indices for edits after an emoji", () => {
    const state = EditorState.create({ doc: "😀ab" });
    const tr = state.update({ changes: { from: 3, to: 3, insert: "X" } });

    expect(toKernelOps(tr.startState.doc, tr.changes)).toEqual([
      { kind: "insert", at: 2, text: "X" },
    ]);
  });

  it("measures deletions in scalars and orders multi-change batches last-to-first", () => {
    const state = EditorState.create({ doc: "😀😀abc" });
    const tr = state.update({
      changes: [
        { from: 0, to: 2 }, // delete first emoji
        { from: 4, to: 5, insert: "Z" }, // replace "a"
      ],
    });

    expect(toKernelOps(tr.startState.doc, tr.changes)).toEqual([
      { kind: "delete", from: 2, to: 3 },
      { kind: "insert", at: 2, text: "Z" },
      { kind: "delete", from: 0, to: 1 },
    ]);
  });

  it("treats combining marks as separate scalars", () => {
    const state = EditorState.create({ doc: "éx" });
    const tr = state.update({ changes: { from: 1, to: 2 } });

    expect(toKernelOps(tr.startState.doc, tr.changes)).toEqual([
      { kind: "delete", from: 1, to: 2 },
    ]);
  });
});

describe("KernelSync", () => {
  it("keeps the kernel identical to the editor through mixed edits", async () => {
    const h = harness("héllo 😀");
    await h.sync.start("héllo 😀");

    h.edit({ changes: { from: 0, insert: "👍 " } });
    h.edit({
      changes: [
        { from: 3, to: 5 },
        { from: 8, insert: "é" },
      ],
    });
    h.edit({ changes: { from: h.text.length, insert: "\n😀😀" } });
    await h.sync.idle();

    expect(await h.kernelText()).toBe(h.text);
    expect(h.sync.status).toBe("synced");
    expect(h.sync.resyncCount).toBe(0);
  });

  it("does not echo kernel-originated changes back to the kernel", async () => {
    const h = harness("abc");
    await h.sync.start("abc");

    h.mock.mutate(null, 3, 0, "def");
    await h.sync.check();

    expect(h.text).toBe("abcdef");
    expect(h.sync.resyncCount).toBe(1);
    expect(h.echoed).toEqual([0]); // the applied transaction yielded no commands
    const info = await h.mock.run<{ revision: number }>("editor.info", {});
    expect(info.revision).toBe(2); // initial insert + the kernel's own edit, nothing more
    expect(h.sync.revision).toBe(2);
  });

  it("resyncs when the kernel changed behind the shell and the next edit lands", async () => {
    const h = harness("abc");
    await h.sync.start("abc");

    h.mock.mutate(null, 0, 1, "X"); // kernel-side edit the shell has not seen
    await h.sync.check();
    h.edit({ changes: { from: 1, insert: "-" } });
    await h.sync.idle();

    expect(h.text).toBe("X-bc");
    expect(await h.kernelText()).toBe("X-bc");
  });

  it("detects a revision mismatch on submit and repairs by pulling kernel text", async () => {
    const h = harness("abc");
    await h.sync.start("abc");

    h.mock.mutate(null, 0, 0, ">"); // revision now ahead of what the shell predicts
    h.edit({ changes: { from: 3, insert: "!" } }); // local doc "abc!"; kernel gets it at 3 => ">ab!c"
    await h.sync.idle();

    expect(h.sync.resyncCount).toBe(1);
    expect(h.text).toBe(await h.kernelText());
    expect(h.sync.revision).toBe(3);
  });

  it("resyncs after a failed command instead of drifting", async () => {
    const h = harness("abc", { n: 1 }); // start's insert succeeds, the next fails
    await h.sync.start("abc");

    h.edit({ changes: { from: 3, insert: "Z" } });
    await h.sync.idle();

    expect(h.text).toBe("abc"); // kernel is the truth; the failed edit is rolled back
    expect(await h.kernelText()).toBe("abc");
    expect(h.sync.status).toBe("synced");
  });

  it("drops commands queued against a document a resync replaced", async () => {
    const h = harness("abc");
    await h.sync.start("abc");

    h.mock.mutate(null, 0, 0, ">");
    const pending = h.sync.check(); // will resync
    h.edit({ changes: { from: 0, insert: "stale" } });
    await pending;
    await h.sync.idle();

    expect(h.text).toBe(">abc");
    expect(await h.kernelText()).toBe(">abc");
  });

  it("goes offline, and stays quiet, when the kernel is unreachable", async () => {
    const h = harness("abc");
    const dead = new KernelSync(
      {
        commands: () => Promise.resolve([]),
        run: () => Promise.reject({ message: "gone" }),
        runBinary: () => Promise.reject({ message: "gone" }),
      },
      { applyText: () => undefined },
    );
    await dead.start("abc");
    dead.submit([{ kind: "insert", at: 0, text: "x" }]);
    await dead.idle();

    expect(dead.status).toBe("offline");
    expect(h.sync.status).toBe("idle");
  });
});
