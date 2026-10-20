import { ChangeSet, EditorState, Text } from "@codemirror/state";
import { describe, expect, it } from "vitest";

import { LiveAuthority } from "$lib/kernel/mockLive";
import type { LiveChange, LivePushResult } from "$lib/kernel/liveApi";

import { LiveBinding, rebase, toLiveChanges, type BindingOptions } from "./binding";
import { remoteSelectionsField } from "./remote";

/** Small deterministic generator, so a failing seed can be replayed. */
function random(seed: number): () => number {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13;
    s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5;
    s >>>= 0;
    return s / 0x1_0000_0000;
  };
}

function words(rand: () => number): string {
  const pool = ["alpha", " ", "beta", "\n", "gamma", "é", "δ", " x ", "zz"];
  let out = "";
  const n = 1 + Math.floor(rand() * 3);
  for (let i = 0; i < n; i += 1) out += pool[Math.floor(rand() * pool.length)] ?? "";
  return out;
}

function randomChange(rand: () => number, length: number): LiveChange {
  const start = Math.floor(rand() * (length + 1));
  const del = rand() < 0.4 ? Math.floor(rand() * Math.min(4, length - start + 1)) : 0;
  return { start, delete: del, insert: rand() < 0.2 && del > 0 ? "" : words(rand) };
}

function applyAll(text: string, changes: readonly LiveChange[]): string {
  let out = text;
  for (const c of changes) {
    out = out.slice(0, c.start) + c.insert + out.slice(c.start + c.delete);
  }
  return out;
}

describe("toLiveChanges", () => {
  it("lists changes last first so each index is valid when applied in order", () => {
    for (let seed = 1; seed <= 200; seed += 1) {
      const rand = random(seed);
      const text = words(rand).repeat(4);
      const doc = Text.of(text.split("\n"));
      // Several non-overlapping changes in one set, as one keystroke batch has.
      let set = ChangeSet.empty(doc.length);
      let state = EditorState.create({ doc });
      for (let i = 0; i < 1 + Math.floor(rand() * 3); i += 1) {
        const c = randomChange(rand, state.doc.length);
        const tr = state.update({
          changes: { from: c.start, to: c.start + c.delete, insert: c.insert },
        });
        set = set.compose(tr.changes);
        state = tr.state;
      }
      expect(applyAll(text, toLiveChanges(set))).toBe(state.doc.toString());
    }
  });
});

describe("rebase", () => {
  it("meets at one text whichever side is applied first", () => {
    for (let seed = 1; seed <= 400; seed += 1) {
      const rand = random(seed);
      const base = words(rand).repeat(3);
      const doc = Text.of(base.split("\n"));
      const make = (from: EditorState, n: number): ChangeSet => {
        let set = ChangeSet.empty(from.doc.length);
        let state = from;
        for (let i = 0; i < n; i += 1) {
          const c = randomChange(rand, state.doc.length);
          const tr = state.update({
            changes: { from: c.start, to: c.start + c.delete, insert: c.insert },
          });
          set = set.compose(tr.changes);
          state = tr.state;
        }
        return set;
      };
      const start = EditorState.create({ doc });
      const remote = make(start, 1 + Math.floor(rand() * 2));
      const inflight = make(start, 1 + Math.floor(rand() * 2));
      const afterInflight = start.update({ changes: inflight }).state;
      const unsent =
        rand() < 0.7 ? make(afterInflight, 1 + Math.floor(rand() * 2)) : null;

      const out = rebase(remote, inflight, unsent);
      // The view has remote-less pending applied; the remote change fits on top.
      let viewDoc = afterInflight.doc;
      if (unsent !== null) viewDoc = unsent.apply(viewDoc);
      const viaView = out.view.apply(viewDoc).toString();
      // Rust has the remote step first, then the rebased pending edits.
      let rustDoc = remote.apply(start.doc);
      rustDoc = out.inflight === null ? rustDoc : out.inflight.apply(rustDoc);
      rustDoc = out.unsent === null ? rustDoc : out.unsent.apply(rustDoc);
      expect(viaView, `seed ${seed}`).toBe(rustDoc.toString());
    }
  });
});

/* -------------------------------------------------------------------------- */
/* The binding against the authority, with every message delayed and reordered */
/* -------------------------------------------------------------------------- */

interface Event {
  readonly topic: string;
  readonly payload: unknown;
}

const tick = async (): Promise<void> => {
  for (let i = 0; i < 6; i += 1) await Promise.resolve();
};

class Rig {
  readonly authority: LiveAuthority;
  state: EditorState;
  readonly binding: LiveBinding;
  /** Pushes the view has sent, not yet at the authority. */
  readonly requests: {
    args: Parameters<BindingOptions["api"]["push"]>[0];
    done: (r: LivePushResult) => void;
    fail: (e: unknown) => void;
  }[] = [];
  /** Answers on their way back. */
  readonly answers: (() => void)[] = [];
  /** Events on their way to the view. */
  readonly outbox: Event[] = [];
  readonly resyncs: string[] = [];
  failNextPush = false;
  /** Pushes the authority refused as stale. */
  stale = 0;

  constructor(
    initial: string,
    readonly path = "a.typ",
  ) {
    this.authority = new LiveAuthority(
      "s",
      initial,
      (topic, payload) => {
        this.outbox.push({ topic, payload });
      },
      undefined,
      path,
    );
    this.state = EditorState.create({ doc: initial });
    this.binding = new LiveBinding({
      session: "s",
      path: this.path,
      version: 0,
      host: {
        state: () => this.state,
        dispatch: (spec) => {
          this.state = this.state.update(spec).state;
        },
      },
      api: {
        push: (args) =>
          new Promise<LivePushResult>((resolve, reject) => {
            this.requests.push({ args, done: resolve, fail: reject });
          }),
        selection: () => Promise.resolve(null),
        snapshot: () =>
          Promise.resolve({
            version: this.authority.version,
            text: this.authority.text,
          }),
      },
      onResync: (reason) => this.resyncs.push(reason),
      watchdogMs: 1e9,
    });
  }

  get view(): string {
    return this.state.doc.toString();
  }

  type(change: LiveChange): void {
    const tr = this.state.update({
      changes: {
        from: change.start,
        to: change.start + change.delete,
        insert: change.insert,
      },
    });
    this.state = tr.state;
    this.binding.local(tr.changes);
  }

  /** One scheduler step; false when nothing is in flight. */
  step(rand: () => number): boolean {
    const choices: (() => void)[] = [];
    if (this.requests.length > 0) {
      choices.push(() => {
        const request = this.requests.shift();
        if (request === undefined) return;
        if (this.failNextPush) {
          this.failNextPush = false;
          this.answers.push(() => request.fail(new Error("ipc broke")));
          return;
        }
        const result = this.authority.push(request.args.version, request.args.changes);
        if (!result.accepted) this.stale += 1;
        this.answers.push(() => request.done(result));
      });
    }
    if (this.answers.length > 0) {
      choices.push(() => this.answers.shift()?.());
    }
    if (this.outbox.length > 0) {
      choices.push(() => {
        const event = this.outbox.shift();
        if (event === undefined) return;
        if (event.topic === "live.remote") {
          this.binding.remote(event.payload as Parameters<LiveBinding["remote"]>[0]);
        } else if (event.topic === "live.presence") {
          this.binding.presence(
            event.payload as Parameters<LiveBinding["presence"]>[0],
          );
        }
      });
    }
    const pick = choices[Math.floor(rand() * choices.length)];
    if (pick === undefined) return false;
    pick();
    return true;
  }

  async drain(rand: () => number): Promise<void> {
    for (let guard = 0; guard < 10_000; guard += 1) {
      await tick();
      if (!this.step(rand)) {
        await tick();
        if (!this.step(rand)) return;
      }
    }
    throw new Error("the exchange never settled");
  }
}

describe("LiveBinding against the authority", () => {
  it("applies a remote step and acknowledges its own edits", async () => {
    const rig = new Rig("hello world");
    rig.authority.remote([{ start: 0, delete: 0, insert: ">> " }]);
    rig.type({ start: 11, delete: 0, insert: "!" });
    await rig.drain(random(1));
    expect(rig.view).toBe(">> hello world!");
    expect(rig.authority.text).toBe(">> hello world!");
    expect(rig.binding.pending).toBe(false);
    expect(rig.binding.version).toBe(rig.authority.version);
  });

  it("converges under random typing, remote edits and message delays", async () => {
    let staleTotal = 0;
    for (let seed = 1; seed <= 250; seed += 1) {
      const rand = random(seed * 7919);
      const rig = new Rig(words(rand).repeat(3));
      for (let i = 0; i < 60; i += 1) {
        const roll = rand();
        if (roll < 0.35) {
          rig.type(randomChange(rand, rig.state.doc.length));
        } else if (roll < 0.55) {
          rig.authority.remote([randomChange(rand, rig.authority.text.length)]);
        } else if (roll < 0.6) {
          // A multi-change remote batch, as a CRDT import can produce.
          const first = randomChange(rand, rig.authority.text.length);
          const after = applyAll(rig.authority.text, [first]);
          rig.authority.remote([first, randomChange(rand, after.length)]);
        } else {
          await tick();
          rig.step(rand);
        }
      }
      await rig.drain(rand);
      expect(rig.view, `seed ${seed}`).toBe(rig.authority.text);
      expect(rig.binding.pending, `seed ${seed}`).toBe(false);
      expect(rig.binding.version, `seed ${seed}`).toBe(rig.authority.version);
      expect(rig.binding.resyncs, `seed ${seed}`).toBe(0);
      staleTotal += rig.stale;
      rig.binding.close();
    }
    // The race the protocol exists for really happened.
    expect(staleTotal).toBeGreaterThan(50);
  });

  it("converges per file when two files of one session are typed into at once", async () => {
    let staleTotal = 0;
    for (let seed = 1; seed <= 40; seed += 1) {
      const rand = random(seed * 104_729);
      const a = new Rig(words(rand).repeat(3), "a.typ");
      const b = new Rig(words(rand).repeat(3), "b.typ");
      const rigs = [a, b];
      for (let i = 0; i < 90; i += 1) {
        const rig = rigs[Math.floor(rand() * 2)] ?? a;
        const roll = rand();
        if (roll < 0.35) {
          rig.type(randomChange(rand, rig.state.doc.length));
        } else if (roll < 0.55) {
          rig.authority.remote([randomChange(rand, rig.authority.text.length)]);
        } else {
          await tick();
          rig.step(rand);
        }
      }
      // Drain both in turns: neither file's traffic may disturb the other's.
      await Promise.all([a.drain(rand), b.drain(rand)]);
      for (const rig of rigs) {
        expect(rig.view, `seed ${seed} ${rig.path}`).toBe(rig.authority.text);
        expect(rig.binding.pending, `seed ${seed} ${rig.path}`).toBe(false);
        expect(rig.binding.version, `seed ${seed} ${rig.path}`).toBe(
          rig.authority.version,
        );
        expect(rig.binding.resyncs, `seed ${seed} ${rig.path}`).toBe(0);
        staleTotal += rig.stale;
        rig.binding.close();
      }
      // Each file's events carry its own path, and nothing else's text leaked in.
      expect(a.authority.path).toBe("a.typ");
      expect(b.authority.path).toBe("b.typ");
    }
    expect(staleTotal).toBeGreaterThan(50);
  });

  it("keeps two files apart when events are fed to the wrong binding too", async () => {
    const a = new Rig("alpha", "a.typ");
    const b = new Rig("beta", "b.typ");
    b.authority.remote([{ start: 0, delete: 0, insert: ">" }]);
    const stray = b.outbox.shift();
    a.binding.remote(stray?.payload as Parameters<LiveBinding["remote"]>[0]);
    expect(a.view).toBe("alpha");
    expect(a.binding.version).toBe(0);
    a.binding.close();
    b.binding.close();
  });

  it("holds steps that arrive out of order until the gap is filled", async () => {
    const rig = new Rig("abc");
    rig.authority.remote([{ start: 3, delete: 0, insert: "1" }]);
    rig.authority.remote([{ start: 4, delete: 0, insert: "2" }]);
    rig.authority.remote([{ start: 5, delete: 0, insert: "3" }]);
    const [a, b, c] = rig.outbox.filter((e) => e.topic === "live.remote");
    rig.binding.remote(c?.payload as Parameters<LiveBinding["remote"]>[0]);
    rig.binding.remote(a?.payload as Parameters<LiveBinding["remote"]>[0]);
    expect(rig.view).toBe("abc1");
    rig.binding.remote(b?.payload as Parameters<LiveBinding["remote"]>[0]);
    expect(rig.view).toBe("abc123");
    expect(rig.binding.version).toBe(3);
    // A step it has already integrated is ignored.
    rig.binding.remote(a?.payload as Parameters<LiveBinding["remote"]>[0]);
    expect(rig.view).toBe("abc123");
    rig.binding.close();
  });

  it("re-reads the document when a push fails, and says so", async () => {
    const rig = new Rig("one two");
    rig.failNextPush = true;
    rig.type({ start: 7, delete: 0, insert: " three" });
    await rig.drain(random(5));
    expect(rig.resyncs).toHaveLength(1);
    expect(rig.resyncs[0]).toMatch(/push failed/);
    // The authority never saw the edit, so the view is back to its text.
    expect(rig.view).toBe(rig.authority.text);
    expect(rig.binding.pending).toBe(false);
    // And the session carries on.
    rig.type({ start: 0, delete: 0, insert: "> " });
    await rig.drain(random(6));
    expect(rig.authority.text).toBe("> one two");
    expect(rig.view).toBe("> one two");
    rig.binding.close();
  });

  it("re-reads the document when a step cannot be applied", async () => {
    const rig = new Rig("short");
    rig.binding.remote({
      session: "s",
      version: 1,
      ack: false,
      changes: [{ start: 99, delete: 0, insert: "x" }],
    });
    await tick();
    await tick();
    expect(rig.resyncs).toHaveLength(1);
    expect(rig.view).toBe("short");
    rig.binding.close();
  });

  it("ignores events that name another file, and names its own file when pushing", async () => {
    const rig = new Rig("abc", "a.typ");
    rig.binding.remote({
      session: "s",
      path: "b.typ",
      version: 1,
      ack: false,
      changes: [{ start: 0, delete: 0, insert: "x" }],
    });
    expect(rig.view).toBe("abc");
    rig.type({ start: 3, delete: 0, insert: "!" });
    await tick();
    expect(rig.requests[0]?.args.path).toBe("a.typ");
    rig.binding.close();
  });

  it("ignores other sessions and stops after close", () => {
    const rig = new Rig("abc");
    rig.binding.remote({
      session: "other",
      version: 1,
      ack: false,
      changes: [{ start: 0, delete: 0, insert: "x" }],
    });
    expect(rig.view).toBe("abc");
    rig.binding.close();
    rig.binding.remote({
      session: "s",
      version: 1,
      ack: false,
      changes: [{ start: 0, delete: 0, insert: "x" }],
    });
    rig.type({ start: 0, delete: 0, insert: "y" });
    expect(rig.requests).toHaveLength(0);
    expect(rig.view).toBe("yabc");
  });

  it("draws remote carets shifted over edits that are still pending", () => {
    const base = EditorState.create({
      doc: "hello world",
      extensions: [remoteSelectionsField],
    });
    let state = base;
    const rig = new Rig("hello world");
    const binding = new LiveBinding({
      session: "s",
      path: "a.typ",
      version: 0,
      host: {
        state: () => state,
        dispatch: (spec) => {
          state = state.update(spec).state;
        },
      },
      api: {
        push: () => new Promise<LivePushResult>(() => undefined),
        selection: () => Promise.resolve(null),
        snapshot: () => Promise.resolve({ version: 0, text: "" }),
      },
      watchdogMs: 1e9,
    });
    // The person typed "XX" at the start; Rust has not seen it yet.
    const tr = state.update({ changes: { from: 0, insert: "XX" } });
    state = tr.state;
    binding.local(tr.changes);
    binding.presence({
      session: "s",
      version: 0,
      selections: [{ peer: "7", name: "Ada", slot: 1, anchor: 6, head: 11 }],
    });
    const [sel] = state.field(remoteSelectionsField);
    expect(sel).toMatchObject({ name: "Ada", anchor: 8, head: 13 });
    binding.close();
    rig.binding.close();
  });
});
