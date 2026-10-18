import { describe, expect, it } from "vitest";

import { projectApi, recoveryApi } from "./api";
import { createDevKernel, DEMO_ROOT } from "./dev";
import { MOCK_MAX_SNAPSHOT_BYTES } from "./mockRecovery";

async function rig() {
  const kernel = createDevKernel();
  const project = projectApi(kernel);
  const recovery = recoveryApi(kernel);
  await project.open(DEMO_ROOT);
  return { kernel, project, recovery };
}

describe("the mock recovery plugin", () => {
  it("stores, lists, reads and discards snapshots", async () => {
    const { project, recovery } = await rig();
    const disk = await project.readText("notes.md");
    const out = await recovery.write({
      project: DEMO_ROOT,
      path: "notes.md",
      kind: "text",
      content: "# Notes\nunsaved",
      savedHash: disk.hash,
      view: { anchor: 3, head: 3 },
    });
    expect(out.stored).toBe(true);
    const list = await recovery.list(DEMO_ROOT);
    expect(list.entries).toHaveLength(1);
    expect(list.entries[0]).toMatchObject({ path: "notes.md", disk: "match" });
    const snap = await recovery.read(DEMO_ROOT, "notes.md");
    expect(snap?.text).toBe("# Notes\nunsaved");
    expect(snap?.view).toEqual({ anchor: 3, head: 3 });
    expect((await recovery.discard(DEMO_ROOT, "notes.md")).removed).toBe(1);
    expect(await recovery.read(DEMO_ROOT, "notes.md")).toBeNull();
  });

  it("reports changed and missing files and keeps projects apart", async () => {
    const { kernel, project, recovery } = await rig();
    const disk = await project.readText("notes.md");
    await recovery.write({
      project: DEMO_ROOT,
      path: "notes.md",
      kind: "text",
      content: "x",
      savedHash: disk.hash,
    });
    await recovery.write({
      project: DEMO_ROOT,
      path: "brand-new.typ",
      kind: "text",
      content: "y",
      savedHash: null,
    });
    await recovery.write({
      project: "/elsewhere",
      path: "notes.md",
      kind: "text",
      content: "z",
      savedHash: null,
    });
    await kernel.run("dev.write-outside", { path: "notes.md", content: "other" });
    const states = Object.fromEntries(
      (await recovery.list(DEMO_ROOT)).entries.map((e) => [e.path, e.disk]),
    );
    expect(states).toEqual({ "notes.md": "changed", "brand-new.typ": "missing" });
    expect((await recovery.list("/elsewhere")).entries).toHaveLength(1);
    expect((await recovery.discard(DEMO_ROOT)).removed).toBe(2);
    expect((await recovery.list("/elsewhere")).entries).toHaveLength(1);
  });

  it("refuses oversized text and paths outside the project", async () => {
    const { recovery } = await rig();
    const big = await recovery.write({
      project: DEMO_ROOT,
      path: "big.typ",
      kind: "text",
      content: "x".repeat(MOCK_MAX_SNAPSHOT_BYTES + 1),
      savedHash: null,
    });
    expect(big).toMatchObject({ stored: false, reason: "too-large" });
    await expect(
      recovery.write({
        project: DEMO_ROOT,
        path: "../x",
        kind: "text",
        content: "",
        savedHash: null,
      }),
    ).rejects.toBeDefined();
  });
});

describe("checked writes in the mock project", () => {
  it("refuses a save when the file changed outside and accepts it when not", async () => {
    const { kernel, project } = await rig();
    const read = await project.readText("notes.md");
    const first = await project.writeFile("notes.md", "mine\n", read.hash);
    await kernel.run("dev.write-outside", { path: "notes.md", content: "theirs\n" });
    await expect(
      project.writeFile("notes.md", "mine 2\n", first.hash),
    ).rejects.toMatchObject({
      message: expect.stringMatching(/^\[conflict\]/) as unknown,
    });
    expect((await project.readText("notes.md")).content).toBe("theirs\n");
    await kernel.run("dev.delete-outside", { path: "notes.md" });
    await expect(project.writeFile("notes.md", "x", first.hash)).rejects.toMatchObject({
      message: expect.stringContaining("deleted") as unknown,
    });
    await expect(project.writeFile("notes.md", "x", "absent")).resolves.toBeDefined();
    await expect(project.writeFile("notes.md", "y", "absent")).rejects.toBeDefined();
  });
});
