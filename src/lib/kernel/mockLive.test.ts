import { describe, expect, it } from "vitest";

import type { KernelEvent } from "./client";
import { liveApi, liveError } from "./liveApi";
import { createMockKernel } from "./mock";
import { livePlugin, mapThrough } from "./mockLive";

async function rig(text = "Hello wörld 😀!\n") {
  const kernel = createMockKernel({ plugins: [livePlugin] });
  const live = liveApi(kernel);
  const events: KernelEvent[] = [];
  await kernel.onEvent((e) => events.push(e));
  const { id } = await kernel.run<{ id: number }>("editor.new");
  await kernel.run("editor.insert", { id, at: 0, text });
  const tick = () => new Promise((r) => setTimeout(r, 5));
  const topics = (topic: string) => events.filter((e) => e.topic === topic);
  return { kernel, live, id, events, tick, topics };
}

describe("the mock live plugin", () => {
  it("lists the same commands as the Rust manifest", async () => {
    const { kernel } = await rig();
    const ids = (await kernel.commands())
      .filter((c) => c.plugin === "live")
      .map((c) => c.id);
    expect(ids).toEqual(
      [
        "live.configure",
        "live.contributors",
        "live.inspect",
        "live.invite",
        "live.join",
        "live.leave",
        "live.push",
        "live.selection",
        "live.snapshot",
        "live.start",
        "live.status",
      ].sort(),
    );
  });

  it("refuses to share without a relay, with a code the shell understands", async () => {
    const { live, id } = await rig();
    expect((await live.status()).relay_set).toBe(false);
    const error = await live.start({ doc: id }).catch((e: unknown) => liveError(e));
    expect(error).toMatchObject({ code: "no-relay" });
    await live.configure({ "live.relay-url": "relay.example.org" });
    expect((await live.status()).relay_set).toBe(true);
    const started = await live.start({ doc: id, file: "main.typ" });
    expect(started.role).toBe("host");
    expect(started.invite).toMatch(
      /^manang-live:\/\/join\?relay=wss%3A%2F%2Frelay\.example\.org/,
    );
    expect(started.invite?.split("#")[1]).toBeTruthy();
    const again = await live.start({ doc: id }).catch((e: unknown) => liveError(e));
    expect(again).toMatchObject({ code: "already-live" });
    expect((await live.leave()).left).toBe(true);
    expect((await live.leave()).left).toBe(false);
  });

  it("accepts a push at the current version, refuses an old one, and mirrors to the editor", async () => {
    const { live, id, kernel, tick, topics } = await rig("abc");
    await live.configure({ "live.relay-url": "ws://127.0.0.1:1" });
    const { session } = await live.start({ doc: id });
    const ok = await live.push({
      session,
      version: 0,
      changes: [{ start: 3, delete: 0, insert: "d" }],
    });
    expect(ok).toEqual({ accepted: true, version: 1 });
    expect(await kernel.run("editor.text", { id })).toBe("abcd");
    const stale = await live.push({
      session,
      version: 0,
      changes: [{ start: 0, delete: 0, insert: "X" }],
    });
    expect(stale).toEqual({ accepted: false, version: 1 });
    expect(await kernel.run("editor.text", { id })).toBe("abcd");
    await tick();
    const acks = topics("live.remote").map((e) => e.payload);
    expect(acks).toEqual([{ session, version: 1, ack: true, changes: [] }]);
    const bad = await live
      .push({ session, version: 1, changes: [{ start: 99, delete: 0, insert: "x" }] })
      .catch((e: unknown) => liveError(e));
    expect(bad).toMatchObject({ code: "invalid" });
    await live.leave();
  });

  it("indexes the editor by scalars while the session indexes UTF-16", async () => {
    const { live, id, kernel } = await rig("😀ab");
    await live.configure({ "live.relay-url": "ws://127.0.0.1:1" });
    const { session } = await live.start({ doc: id });
    await live.push({
      session,
      version: 0,
      changes: [{ start: 3, delete: 0, insert: "X" }],
    });
    expect(await kernel.run("editor.text", { id })).toBe("😀aXb");
    await live.leave();
  });

  it("walks the states and shows a scripted collaborator", async () => {
    const { live, id, kernel, tick, topics } = await rig("abc");
    await live.configure({ "live.relay-url": "ws://127.0.0.1:1" });
    const { session } = await live.start({ doc: id });
    await new Promise((r) => setTimeout(r, 80));
    expect((await live.status()).state).toBe("connected");
    const joined = await kernel.run<{ peer: string }>("dev.live-peer-join", {
      name: "Ada",
      slot: 2,
    });
    await kernel.run("dev.live-peer-type", { at: 3, text: "!" });
    await kernel.run("dev.live-peer-caret", { peer: joined.peer, anchor: 1, head: 2 });
    await kernel.run("dev.live-link", { state: "down" });
    await kernel.run("dev.live-link", { state: "up" });
    await kernel.run("dev.live-fail", { message: "boom" });
    await tick();
    const states = topics("live.state").map(
      (e) => (e.payload as { state: string }).state,
    );
    expect(states).toEqual([
      "connecting",
      "connected",
      "connected",
      "reconnecting",
      "connected",
      "error",
    ]);
    const remote = topics("live.remote").map((e) => e.payload);
    expect(remote).toContainEqual({
      session,
      version: 1,
      ack: false,
      changes: [{ start: 3, delete: 0, insert: "!" }],
    });
    const last = topics("live.presence").at(-1)?.payload as { selections: unknown[] };
    expect(last.selections).toEqual([
      { peer: joined.peer, name: "Ada", slot: 2, anchor: 1, head: 2 },
    ]);
    expect(await kernel.run("editor.text", { id })).toBe("abc!");
    expect((await live.status()).peers).toEqual([
      { peer: joined.peer, name: "Ada", slot: 2 },
    ]);
  });

  it("joins: an equal copy seeds locally, a different one asks, an empty one just joins", async () => {
    const a = await rig("shared text\n");
    await a.live.configure({ "live.relay-url": "wss://relay.example.org" });
    const { invite } = await a.live.start({ doc: a.id, file: "main.typ" });
    await a.live.leave();
    expect(invite).toBeDefined();

    const same = await rig("shared text\n");
    const made = await same.kernel.run<{ invite: string }>("dev.live-make-invite", {
      text: "shared text\n",
      file: "main.typ",
    });
    const joined = await same.live.join({ doc: same.id, invite: made.invite });
    expect(joined.role).toBe("guest");
    expect(joined.file).toBe("main.typ");
    expect(joined.reset ?? null).toBeNull();
    await same.live.leave();

    const other = await rig("my own text\n");
    const err = await other.live
      .join({ doc: other.id, invite: made.invite })
      .catch((e: unknown) => liveError(e));
    expect(err).toMatchObject({ code: "differs" });
    expect((await other.live.status()).active).toBe(false);
    expect(await other.kernel.run("editor.text", { id: other.id })).toBe(
      "my own text\n",
    );
    const taken = await other.live.join({
      doc: other.id,
      invite: made.invite,
      resolution: "take-host",
    });
    expect(taken.reset).toEqual({ start: 0, delete: 12, insert: "" });
    await new Promise((r) => setTimeout(r, 200));
    expect(await other.kernel.run("editor.text", { id: other.id })).toBe(
      "shared text\n",
    );
    await other.live.leave();

    const empty = await rig("");
    await empty.live.join({ doc: empty.id, invite: made.invite });
    await new Promise((r) => setTimeout(r, 200));
    expect(await empty.kernel.run("editor.text", { id: empty.id })).toBe(
      "shared text\n",
    );
  });

  it("rejects bad invites", async () => {
    const { live, id } = await rig();
    for (const invite of ["", "nope", "manang-live://join?relay=wss%3A%2F%2Fx.org"]) {
      const err = await live
        .join({ doc: id, invite })
        .catch((e: unknown) => liveError(e));
      expect(err, invite).toMatchObject({ code: "bad-invite" });
    }
  });

  it("delays events on request so a push can be refused as stale", async () => {
    const { live, id, kernel, topics } = await rig("abc");
    await live.configure({ "live.relay-url": "ws://127.0.0.1:1" });
    const { session } = await live.start({ doc: id });
    await kernel.run("dev.live-lag", { ms: 60 });
    await kernel.run("dev.live-peer-type", { at: 0, text: ">" });
    // The event is still on its way, but Rust is already at version 1.
    expect(topics("live.remote")).toHaveLength(0);
    const stale = await live.push({
      session,
      version: 0,
      changes: [{ start: 3, delete: 0, insert: "!" }],
    });
    expect(stale.accepted).toBe(false);
    await new Promise((r) => setTimeout(r, 120));
    expect(topics("live.remote")).toHaveLength(1);
  });
});

describe("mapThrough", () => {
  it("moves a position over sequential replacements", () => {
    expect(mapThrough(5, [{ start: 0, delete: 0, insert: "ab" }])).toBe(7);
    expect(mapThrough(5, [{ start: 6, delete: 0, insert: "ab" }])).toBe(5);
    expect(mapThrough(5, [{ start: 2, delete: 6, insert: "" }])).toBe(2);
    expect(mapThrough(5, [{ start: 5, delete: 0, insert: "x" }])).toBe(5);
  });
});
