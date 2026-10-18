import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { diagramApi } from "$lib/kernel/api";
import { createDevKernel } from "$lib/kernel/dev";

import {
  DIAGRAM_DEBOUNCE_MS,
  DiagramPreview,
  type DiagramSnapshot,
} from "./diagramPreview";

const GOOD = "flowchart TD\n  A[One] --> B[Two]\n";
const BAD = "flowchart TD\n  A[One] -->>> B[Two]\n";
const PATH = "figures/flow.mmd";

function setup() {
  const api = diagramApi(createDevKernel());
  const render = vi.spyOn(api, "render");
  let next = 0;
  const revoked: string[] = [];
  const urls = {
    current: null as string | null,
    set() {
      if (this.current !== null) revoked.push(this.current);
      next += 1;
      this.current = `blob:${next}`;
      return this.current;
    },
    release() {
      if (this.current !== null) revoked.push(this.current);
      this.current = null;
    },
  };
  const seen: DiagramSnapshot[] = [];
  const errors: unknown[] = [];
  const preview = new DiagramPreview({
    api,
    urls,
    onChange: (s) => seen.push(s),
    onError: (e) => errors.push(e),
  });
  const show = (source: string, theme: "dark" | "light" = "dark", path = PATH) =>
    preview.show({ path, language: "mermaid", source, theme });
  return { preview, render, revoked, show, seen, errors };
}

/** Let timers fire and the mock kernel's promises settle. */
async function settle(ms = 0): Promise<void> {
  await vi.advanceTimersByTimeAsync(ms);
  await vi.advanceTimersByTimeAsync(0);
}

describe("diagram preview", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders a newly shown file at once", async () => {
    const { preview, show } = setup();
    show(GOOD);
    expect(preview.snapshot).toMatchObject({ path: PATH, rendering: true });
    await settle();
    expect(preview.snapshot).toMatchObject({
      url: "blob:1",
      theme: "dark",
      stale: false,
      error: "",
      rendering: false,
      problems: [],
    });
  });

  it("debounces edits into one render", async () => {
    const { preview, render, show } = setup();
    show(GOOD);
    await settle();
    render.mockClear();
    show(`${GOOD}  B --> C\n`);
    show(`${GOOD}  B --> C[Three]\n`);
    await settle(DIAGRAM_DEBOUNCE_MS - 10);
    expect(render).not.toHaveBeenCalled();
    await settle(20);
    expect(render).toHaveBeenCalledTimes(1);
    expect(preview.snapshot.url).toBe("blob:2");
  });

  it("keeps the last good picture, marked stale, while the source has an error", async () => {
    const { preview, show, revoked, errors } = setup();
    show(GOOD);
    await settle();
    show(BAD);
    await settle(DIAGRAM_DEBOUNCE_MS);
    expect(preview.snapshot).toMatchObject({
      url: "blob:1",
      stale: true,
      error: expect.stringMatching(/^Line 2: unexpected token/) as unknown,
    });
    expect(preview.snapshot.problems).toMatchObject([
      { file: PATH, line: 2, column: 10, severity: "error", source: "diagram" },
    ]);
    expect(revoked).toEqual([]);
    expect(errors).toEqual([]);

    show(GOOD.replace("Two", "Two again"));
    await settle(DIAGRAM_DEBOUNCE_MS);
    expect(preview.snapshot).toMatchObject({ url: "blob:2", stale: false, error: "" });
    expect(preview.snapshot.problems).toEqual([]);
    expect(revoked).toEqual(["blob:1"]);
  });

  it("re-renders at once when the theme changes", async () => {
    const { preview, show, render } = setup();
    show(GOOD);
    await settle();
    show(GOOD, "light");
    await settle();
    expect(render).toHaveBeenLastCalledWith("mermaid", GOOD, "light");
    expect(preview.snapshot.theme).toBe("light");
  });

  it("does not render unchanged input twice, but Refresh does", async () => {
    const { preview, show, render } = setup();
    show(GOOD);
    await settle();
    show(GOOD);
    await settle(DIAGRAM_DEBOUNCE_MS);
    expect(render).toHaveBeenCalledTimes(1);
    preview.refresh();
    await settle();
    expect(render).toHaveBeenCalledTimes(2);
  });

  it("drops the old picture when another file is shown, and clears", async () => {
    const { preview, show, revoked } = setup();
    show(GOOD);
    await settle();
    show(BAD, "dark", "figures/other.mmd");
    expect(preview.snapshot.url).toBeNull();
    await settle();
    expect(preview.snapshot).toMatchObject({ path: "figures/other.mmd", url: null });
    expect(preview.snapshot.error).not.toBe("");
    preview.show(null);
    expect(preview.snapshot.path).toBeNull();
    expect(revoked).toEqual(["blob:1"]);
  });

  it("reports kernel trouble that is not the diagram's fault", async () => {
    const { preview, show, errors, render } = setup();
    render.mockRejectedValueOnce(new Error("[worker_crashed] gone"));
    show(GOOD);
    await settle();
    expect(errors).toHaveLength(1);
    expect(preview.snapshot.error).toBe("gone");
  });
});
