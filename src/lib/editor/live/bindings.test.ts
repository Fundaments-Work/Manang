import { EditorState } from "@codemirror/state";
import { describe, expect, it } from "vitest";

import { liveApi, type LiveApi } from "$lib/kernel/liveApi";
import { createMockKernel } from "$lib/kernel/mock";
import { LiveAuthority, livePlugin } from "$lib/kernel/mockLive";

import type { LiveBinding, LiveHost } from "./binding";
import { LiveBindings, type LiveDocument, type RoutedEvent } from "./bindings";

class FakeDoc implements LiveDocument {
  state: EditorState;
  binding: LiveBinding | null = null;
  readOnly = false;
  attaches = 0;
  detaches = 0;

  constructor(
    readonly path: string,
    text: string,
  ) {
    this.state = EditorState.create({ doc: text });
  }

  get text(): string {
    return this.state.doc.toString();
  }

  liveHost(): LiveHost {
    return {
      state: () => this.state,
      dispatch: (spec) => {
        this.state = this.state.update(spec).state;
      },
    };
  }

  attachLive(binding: LiveBinding): void {
    this.binding = binding;
    this.attaches += 1;
  }

  setLiveReadOnly(_binding: LiveBinding, readOnly: boolean): void {
    this.readOnly = readOnly;
  }

  detachLive(): Promise<void> {
    this.binding = null;
    this.detaches += 1;
    return Promise.resolve();
  }

  /** The person types: the edit goes to the binding like the editor's listener does. */
  type(at: number, insert: string): void {
    const tr = this.state.update({ changes: { from: at, insert } });
    this.state = tr.state;
    this.binding?.local(tr.changes);
  }
}

const tick = (ms = 10) => new Promise((r) => setTimeout(r, ms));

async function projectRig(files: Record<string, string>) {
  const kernel = createMockKernel({ plugins: [livePlugin] });
  const api: LiveApi = liveApi(kernel);
  const calls: string[] = [];
  const spied = {
    ...api,
    detach: (path: string) => {
      calls.push(`detach ${path}`);
      return api.detach(path);
    },
  };
  const bindings = new LiveBindings({ api: spied, watchdogMs: 1e9 });
  await kernel.onEvent((event) => {
    if (event.topic === "live.remote" || event.topic === "live.presence") {
      bindings.route(event as unknown as RoutedEvent);
    }
  });
  await kernel.run("dev.live-seed", { files });
  await api.configure({ "live.relay-url": "wss://relay.example.org" });
  const started = await api.start({ scope: "project" });
  bindings.begin(started.session);
  const peer = (
    await kernel.run<{ peer: string }>("dev.live-peer-join", { name: "Ada" })
  ).peer;
  return { kernel, api, bindings, session: started.session, peer, calls };
}

describe("per-file bindings in a project session", () => {
  it("binds a file when it opens, bringing the view to the session's text", async () => {
    const { bindings, api } = await projectRig({ "a.typ": "alpha", "b.typ": "beta" });
    const doc = new FakeDoc("a.typ", "stale local text");
    const binding = await bindings.attach(doc);
    expect(binding?.path).toBe("a.typ");
    expect(doc.text).toBe("alpha");
    expect(doc.binding).toBe(binding);
    expect(bindings.paths()).toEqual(["a.typ"]);
    expect(bindings.has("b.typ")).toBe(false);
    // The same file again is the same binding, with one attach.
    expect(await bindings.attach(doc)).toBe(binding);
    expect(doc.attaches).toBe(1);
    expect((await api.files()).files.map((f) => f.path)).toEqual(["a.typ", "b.typ"]);
  });

  it("shares one attach between concurrent calls", async () => {
    const { bindings } = await projectRig({ "a.typ": "alpha" });
    const doc = new FakeDoc("a.typ", "alpha");
    const [one, two] = await Promise.all([bindings.attach(doc), bindings.attach(doc)]);
    expect(one).toBe(two);
    expect(doc.attaches).toBe(1);
  });

  it("does not bind a file the session does not have, or when no session runs", async () => {
    const { bindings } = await projectRig({ "a.typ": "alpha" });
    expect(await bindings.attach(new FakeDoc("nope.typ", ""))).toBeNull();
    const idle = new LiveBindings({
      api: liveApi(createMockKernel({ plugins: [livePlugin] })),
    });
    expect(await idle.attach(new FakeDoc("a.typ", ""))).toBeNull();
  });

  it("routes remote steps to the right file only", async () => {
    const { bindings, kernel } = await projectRig({
      "a.typ": "alpha",
      "b.typ": "beta",
    });
    const a = new FakeDoc("a.typ", "alpha");
    const b = new FakeDoc("b.typ", "beta");
    await bindings.attach(a);
    await bindings.attach(b);
    await kernel.run("dev.live-peer-type", { path: "a.typ", at: 5, text: "!" });
    await kernel.run("dev.live-peer-type", { path: "b.typ", at: 0, text: ">" });
    await tick();
    expect(a.text).toBe("alpha!");
    expect(b.text).toBe(">beta");
    expect(bindings.get("a.typ")?.version).toBe(1);
    expect(bindings.get("b.typ")?.version).toBe(1);
  });

  it("pushes local edits to the file they were made in", async () => {
    const { bindings, kernel } = await projectRig({
      "a.typ": "alpha",
      "b.typ": "beta",
    });
    const a = new FakeDoc("a.typ", "alpha");
    const b = new FakeDoc("b.typ", "beta");
    await bindings.attach(a);
    await bindings.attach(b);
    b.type(4, "?");
    await tick();
    expect(await kernel.run("dev.live-text", { path: "b.typ" })).toBe("beta?");
    expect(await kernel.run("dev.live-text", { path: "a.typ" })).toBe("alpha");
    expect(bindings.get("b.typ")?.pending).toBe(false);
  });

  it("keeps events for a file that is not open, and the later attach starts from the session's text", async () => {
    const { bindings, kernel } = await projectRig({
      "a.typ": "alpha",
      "b.typ": "beta",
    });
    await kernel.run("dev.live-peer-type", { path: "b.typ", at: 4, text: "1" });
    await kernel.run("dev.live-peer-type", { path: "b.typ", at: 5, text: "2" });
    await tick();
    // Two steps and the presence update each one brings.
    expect(bindings.buffered("b.typ")).toBe(4);
    const b = new FakeDoc("b.typ", "beta");
    await bindings.attach(b);
    // Both steps are already in the text the session handed over: none is applied twice.
    expect(b.text).toBe("beta12");
    expect(bindings.buffered("b.typ")).toBe(0);
    await kernel.run("dev.live-peer-type", { path: "b.typ", at: 6, text: "3" });
    await tick();
    expect(b.text).toBe("beta123");
  });

  it("applies events that arrive while the attach is in flight", async () => {
    const { bindings, kernel } = await projectRig({ "a.typ": "alpha" });
    const a = new FakeDoc("a.typ", "alpha");
    const attaching = bindings.attach(a);
    await kernel.run("dev.live-peer-type", { path: "a.typ", at: 5, text: "!" });
    await attaching;
    await tick();
    expect(a.text).toBe("alpha!");
  });

  it("caps what is kept per file", () => {
    const bindings = new LiveBindings({
      api: liveApi(createMockKernel({ plugins: [livePlugin] })),
    });
    for (let i = 1; i <= 650; i += 1) {
      bindings.route({
        topic: "live.remote",
        payload: { session: "s", path: "a.typ", version: i, ack: false, changes: [] },
      });
    }
    bindings.route({
      topic: "live.remote",
      payload: { session: "s", path: "b.typ", version: 1, ack: false, changes: [] },
    });
    expect(bindings.buffered("a.typ")).toBe(500);
    expect(bindings.buffered("b.typ")).toBe(1);
  });

  it("drops what was kept for other sessions when a session begins", () => {
    const bindings = new LiveBindings({
      api: liveApi(createMockKernel({ plugins: [livePlugin] })),
    });
    const event = (session: string, path: string): RoutedEvent => ({
      topic: "live.remote",
      payload: { session, path, version: 1, ack: false, changes: [] },
    });
    bindings.route(event("old", "a.typ"));
    bindings.route(event("new", "b.typ"));
    bindings.begin("new");
    expect(bindings.buffered("a.typ")).toBe(0);
    expect(bindings.buffered("b.typ")).toBe(1);
  });

  it("detaches one file and leaves the others live", async () => {
    const { bindings, kernel, calls } = await projectRig({
      "a.typ": "alpha",
      "b.typ": "beta",
    });
    const a = new FakeDoc("a.typ", "alpha");
    const b = new FakeDoc("b.typ", "beta");
    await bindings.attach(a);
    await bindings.attach(b);
    await bindings.detach("a.typ");
    expect(bindings.paths()).toEqual(["b.typ"]);
    expect(a.binding).toBeNull();
    expect(a.detaches).toBe(1);
    expect(calls).toEqual(["detach a.typ"]);
    await kernel.run("dev.live-peer-type", { path: "a.typ", at: 5, text: "!" });
    await kernel.run("dev.live-peer-type", { path: "b.typ", at: 4, text: "!" });
    await tick();
    expect(a.text).toBe("alpha");
    expect(b.text).toBe("beta!");
    // Opening it again resumes from the session's text, including what it missed.
    await bindings.attach(a);
    expect(a.text).toBe("alpha!");
    // A file that was never bound is a no-op.
    await bindings.detach("zzz.typ");
  });

  it("detaches quietly when the session already forgot the file", async () => {
    const { bindings, calls } = await projectRig({ "a.typ": "alpha" });
    await bindings.attach(new FakeDoc("a.typ", "alpha"));
    await bindings.detach("a.typ", { quiet: true });
    expect(calls).toEqual([]);
  });

  it("makes every bound file read-only while waiting, including files bound later", async () => {
    const { bindings } = await projectRig({ "a.typ": "alpha", "b.typ": "beta" });
    const a = new FakeDoc("a.typ", "alpha");
    await bindings.attach(a);
    bindings.setReadOnly(true);
    expect(a.readOnly).toBe(true);
    const b = new FakeDoc("b.typ", "beta");
    await bindings.attach(b);
    expect(b.readOnly).toBe(true);
    bindings.setReadOnly(false);
    expect([a.readOnly, b.readOnly]).toEqual([false, false]);
  });

  it("gives every document back when the session ends", async () => {
    const { bindings } = await projectRig({ "a.typ": "alpha", "b.typ": "beta" });
    const a = new FakeDoc("a.typ", "alpha");
    const b = new FakeDoc("b.typ", "beta");
    await bindings.attach(a);
    await bindings.attach(b);
    await bindings.closeAll();
    expect(bindings.paths()).toEqual([]);
    expect([a.detaches, b.detaches]).toEqual([1, 1]);
    expect(bindings.session).toBeNull();
  });
});

describe("a single-file session", () => {
  it("adopts one document and takes events with or without a path", () => {
    const events: RoutedEvent[] = [];
    const authority = new LiveAuthority("s", "hello", (topic, payload) => {
      events.push({ topic, payload } as RoutedEvent);
    });
    const bindings = new LiveBindings({
      api: liveApi(createMockKernel({ plugins: [livePlugin] })),
      watchdogMs: 1e9,
    });
    // Events can come before the session is known.
    authority.remote([{ start: 5, delete: 0, insert: "!" }]);
    for (const event of events.splice(0)) bindings.route(event);
    const doc = new FakeDoc("main.typ", "hello");
    bindings.adopt(doc, "s", 0);
    expect(doc.text).toBe("hello!");
    authority.remote([{ start: 0, delete: 0, insert: ">" }]);
    for (const event of events.splice(0)) bindings.route(event);
    expect(doc.text).toBe(">hello!");
    expect(bindings.paths()).toEqual(["main.typ"]);
  });

  it("ignores events of another session", () => {
    const bindings = new LiveBindings({
      api: liveApi(createMockKernel({ plugins: [livePlugin] })),
    });
    const doc = new FakeDoc("main.typ", "hello");
    bindings.adopt(doc, "s", 0);
    bindings.route({
      topic: "live.remote",
      payload: {
        session: "other",
        version: 1,
        ack: false,
        changes: [{ start: 0, delete: 0, insert: "x" }],
      },
    });
    expect(doc.text).toBe("hello");
  });

  it("abandons without touching the documents", () => {
    const bindings = new LiveBindings({
      api: liveApi(createMockKernel({ plugins: [livePlugin] })),
    });
    const doc = new FakeDoc("main.typ", "hello");
    bindings.adopt(doc, "s", 0);
    bindings.abandon();
    expect(bindings.paths()).toEqual([]);
    expect(doc.detaches).toBe(0);
  });
});
