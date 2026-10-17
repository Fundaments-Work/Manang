import { describe, expect, it } from "vitest";

import { projectApi, typstApi } from "./api";
import { DEMO_ROOT, createDevKernel } from "./dev";

describe("dev kernel: project registry", () => {
  it("lists every known project with metadata, archived included", async () => {
    const project = projectApi(createDevKernel());
    const all = await project.list({ include_archived: true });
    expect(all.map((p) => p.name)).toEqual(
      expect.arrayContaining([
        "Entropy paper",
        "Lecture notes",
        "Thesis chapter 3",
        "Conference poster",
        "Old thesis",
      ]),
    );
    const entropy = all.find((p) => p.path === DEMO_ROOT);
    expect(entropy).toMatchObject({
      pinned: true,
      engine: "typst",
      exists: true,
      has_thumbnail: false,
    });
    expect(all.find((p) => p.name === "Old thesis")?.exists).toBe(false);
    expect(all.find((p) => p.name === "Conference poster")?.archived).toBe(true);
  });

  it("updates, forgets and re-registers without touching files", async () => {
    const project = projectApi(createDevKernel());
    const updated = await project.update(DEMO_ROOT, {
      name: "  Renamed ",
      pinned: false,
      tags: ["a", " a ", "b"],
    });
    expect(updated).toMatchObject({ name: "Renamed", pinned: false, tags: ["a", "b"] });
    await expect(project.update(DEMO_ROOT, { name: " " })).rejects.toMatchObject({
      message: expect.stringContaining("name cannot be empty"),
    });
    expect(await project.forget(DEMO_ROOT)).toEqual({ removed: true });
    expect(
      (await project.list({ include_archived: true })).map((p) => p.path),
    ).not.toContain(DEMO_ROOT);
    // The folder is still there: registering brings it back.
    expect((await project.register(DEMO_ROOT)).path).toBe(DEMO_ROOT);
    await expect(project.register("/nowhere")).rejects.toMatchObject({
      message: expect.stringContaining("not a Manang project"),
    });
  });

  it("opening a project updates its last-opened time", async () => {
    const project = projectApi(createDevKernel());
    const before = (await project.list()).find((p) => p.path === DEMO_ROOT);
    await project.open(DEMO_ROOT);
    const after = (await project.list()).find((p) => p.path === DEMO_ROOT);
    expect((after?.last_opened ?? 0) > (before?.last_opened ?? 0)).toBe(true);
  });

  it("stores and returns a thumbnail as bytes", async () => {
    const kernel = createDevKernel();
    const project = projectApi(kernel);
    await project.open(DEMO_ROOT);
    await typstApi(kernel).compile({ main: "main.typ", root: DEMO_ROOT });
    const page = await typstApi(kernel).render(0);
    await project.setThumbnail(DEMO_ROOT, new TextDecoder().decode(page));
    const entry = (await project.list()).find((p) => p.path === DEMO_ROOT);
    expect(entry?.has_thumbnail).toBe(true);
    expect(new TextDecoder().decode(await project.thumbnail(DEMO_ROOT))).toContain(
      "<svg",
    );
    await expect(project.setThumbnail(DEMO_ROOT, "hello")).rejects.toMatchObject({
      message: expect.stringContaining("not an SVG"),
    });
  });
});
