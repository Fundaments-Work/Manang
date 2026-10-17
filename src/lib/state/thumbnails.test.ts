import { describe, expect, it } from "vitest";

import { kernel } from "$lib/kernel";
import { projectApi } from "$lib/kernel/api";
import { DEMO_ROOT } from "$lib/kernel/dev";

import {
  THUMBNAIL_INTERVAL_MS,
  captureThumbnail,
  due,
  resetThumbnailClock,
} from "./thumbnails";

describe("thumbnail throttle", () => {
  it("is due the first time and once the interval has passed", () => {
    expect(due(undefined, 5)).toBe(true);
    expect(due(1000, 1000 + THUMBNAIL_INTERVAL_MS - 1)).toBe(false);
    expect(due(1000, 1000 + THUMBNAIL_INTERVAL_MS)).toBe(true);
  });

  it("stores page 1 once per interval per project", async () => {
    resetThumbnailClock();
    const api = projectApi(kernel);
    await api.open(DEMO_ROOT);
    const { typstApi } = await import("$lib/kernel/api");
    await typstApi(kernel).compile({ main: "main.typ", root: DEMO_ROOT });

    expect(await captureThumbnail(DEMO_ROOT, 10_000)).toBe(true);
    expect(await captureThumbnail(DEMO_ROOT, 10_000 + 30_000)).toBe(false);
    expect(await captureThumbnail(DEMO_ROOT, 10_000 + THUMBNAIL_INTERVAL_MS)).toBe(
      true,
    );
    const entry = (await api.list()).find((p) => p.path === DEMO_ROOT);
    expect(entry?.has_thumbnail).toBe(true);
  });

  it("does not burn the interval when rendering fails", async () => {
    resetThumbnailClock();
    const api = projectApi(kernel);
    await api.close();
    // No project, no compiled document: nothing to render.
    expect(await captureThumbnail("/nowhere", 1)).toBe(false);
    expect(await captureThumbnail("/nowhere", 2)).toBe(false);
  });
});
