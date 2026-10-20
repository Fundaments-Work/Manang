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
        "live.attach",
        "live.configure",
        "live.contributors",
        "live.detach",
        "live.exclude",
        "live.files",
        "live.follow",
        "live.include",
        "live.inspect",
        "live.invite",
        "live.join",
        "live.leave",
        "live.push",
        "live.resolve",
        "live.restore",
        "live.resync",
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
      path: "main.typ",
      version: 0,
      changes: [{ start: 3, delete: 0, insert: "d" }],
    });
    expect(ok).toEqual({ accepted: true, version: 1 });
    expect(await kernel.run("editor.text", { id })).toBe("abcd");
    const stale = await live.push({
      session,
      path: "main.typ",
      version: 0,
      changes: [{ start: 0, delete: 0, insert: "X" }],
    });
    expect(stale).toEqual({ accepted: false, version: 1 });
    expect(await kernel.run("editor.text", { id })).toBe("abcd");
    await tick();
    const acks = topics("live.remote").map((e) => e.payload);
    expect(acks).toEqual([{ session, version: 1, ack: true, changes: [] }]);
    const bad = await live
      .push({
        session,
        path: "main.typ",
        version: 1,
        changes: [{ start: 99, delete: 0, insert: "x" }],
      })
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
      path: "main.typ",
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
      path: "main.typ",
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

describe("the mock live plugin, project sessions", () => {
  const files = {
    "main.typ": "= Title\n",
    "chapters/one.typ": "One\n",
    "refs.bib": "@x{}\n",
  };

  async function host() {
    const r = await rig("");
    await r.live.configure({ "live.relay-url": "wss://relay.example.org" });
    await r.kernel.run("dev.live-seed", { files });
    return r;
  }

  it("starts a project: the invite says so and the tree is listed", async () => {
    const { live, kernel } = await host();
    const started = await live.start({ scope: "project", root: "/p" });
    expect(started).toMatchObject({ scope: "project", role: "host" });
    expect(started.invite).toContain("scope=project");
    expect(await live.inspect(started.invite ?? "")).toMatchObject({
      scope: "project",
    });
    const status = await live.status();
    expect(status).toMatchObject({ active: true, scope: "project", files: 3 });
    expect((await live.files()).files.map((f) => f.path)).toEqual([
      "chapters/one.typ",
      "main.typ",
      "refs.bib",
    ]);
    // A single-file invite still reports its scope.
    const single = await kernel.run<{ invite: string }>("dev.live-make-invite", {
      text: "x",
    });
    expect(await live.inspect(single.invite)).toMatchObject({ scope: "file" });
  });

  it("keeps a document per file, each with its own version and path on its events", async () => {
    const { live, kernel, tick, topics } = await host();
    const { session } = await live.start({ scope: "project", root: "/p" });
    const a = await live.attach("main.typ");
    const b = await live.attach("chapters/one.typ");
    expect([a, b]).toEqual([
      { version: 0, text: "= Title\n" },
      { version: 0, text: "One\n" },
    ]);
    const ok = await live.push({
      session,
      path: "main.typ",
      version: 0,
      changes: [{ start: 8, delete: 0, insert: "More\n" }],
    });
    expect(ok).toEqual({ accepted: true, version: 1 });
    // The other file is still at 0: its push is accepted against 0.
    const other = await live.push({
      session,
      path: "chapters/one.typ",
      version: 0,
      changes: [{ start: 0, delete: 0, insert: ">" }],
    });
    expect(other.accepted).toBe(true);
    expect((await live.snapshot(session, "main.typ")).text).toBe("= Title\nMore\n");
    expect((await live.snapshot(session, "chapters/one.typ")).text).toBe(">One\n");
    await tick();
    const acks = topics("live.remote").map((e) => (e.payload as { path: string }).path);
    expect(acks).toEqual(["main.typ", "chapters/one.typ"]);
    const notIn = await live
      .push({ session, path: "nope.typ", version: 0, changes: [] })
      .catch((e: unknown) => liveError(e));
    expect(notIn).toMatchObject({ code: "not-in-session" });
    expect(kernel).toBeDefined();
  });

  it("detaches and re-attaches", async () => {
    const { live } = await host();
    await live.start({ scope: "project", root: "/p" });
    await live.attach("main.typ");
    expect(await live.detach("main.typ")).toEqual({ detached: true });
    expect(await live.detach("main.typ")).toEqual({ detached: false });
  });

  it("joins in two phases: a plan, then the choices", async () => {
    const { kernel, live, topics } = await rig("");
    const made = await kernel.run<{ invite: string }>("dev.live-make-invite", {
      files: {
        "main.typ": "host main\n",
        "chapters/one.typ": "same\n",
        "new.typ": "only on host\n",
      },
    });
    await kernel.run("dev.live-local", {
      files: {
        "main.typ": "my main\n",
        "chapters/one.typ": "same\n",
        "notes.txt": "mine\n",
      },
    });
    const joined = await live.join({ invite: made.invite, root: "/p" });
    expect(joined).toMatchObject({ scope: "project", role: "guest" });
    await new Promise((r) => setTimeout(r, 60));
    const plan = topics("live.plan").at(-1)?.payload as {
      files: { path: string; class: string }[];
    };
    expect(Object.fromEntries(plan.files.map((f) => [f.path, f.class]))).toEqual({
      "main.typ": "differing",
      "chapters/one.typ": "identical",
      "new.typ": "missing",
      "notes.txt": "guest-only",
    });
    // Nothing is in the session until the guest decides.
    expect(await live.files().catch((e: unknown) => liveError(e))).toMatchObject({
      code: "invalid",
    });
    // A differing file with no answer is refused: never silent.
    const refused = await live.resolve([]).catch((e: unknown) => liveError(e));
    expect(refused).toMatchObject({ code: "unresolved" });
    const out = await live.resolve([
      { path: "main.typ", action: "take-host" },
      { path: "notes.txt", action: "share" },
    ]);
    expect(out).toEqual({ applied: 4, trashed: ["main.typ"] });
    expect((await live.files()).files.map((f) => f.path)).toEqual([
      "chapters/one.typ",
      "main.typ",
      "new.typ",
      "notes.txt",
    ]);
    expect((await live.attach("main.typ")).text).toBe("host main\n");
    expect((await live.attach("notes.txt")).text).toBe("mine\n");
    await new Promise((r) => setTimeout(r, 20));
    const progress = topics("live.progress").map((e) => e.payload);
    expect(progress.at(-1)).toMatchObject({ done: 4, total: 4 });
    expect(await live.resolve([]).catch((e: unknown) => liveError(e))).toMatchObject({
      code: "no-plan",
    });
  });

  it("keeps a differing file out of the session when the guest keeps their own", async () => {
    const { kernel, live } = await rig("");
    const made = await kernel.run<{ invite: string }>("dev.live-make-invite", {
      files: { "a.typ": "host\n" },
    });
    await kernel.run("dev.live-local", { files: { "a.typ": "mine\n" } });
    await live.join({ invite: made.invite, root: "/p" });
    await new Promise((r) => setTimeout(r, 60));
    const out = await live.resolve([{ path: "a.typ", action: "keep-mine" }]);
    expect(out).toEqual({ applied: 0, trashed: [] });
    const refused = await live.attach("a.typ").catch((e: unknown) => liveError(e));
    expect(refused).toMatchObject({ code: "excluded" });
    await live.include("a.typ");
  });

  it("shows two scripted peers, where they are, and what they do to the tree", async () => {
    const { live, kernel, tick, topics } = await host();
    await live.start({ scope: "project", root: "/p" });
    const ada = (
      await kernel.run<{ peer: string }>("dev.live-peer-join", { name: "Ada", slot: 1 })
    ).peer;
    const bo = (
      await kernel.run<{ peer: string }>("dev.live-peer-join", {
        name: "Bo",
        slot: 2,
        path: "refs.bib",
      })
    ).peer;
    await kernel.run("dev.live-peer-open", { peer: ada, path: "main.typ" });
    await kernel.run("dev.live-peer-caret", {
      peer: ada,
      path: "main.typ",
      anchor: 2,
      head: 3,
    });
    await kernel.run("dev.live-peer-caret", {
      peer: bo,
      path: "refs.bib",
      anchor: 0,
      head: 1,
    });
    await tick();
    const peers = topics("live.peers").at(-1)?.payload as {
      peers: { peer: string; path: string | null }[];
    };
    expect(peers.peers.map((p) => [p.peer, p.path])).toEqual([
      [ada, "main.typ"],
      [bo, "refs.bib"],
    ]);
    // Presence is per file: each file reports only its own people.
    const presence = topics("live.presence").filter(
      (e) => (e.payload as { path: string }).path === "main.typ",
    );
    expect(
      (presence.at(-1)?.payload as { selections: { name: string }[] }).selections.map(
        (s) => s.name,
      ),
    ).toEqual(["Ada"]);
    expect(
      (await live.files()).files.find((f) => f.path === "main.typ")?.peers,
    ).toEqual([ada]);

    await kernel.run("dev.live-peer-create", {
      peer: bo,
      path: "extra.typ",
      text: "x",
    });
    await kernel.run("dev.live-peer-rename", {
      peer: bo,
      from: "extra.typ",
      to: "moved.typ",
    });
    await kernel.run("dev.live-peer-delete", { peer: ada, path: "chapters/one.typ" });
    await kernel.run("dev.live-peer-leave", { peer: bo });
    await tick();
    const ops = topics("live.tree").flatMap(
      (e) => (e.payload as { ops: unknown[] }).ops,
    );
    expect(ops).toEqual([
      { op: "create", path: "extra.typ", by: bo },
      { op: "rename", path: "moved.typ", from: "extra.typ", by: bo },
      { op: "delete", path: "chapters/one.typ", by: ada },
    ]);
    const kinds = topics("live.notice").map(
      (e) => (e.payload as { kind: string }).kind,
    );
    expect(kinds).toEqual(["joined", "joined", "renamed", "deleted", "left"]);
    const bad = await kernel
      .run("dev.live-peer-create", { peer: ada, path: "../escape.typ" })
      .catch((e: unknown) => liveError(e));
    expect(bad).toMatchObject({ code: "bad-path" });

    // A file someone deleted can be restored.
    expect((await live.restore("chapters/one.typ")).restored).toBe(true);
    expect((await live.attach("chapters/one.typ")).text).toBe("One\n");
  });

  it("accepts follow and file exclusion as session commands", async () => {
    const { live } = await host();
    await live.start({ scope: "project", root: "/p" });
    expect(await live.follow("7001")).toEqual({ following: "7001" });
    expect(await live.follow(null)).toEqual({ following: null });
    await live.exclude("refs.bib");
    expect((await live.files()).files.map((f) => f.path)).not.toContain("refs.bib");
    expect(await live.include("refs.bib")).toEqual({ included: true });
    const err = await live.restore("main.typ").catch((e: unknown) => liveError(e));
    expect(err).toMatchObject({ code: "not-deleted" });
  });

  it("needs the open folder to start or join a project", async () => {
    const { live, kernel } = await host();
    const none = await live
      .start({ scope: "project" })
      .catch((e: unknown) => liveError(e));
    expect(none).toMatchObject({ code: "no-project" });
    const made = await kernel.run<{ invite: string }>("dev.live-make-invite", {
      files,
    });
    const noJoin = await live
      .join({ invite: made.invite })
      .catch((e: unknown) => liveError(e));
    expect(noJoin).toMatchObject({ code: "no-project" });
    expect((await live.status()).active).toBe(false);
  });

  it("reports phase, unsynced files and no doc in project status", async () => {
    const { live, kernel, tick, topics } = await host();
    await live.start({ scope: "project", root: "/p" });
    const alone = await live.status();
    expect(alone).toMatchObject({
      scope: "project",
      phase: "live",
      unsynced_files: 3,
      paused: null,
      skipped: [],
      following: null,
    });
    expect(alone).not.toHaveProperty("doc");
    await kernel.run("dev.live-peer-join", { name: "Ada", slot: 1 });
    expect((await live.status()).unsynced_files).toBe(0);
    await kernel.run("dev.live-unsynced", { files: 2 });
    await tick();
    expect((await live.status()).unsynced_files).toBe(2);
    expect(topics("live.peers").at(-1)?.payload).toMatchObject({ unsynced_files: 2 });
  });

  it("walks a guest through the phases and refuses commands out of turn", async () => {
    const { kernel, live } = await rig("");
    const made = await kernel.run<{ invite: string }>("dev.live-make-invite", {
      files,
    });
    await live.join({ invite: made.invite, root: "/p" });
    expect((await live.status()).phase).toBe("manifest");
    expect(await live.files().catch((e: unknown) => liveError(e))).toMatchObject({
      code: "waiting",
    });
    await new Promise((r) => setTimeout(r, 60));
    expect((await live.status()).phase).toBe("plan");
    expect(
      await live.attach("main.typ").catch((e: unknown) => liveError(e)),
    ).toMatchObject({
      code: "invalid",
    });
    await live.resolve([]);
    expect((await live.status()).phase).toBe("live");
  });

  it("refuses replacing several files without the all-files confirmation", async () => {
    const { kernel, live } = await rig("");
    const made = await kernel.run<{ invite: string }>("dev.live-make-invite", {
      files: { "a.typ": "h1", "b.typ": "h2" },
    });
    await kernel.run("dev.live-local", { files: { "a.typ": "m1", "b.typ": "m2" } });
    await live.join({ invite: made.invite, root: "/p" });
    await new Promise((r) => setTimeout(r, 60));
    const explicit = [
      { path: "a.typ", action: "take-host" as const },
      { path: "b.typ", action: "take-host" as const },
    ];
    expect(
      await live.resolve(explicit).catch((e: unknown) => liveError(e)),
    ).toMatchObject({
      code: "needs-confirmation",
    });
    const out = await live.resolve([...explicit, { path: "*", action: "take-host" }]);
    expect(out.trashed).toEqual(["a.typ", "b.typ"]);
  });

  it("holds a large change until it is shared or discarded", async () => {
    const { live, kernel, tick, topics } = await host();
    await live.start({ scope: "project", root: "/p" });
    await kernel.run("dev.live-paused", {
      files: 12,
      changed_lines: 300,
      total_lines: 800,
    });
    await tick();
    expect(topics("live.paused").at(-1)?.payload).toMatchObject({
      files: 12,
      changed_lines: 300,
      total_lines: 800,
    });
    expect((await live.status()).paused).toMatchObject({ files: 12 });
    expect(await live.resync().catch((e: unknown) => liveError(e))).toMatchObject({
      code: "paused",
    });
    expect(await live.resync({ paused: "discard" })).toEqual({ resyncing: true });
    expect((await live.status()).paused).toBeNull();
  });

  it("can delete a file without saying who did it", async () => {
    const { live, kernel, tick, topics } = await host();
    await live.start({ scope: "project", root: "/p" });
    const ada = (await kernel.run<{ peer: string }>("dev.live-peer-join", {})).peer;
    await kernel.run("dev.live-peer-delete", {
      peer: ada,
      path: "refs.bib",
      anonymous: true,
    });
    await tick();
    expect(topics("live.tree").at(-1)?.payload).toMatchObject({
      ops: [{ op: "delete", path: "refs.bib", by: "" }],
    });
  });

  it("refuses project commands in a single-file session", async () => {
    const { live, id } = await rig("abc");
    await live.configure({ "live.relay-url": "ws://127.0.0.1:1" });
    await live.start({ doc: id });
    const err = await live.attach("main.typ").catch((e: unknown) => liveError(e));
    expect(err).toMatchObject({ code: "not-project" });
  });
});
